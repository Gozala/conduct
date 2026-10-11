//! Surface syntax: s-expressions parsed into expressions that still
//! refer to things by name. Resolution (in `codebase`) turns these into
//! name-free terms that can be hashed.

use crate::reader::{Sexp, Span};
use std::fmt;

#[derive(Clone, Debug)]
pub enum Expr {
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
    Var(String, Span),
    Fn(Vec<String>, Box<Expr>),
    App(Box<Expr>, Vec<Expr>, Span),
    Let(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `(if-some [x e] then else)`: `x` is bound to `e` in `then`, with
    /// `nil` removed from its type.
    IfSome(String, Box<Expr>, Box<Expr>, Box<Expr>),
    /// `(?? a b)`: `a` unless it is `nil`, then `b`.
    Coalesce(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Record(Vec<(String, Expr)>),
    Field(String, Box<Expr>),
    /// `(assoc record :field value)`: a copy of a record with one field set.
    Update(Box<Expr>, String, Box<Expr>),
    Dict(Vec<(String, Expr)>),
    List(Vec<Expr>),
}

/// A top-level definition, before resolution.
#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.span.start)
    }
}

impl std::error::Error for SyntaxError {}

type Result<T> = std::result::Result<T, SyntaxError>;

fn err<T>(message: impl Into<String>, span: Span) -> Result<T> {
    Err(SyntaxError { message: message.into(), span })
}

/// Words that cannot name a definition or a local.
const RESERVED: &[&str] =
    &["def", "defn", "fn", "let", "if", "if-some", "??", "and", "or", "cond", "nil", "true", "false"];

/// Parse top-level forms into definitions.
pub fn defs(forms: &[Sexp]) -> Result<Vec<Def>> {
    forms.iter().map(def).collect()
}

fn def(form: &Sexp) -> Result<Def> {
    let Sexp::List(items, span) = form else {
        return err("a top-level form must be `(def name expr)` or `(defn name [params] body)`", form.span());
    };
    match items.first().and_then(Sexp::as_symbol) {
        Some("def") => {
            let [_, name, body] = items.as_slice() else {
                return err("`def` takes a name and one expression", *span);
            };
            Ok(Def { name: binder(name)?, body: expr(body)?, span: *span })
        }
        Some("defn") => {
            let [_, name, params, body] = items.as_slice() else {
                return err("`defn` takes a name, a parameter vector, and one body expression", *span);
            };
            let params = param_vector(params)?;
            Ok(Def { name: binder(name)?, body: Expr::Fn(params, Box::new(expr(body)?)), span: *span })
        }
        _ => err("a top-level form must be `(def name expr)` or `(defn name [params] body)`", *span),
    }
}

fn binder(form: &Sexp) -> Result<String> {
    match form {
        Sexp::Symbol(name, span) => {
            if RESERVED.contains(&name.as_str()) {
                return err(format!("`{name}` is reserved and cannot be bound"), *span);
            }
            Ok(name.clone())
        }
        other => err(format!("expected a name, found `{other}`"), other.span()),
    }
}

fn param_vector(form: &Sexp) -> Result<Vec<String>> {
    let Sexp::Vector(items, _) = form else {
        return err(format!("expected a parameter vector, found `{form}`"), form.span());
    };
    let params: Vec<String> = items.iter().map(binder).collect::<Result<_>>()?;
    for (i, p) in params.iter().enumerate() {
        if params[..i].contains(p) {
            return err(format!("parameter `{p}` appears twice"), form.span());
        }
    }
    Ok(params)
}

fn boxed(form: &Sexp) -> Result<Box<Expr>> {
    expr(form).map(Box::new)
}

pub fn expr(form: &Sexp) -> Result<Expr> {
    match form {
        Sexp::Int(n, _) => Ok(Expr::Int(*n)),
        Sexp::Str(s, _) => Ok(Expr::Str(s.clone())),
        Sexp::Symbol(name, span) => Ok(match name.as_str() {
            "nil" => Expr::Nil,
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            _ => Expr::Var(name.clone(), *span),
        }),
        Sexp::Keyword(k, span) => err(format!("a keyword is not a value; use `(:{k} record)` to read a field"), *span),
        Sexp::Vector(items, _) => Ok(Expr::List(items.iter().map(expr).collect::<Result<_>>()?)),
        Sexp::Map(items, span) => map_literal(items, *span),
        Sexp::List(items, span) => list_form(items, *span),
    }
}

fn map_literal(items: &[Sexp], span: Span) -> Result<Expr> {
    if !items.len().is_multiple_of(2) {
        return err("a map needs an even number of forms", span);
    }
    let pairs: Vec<(&Sexp, &Sexp)> = items.chunks(2).map(|c| (&c[0], &c[1])).collect();
    // An empty map is a dictionary: a record needs at least one field.
    if pairs.is_empty() || matches!(pairs[0].0, Sexp::Str(..)) {
        let mut entries: Vec<(String, Expr)> = Vec::new();
        for (k, v) in pairs {
            let Sexp::Str(key, _) = k else {
                return err("a dictionary's keys must all be strings", k.span());
            };
            let value = expr(v)?;
            if let Some(slot) = entries.iter_mut().find(|(name, _)| name == key) {
                slot.1 = value;
            } else {
                entries.push((key.clone(), value));
            }
        }
        return Ok(Expr::Dict(entries));
    }
    let mut fields: Vec<(String, Expr)> = Vec::new();
    for (k, v) in pairs {
        let Sexp::Keyword(name, kspan) = k else {
            return err("a record's keys must all be keywords", k.span());
        };
        if fields.iter().any(|(n, _)| n == name) {
            return err(format!("field `:{name}` appears twice"), *kspan);
        }
        fields.push((name.clone(), expr(v)?));
    }
    Ok(Expr::Record(fields))
}

fn list_form(items: &[Sexp], span: Span) -> Result<Expr> {
    let Some(head) = items.first() else {
        return err("`()` is not an expression; use `nil` or `[]`", span);
    };
    let args = &items[1..];
    if let Sexp::Keyword(field, _) = head {
        let [record] = args else {
            return err(format!("`(:{field} record)` takes one argument"), span);
        };
        return Ok(Expr::Field(field.clone(), boxed(record)?));
    }
    if let Some(name) = head.as_symbol() {
        match name {
            "fn" => {
                let [params, body] = args else {
                    return err("`fn` takes a parameter vector and one body expression", span);
                };
                return Ok(Expr::Fn(param_vector(params)?, boxed(body)?));
            }
            "let" => {
                let [bindings, body] = args else {
                    return err("`let` takes a binding vector and one body expression", span);
                };
                let Sexp::Vector(bs, bspan) = bindings else {
                    return err("`let` bindings go in a vector", bindings.span());
                };
                if bs.len() % 2 != 0 {
                    return err("`let` bindings come in pairs", *bspan);
                }
                let mut out = expr(body)?;
                for pair in bs.chunks(2).rev() {
                    out = Expr::Let(binder(&pair[0])?, boxed(&pair[1])?, Box::new(out));
                }
                return Ok(out);
            }
            "if" => {
                let [c, t, e] = args else {
                    return err("`if` takes a condition, a then branch, and an else branch", span);
                };
                return Ok(Expr::If(boxed(c)?, boxed(t)?, boxed(e)?));
            }
            "if-some" => {
                let [binding, t, e] = args else {
                    return err("`if-some` takes `[name expr]`, a then branch, and an else branch", span);
                };
                let Sexp::Vector(b, bspan) = binding else {
                    return err("`if-some` binds with `[name expr]`", binding.span());
                };
                let [name, value] = b.as_slice() else {
                    return err("`if-some` binds exactly one name", *bspan);
                };
                return Ok(Expr::IfSome(binder(name)?, boxed(value)?, boxed(t)?, boxed(e)?));
            }
            "??" => {
                let [a, b] = args else {
                    return err("`??` takes two arguments", span);
                };
                return Ok(Expr::Coalesce(boxed(a)?, boxed(b)?));
            }
            "and" | "or" => {
                if args.len() < 2 {
                    return err(format!("`{name}` takes at least two arguments"), span);
                }
                let mut parts = args.iter().map(expr).collect::<Result<Vec<_>>>()?;
                let mut out = parts.pop().unwrap();
                while let Some(prev) = parts.pop() {
                    out = if name == "and" {
                        Expr::And(Box::new(prev), Box::new(out))
                    } else {
                        Expr::Or(Box::new(prev), Box::new(out))
                    };
                }
                return Ok(out);
            }
            "cond" => {
                if args.is_empty() || !args.len().is_multiple_of(2) {
                    return err("`cond` takes pairs of a test and a result, ending with `:else`", span);
                }
                let pairs: Vec<&[Sexp]> = args.chunks(2).collect();
                let (last, rest) = pairs.split_last().unwrap();
                if !matches!(&last[0], Sexp::Keyword(k, _) if k == "else") {
                    return err("`cond` must end with an `:else` clause", last[0].span());
                }
                let mut out = expr(&last[1])?;
                for pair in rest.iter().rev() {
                    out = Expr::If(boxed(&pair[0])?, boxed(&pair[1])?, Box::new(out));
                }
                return Ok(out);
            }
            _ => {}
        }
        if name == "assoc"
            && let [record, Sexp::Keyword(field, _), value] = args
        {
            return Ok(Expr::Update(boxed(record)?, field.clone(), boxed(value)?));
        }
    }
    let callee = expr(head)?;
    let args = args.iter().map(expr).collect::<Result<Vec<_>>>()?;
    Ok(Expr::App(Box::new(callee), args, span))
}
