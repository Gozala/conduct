//! Surface syntax: s-expressions parsed into expressions that still
//! refer to things by name. Resolution (in `codebase`) turns these into
//! name-free terms that can be hashed.

use crate::reader::{Sexp, Span};
pub use crate::term::Pattern;
use std::fmt;

/// A parameter: the label a caller passes it by, and the name the body
/// knows it by. `(define (greet who greeting: g) ..)` has the parameters
/// `who` (label and local alike) and `greeting` (local `g`).
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub label: String,
    pub local: String,
}

/// An argument of a call: the first may come without a label.
#[derive(Clone, Debug)]
pub struct Arg {
    pub label: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Int(i64),
    Str(String),
    Bool(bool),
    Var(String, Span),
    Fn(Vec<Param>, Box<Expr>),
    App(Box<Expr>, Vec<Arg>, Span),
    Let(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    /// `{name: "Ada"}`: keys as written, a claimed attribute's local name
    /// or a global one.
    Record(Vec<(String, Expr)>, Span),
    /// `(assoc record name: value ..)`: a copy of a record with
    /// attributes set.
    Update(Box<Expr>, Vec<(String, Expr)>, Span),
    Dict(Vec<(String, Expr)>),
    List(Vec<Expr>),
    /// `:active`: a keyword, a value whose type is that keyword alone.
    Keyword(String),
    /// `(match e p1 r1 p2 r2 ... _ r)`: the result of the first pattern
    /// `e` matches; `_` matches anything. Without `_`, the patterns must
    /// be able to cover every value (see [`Pattern::cover`]). Record
    /// patterns' keys are as written, resolved with the expression.
    Match(Box<Expr>, Vec<(Pattern, Expr)>, Option<Box<Expr>>, Span),
}

/// A top-level definition, before resolution.
#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub body: Expr,
    pub span: Span,
}

/// A type, as a claim writes it.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpr {
    /// `text`, `integer`, a claimed name.
    Name(String, Span),
    /// `?a`.
    Var(String),
    /// `(list text)`.
    App(String, Vec<TypeExpr>, Span),
    /// `[:active :completed]`.
    Keywords(Vec<String>),
    /// `{}`.
    Empty,
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpr::Name(n, _) => write!(f, "{n}"),
            TypeExpr::Var(v) => write!(f, "?{v}"),
            TypeExpr::App(head, args, _) => {
                write!(f, "({head}")?;
                args.iter().try_for_each(|a| write!(f, " {a}"))?;
                write!(f, ")")
            }
            TypeExpr::Keywords(ks) => {
                write!(f, "[{}]", ks.iter().map(|k| format!(":{k}")).collect::<Vec<_>>().join(" "))
            }
            TypeExpr::Empty => write!(f, "{{}}"),
        }
    }
}

/// `(claim name : (io.gozala.person/name text))`: the attribute
/// `io.gozala.person/name`, known locally as `name`, whose values are
/// `text`.
#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: String,
    pub global: String,
    pub ty: TypeExpr,
    pub span: Span,
}

/// A top-level form.
#[derive(Clone, Debug)]
pub enum Item {
    Def(Def),
    Attribute(Attribute),
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
    &["define", "claim", "fn", "let", "do", "if", "and", "or", "cond", "match", "nil", "true", "false"];

/// What to write instead of the forms conduct no longer has.
fn retired(name: &str) -> Option<&'static str> {
    Some(match name {
        "nil" => {
            "conduct has no `nil`: leave an attribute out of a record, return a variant such as \
             `{error: :not-found}`, or use `[]` for no children"
        }
        "??" => "conduct has no `??`: read an attribute with a default, `(title todo \"\")`, or `(get d k default)`",
        "if-some" => "conduct has no `if-some`: `match` the value, `(match r {title: t} .. _ ..)`",
        "nil?" | "some?" => "conduct has no `nil`: `match` on whether an attribute is there, `{title: t}`",
        "def" => "`def` is now `define`: `(define name value)`",
        "defn" => "`defn` is now `define`: `(define (name first second) body)`, called `(name 1 second: 2)`",
        _ => return None,
    })
}

/// Parse top-level forms.
pub fn items(forms: &[Sexp]) -> Result<Vec<Item>> {
    forms.iter().map(item).collect()
}

const TOP: &str = "a top-level form is `(define name value)`, `(define (name params ..) body)`, or a `claim`";

fn item(form: &Sexp) -> Result<Item> {
    let Sexp::List(items, span) = form else {
        return err(TOP, form.span());
    };
    match items.first().and_then(Sexp::as_symbol) {
        Some("define") => define(&items[1..], *span).map(Item::Def),
        Some("claim") => claim(&items[1..], *span),
        Some(name) if retired(name).is_some() => err(retired(name).unwrap(), *span),
        _ => err(TOP, *span),
    }
}

fn define(args: &[Sexp], span: Span) -> Result<Def> {
    match args {
        [Sexp::Symbol(..), value] => Ok(Def { name: binder(&args[0])?, body: expr(value)?, span }),
        [Sexp::Symbol(..), ..] => err("`(define name value)` takes one value", span),
        [Sexp::List(head, hspan), body @ ..] => {
            let Some((name, params)) = head.split_first() else {
                return err("`(define (name params ..) body)` needs a name", *hspan);
            };
            let name = binder(name)?;
            if params.is_empty() {
                return err(format!("`{name}` takes no parameters, so it is a value: `(define {name} value)`"), *hspan);
            }
            let params = parameters(params, *hspan)?;
            Ok(Def { name, body: Expr::Fn(params, Box::new(sequence(body, span)?)), span })
        }
        _ => err("`define` takes a name and a value, or `(name params ..)` and a body", span),
    }
}

/// `first second label: local ..`: the first parameter, then the
/// others, each a name or a label and the name the body uses.
fn parameters(items: &[Sexp], span: Span) -> Result<Vec<Param>> {
    let mut params: Vec<Param> = Vec::new();
    let mut rest = items.iter();
    while let Some(item) = rest.next() {
        let param = match item {
            Sexp::Symbol(..) => {
                let name = binder(item)?;
                Param { label: name.clone(), local: name }
            }
            Sexp::Label(label, lspan) if !params.is_empty() => {
                let Some(local) = rest.next() else {
                    return err(format!("the label `{label}:` needs the name its parameter is known by"), *lspan);
                };
                Param { label: label.clone(), local: binder(local)? }
            }
            Sexp::Label(_, lspan) => {
                return err("the first parameter is passed without a label, so it is a name alone", *lspan);
            }
            other => return err(format!("expected a parameter, found `{other}`"), other.span()),
        };
        if params.iter().any(|p| p.label == param.label) {
            return err(format!("the label `{}:` appears twice", param.label), span);
        }
        if params.iter().any(|p| p.local == param.local) {
            return err(format!("parameter `{}` appears twice", param.local), span);
        }
        params.push(param);
    }
    Ok(params)
}

fn claim(args: &[Sexp], span: Span) -> Result<Item> {
    let [Sexp::Symbol(..), Sexp::Label(colon, _), ty] = args else {
        return err("a claim is `(claim name : type)`", span);
    };
    if !colon.is_empty() {
        return err("a claim is `(claim name : type)`, with `:` before the type", span);
    }
    let name = binder(&args[0])?;
    match ty {
        Sexp::List(items, tspan) => match items.as_slice() {
            [Sexp::Symbol(global, _), value] if global.contains('/') => {
                Ok(Item::Attribute(Attribute { name, global: global.clone(), ty: type_expr(value)?, span: *tspan }))
            }
            _ => err("this claim is an attribute, `(domain/name type)`; claims of other types come later", *tspan),
        },
        other => err("an attribute is claimed as `(domain/name type)`", other.span()),
    }
}

fn type_expr(form: &Sexp) -> Result<TypeExpr> {
    Ok(match form {
        Sexp::Symbol(name, _) if name.starts_with('?') && name.len() > 1 => TypeExpr::Var(name[1..].to_string()),
        Sexp::Symbol(name, _) if name == "?" => {
            return err("a type variable has a name, `?a`, so that its uses can be tied together", form.span());
        }
        Sexp::Symbol(name, span) => TypeExpr::Name(name.clone(), *span),
        Sexp::List(items, span) => match items.split_first() {
            Some((Sexp::Symbol(head, _), args)) if !args.is_empty() => {
                TypeExpr::App(head.clone(), args.iter().map(type_expr).collect::<Result<_>>()?, *span)
            }
            _ => return err(format!("expected a type, found `{form}`"), *span),
        },
        Sexp::Vector(items, _) => TypeExpr::Keywords(
            items
                .iter()
                .map(|k| match k {
                    Sexp::Keyword(k, _) => Ok(k.clone()),
                    other => {
                        err("a list of keywords, `[:a :b]`, is the only union an attribute takes yet", other.span())
                    }
                })
                .collect::<Result<_>>()?,
        ),
        Sexp::Map(items, _) if items.is_empty() => TypeExpr::Empty,
        other => return err(format!("expected a type, found `{other}`"), other.span()),
    })
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

fn boxed(form: &Sexp) -> Result<Box<Expr>> {
    expr(form).map(Box::new)
}

/// Forms evaluated in order: the value of the last.
fn sequence(forms: &[Sexp], span: Span) -> Result<Expr> {
    let Some((last, before)) = forms.split_last() else {
        return err("expected a body", span);
    };
    let mut out = expr(last)?;
    for form in before.iter().rev() {
        out = Expr::Let(" ".into(), boxed(form)?, Box::new(out));
    }
    Ok(out)
}

pub fn expr(form: &Sexp) -> Result<Expr> {
    match form {
        Sexp::Int(n, _) => Ok(Expr::Int(*n)),
        Sexp::Str(s, _) => Ok(Expr::Str(s.clone())),
        Sexp::Symbol(name, span) => Ok(match name.as_str() {
            n if retired(n).is_some() => return err(retired(n).unwrap(), *span),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            _ => Expr::Var(name.clone(), *span),
        }),
        Sexp::Keyword(k, _) => Ok(Expr::Keyword(k.clone())),
        Sexp::Label(l, span) => {
            err(format!("the label `{l}:` names an argument or attribute; a value follows it"), *span)
        }
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
        let name = match k {
            Sexp::Label(name, _) if !name.is_empty() => name,
            Sexp::Keyword(name, kspan) => {
                return err(format!("a record's keys are attributes: write `{name}:`, not `:{name}`"), *kspan);
            }
            other => return err("a record's keys are attributes, `name:`", other.span()),
        };
        if fields.iter().any(|(n, _)| n == name) {
            return err(format!("the attribute `{name}:` appears twice"), k.span());
        }
        fields.push((name.clone(), expr(v)?));
    }
    Ok(Expr::Record(fields, span))
}

/// `label: value ..` pairs, as `assoc` takes them.
fn labeled(args: &[Sexp], span: Span) -> Result<Vec<(String, Expr)>> {
    if !args.len().is_multiple_of(2) {
        return err("expected `label: value` pairs", span);
    }
    let mut out: Vec<(String, Expr)> = Vec::new();
    for pair in args.chunks(2) {
        let Sexp::Label(name, lspan) = &pair[0] else {
            return err(format!("expected a label, found `{}`", pair[0]), pair[0].span());
        };
        if out.iter().any(|(n, _)| n == name) {
            return err(format!("the label `{name}:` appears twice"), *lspan);
        }
        out.push((name.clone(), expr(&pair[1])?));
    }
    Ok(out)
}

/// A call's arguments: values, each after its label if it has one.
fn arguments(args: &[Sexp]) -> Result<Vec<Arg>> {
    let mut out: Vec<Arg> = Vec::new();
    let mut rest = args.iter();
    while let Some(item) = rest.next() {
        match item {
            Sexp::Label(label, lspan) => {
                let Some(value) = rest.next() else {
                    return err(format!("the label `{label}:` needs a value"), *lspan);
                };
                if out.iter().any(|a| a.label.as_deref() == Some(label)) {
                    return err(format!("the label `{label}:` appears twice"), *lspan);
                }
                out.push(Arg { label: Some(label.clone()), value: expr(value)? });
            }
            value => out.push(Arg { label: None, value: expr(value)? }),
        }
    }
    Ok(out)
}

fn list_form(items: &[Sexp], span: Span) -> Result<Expr> {
    let Some(head) = items.first() else {
        return err("`()` is not an expression; use `[]` for an empty list", span);
    };
    let args = &items[1..];
    if let Sexp::Keyword(field, _) = head {
        return err(format!("read an attribute by calling it: `({field} record)`, not `(:{field} record)`"), span);
    }
    if let Some(name) = head.as_symbol() {
        match name {
            n if retired(n).is_some() => return err(retired(n).unwrap(), span),
            "define" | "claim" => return err(format!("`{name}` is only allowed at the top level"), span),
            "fn" => {
                let [Sexp::Vector(params, pspan), body @ ..] = args else {
                    return err("`fn` takes a parameter vector and a body", span);
                };
                if params.is_empty() {
                    return err("a function takes at least one parameter", *pspan);
                }
                return Ok(Expr::Fn(parameters(params, *pspan)?, Box::new(sequence(body, span)?)));
            }
            "do" => return sequence(args, span),
            "let" => {
                let [bindings, body @ ..] = args else {
                    return err("`let` takes a binding vector and a body", span);
                };
                let Sexp::Vector(bs, bspan) = bindings else {
                    return err("`let` bindings go in a vector", bindings.span());
                };
                if bs.len() % 2 != 0 {
                    return err("`let` bindings come in pairs", *bspan);
                }
                let mut out = sequence(body, span)?;
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
            "match" => return match_form(args, span),
            _ => {}
        }
        if name == "assoc"
            && let [record, Sexp::Label(..), ..] = args
        {
            return Ok(Expr::Update(boxed(record)?, labeled(&args[1..], span)?, span));
        }
    }
    let callee = expr(head)?;
    Ok(Expr::App(Box::new(callee), arguments(args)?, span))
}

fn match_form(args: &[Sexp], span: Span) -> Result<Expr> {
    let Some((scrutinee, clauses)) = args.split_first() else {
        return err("`match` takes a value and pairs of a pattern and a result", span);
    };
    if clauses.is_empty() || !clauses.len().is_multiple_of(2) {
        return err("`match` takes a value and pairs of a pattern and a result", span);
    }
    let mut arms: Vec<(Pattern, Expr)> = Vec::new();
    let mut default = None;
    let pairs: Vec<&[Sexp]> = clauses.chunks(2).collect();
    for (i, pair) in pairs.iter().enumerate() {
        if let Sexp::Symbol(s, sspan) = &pair[0]
            && s == "_"
        {
            if i + 1 != pairs.len() {
                return err("`_` matches anything, so it must be the last clause of a `match`", *sspan);
            }
            default = Some(boxed(&pair[1])?);
            continue;
        }
        let pattern = pattern(&pair[0])?;
        let mut names = pattern.binders();
        names.sort();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            return err(format!("the pattern binds `{}` twice", w[0]), pair[0].span());
        }
        arms.push((pattern, expr(&pair[1])?));
    }
    if arms.is_empty() {
        return err("a `match` needs a pattern besides `_`", span);
    }
    Ok(Expr::Match(boxed(scrutinee)?, arms, default, span))
}

fn pattern(form: &Sexp) -> Result<Pattern> {
    Ok(match form {
        Sexp::Keyword(k, _) => Pattern::Keyword(k.clone()),
        Sexp::Str(s, _) => Pattern::Str(s.clone()),
        Sexp::Int(n, _) => Pattern::Int(*n),
        Sexp::Symbol(s, _) if s == "_" => Pattern::Any,
        Sexp::Symbol(s, sspan) => {
            if let Some(why) = retired(s) {
                return err(why, *sspan);
            }
            if RESERVED.contains(&s.as_str()) {
                return err(format!("`{s}` is reserved, so it cannot name a value a pattern binds"), *sspan);
            }
            Pattern::Bind(s.clone())
        }
        Sexp::Map(items, _) if !items.is_empty() && items.len().is_multiple_of(2) => {
            let mut fields: Vec<(String, Pattern)> = Vec::new();
            for pair in items.chunks(2) {
                let name = match &pair[0] {
                    Sexp::Label(name, _) if !name.is_empty() => name,
                    Sexp::Keyword(name, kspan) => {
                        return err(format!("a record pattern's keys are attributes: write `{name}:`"), *kspan);
                    }
                    other => return err("a record pattern's keys are attributes, `name:`", other.span()),
                };
                if fields.iter().any(|(n, _)| n == name) {
                    return err(format!("the attribute `{name}:` appears twice"), pair[0].span());
                }
                fields.push((name.clone(), pattern(&pair[1])?));
            }
            Pattern::Fields(fields)
        }
        other => {
            return err(
                format!(
                    "a `match` pattern is a keyword, string, or integer, `_`, a name to bind, or a record of those, not `{other}`"
                ),
                other.span(),
            );
        }
    })
}
