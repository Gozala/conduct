//! Surface syntax: s-expressions parsed into expressions that still
//! refer to things by name. Resolution (in `codebase`) turns these into
//! name-free terms that can be hashed.

use crate::reader::{Sexp, Span, bare, mark_of};
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
    /// `{name: "Ada"}`: keys as written, a declared attribute's local name
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
    Match(Box<Expr>, Vec<(Pat, Expr)>, Option<Box<Expr>>, Span),
    /// Work not done yet: the body of a definition or `fn!` marked `!`.
    Task(Box<Expr>),
    /// `(name! ..)`: perform the task the call answers.
    Perform(Box<Expr>),
    /// `(perform task with: {capability: {operation: implementation}})`.
    Handle(Box<Expr>, Vec<(String, Vec<(String, Expr)>)>),
    /// A function defined by clauses, each with patterns for its
    /// parameters: every `define` of one name in a compilation.
    Clauses(Vec<Clause>),
}

/// One clause of a function: a pattern for each parameter, the first
/// without a label, and the body for calls they match. A name alone
/// binds the argument, whatever it is.
#[derive(Clone, Debug)]
pub struct Clause {
    pub params: Vec<(Option<String>, Pat)>,
    pub body: Expr,
}

impl Clause {
    /// A function written with names alone, as a clause.
    pub fn of_fn(params: &[Param], body: &Expr) -> Clause {
        let params = params
            .iter()
            .enumerate()
            .map(|(i, p)| ((i > 0).then(|| p.label.clone()), Pat::Name(p.local.clone())))
            .collect();
        Clause { params, body: body.clone() }
    }
}

/// A pattern as written. Record keys and constructors are resolved
/// with the expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Keyword(String),
    Str(String),
    Int(i64),
    /// `true` or `false`.
    Bool(bool),
    /// `_`.
    Any,
    /// A name: it binds the value, unless it names a record type, which
    /// it then tests for.
    Name(String),
    /// `{title: t}`.
    Record(Vec<(String, Pat)>),
    /// `(ok value)`, `(done description: d)`: a record type, and patterns
    /// for its attributes, the first unlabeled.
    Ctor(String, Vec<(Option<String>, Pat)>),
    /// `[]`, `[x y]`, `[first rest ...]`: patterns for a list's first
    /// items and, after `...`, a name (or `_`) for the list of the rest.
    List(Vec<Pat>, Option<Option<String>>),
}

impl Pat {
    /// The names the pattern may bind (a name may turn out to be a type).
    pub fn binders(&self) -> Vec<String> {
        let mut out = Vec::new();
        fn go(p: &Pat, out: &mut Vec<String>) {
            match p {
                Pat::Name(n) => out.push(n.clone()),
                Pat::Record(fields) => fields.iter().for_each(|(_, p)| go(p, out)),
                Pat::Ctor(_, args) => args.iter().for_each(|(_, p)| go(p, out)),
                Pat::List(items, rest) => {
                    items.iter().for_each(|p| go(p, out));
                    if let Some(Some(r)) = rest {
                        out.push(r.clone());
                    }
                }
                _ => {}
            }
        }
        go(self, &mut out);
        out
    }
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
    /// `text`, `integer`, a declared name.
    Name(String, Span),
    /// `?a`.
    Var(String),
    /// `(list text)`, `(result text error: text)`: a type constructor
    /// applied to its arguments, the first unlabeled.
    App(String, Vec<(Option<String>, TypeExpr)>, Span),
    /// `(io.gozala.person/name text)`: an attribute, by its global name,
    /// and the type of its values.
    Attribute(String, Box<TypeExpr>, Span),
    /// `{name age ? phone}`: attributes, required and then optional.
    Record(Vec<Member>, usize),
    /// `[:active :done]`, `[(ok ?a) (error ?e)]`: cases, in rank order.
    Union(Vec<TypeExpr>),
    /// `:active` as one case of a union.
    Keyword(String),
    /// `((\ ?a each: ?b) with: {print} : ?c)`: a function's parameters,
    /// the first unlabeled, its capabilities, and its result.
    Fn(Vec<(Option<String>, TypeExpr)>, Option<Box<TypeExpr>>, Box<TypeExpr>),
    /// `{print model}` after `with:`: capabilities.
    Caps(Vec<String>),
}

/// A member of a record type: an attribute by its local name, or one
/// written in place.
#[derive(Clone, Debug, PartialEq)]
pub enum Member {
    Name(String, Span),
    Inline(String, TypeExpr),
}

impl fmt::Display for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Member::Name(n, _) => write!(f, "{n}"),
            Member::Inline(g, t) => write!(f, "({g} {t})"),
        }
    }
}

fn args_text(args: &[(Option<String>, TypeExpr)]) -> String {
    args.iter()
        .map(|(l, t)| match l {
            Some(l) => format!(" {l}: {t}"),
            None => format!(" {t}"),
        })
        .collect()
}

impl fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpr::Name(n, _) => write!(f, "{n}"),
            TypeExpr::Var(v) => write!(f, "?{v}"),
            TypeExpr::App(head, args, _) => write!(f, "({head}{})", args_text(args)),
            TypeExpr::Attribute(g, t, _) => write!(f, "({g} {t})"),
            TypeExpr::Record(members, optional) => {
                let mut parts: Vec<String> = members[..*optional].iter().map(|m| m.to_string()).collect();
                if *optional < members.len() {
                    parts.push("?".into());
                    parts.extend(members[*optional..].iter().map(|m| m.to_string()));
                }
                write!(f, "{{{}}}", parts.join(" "))
            }
            TypeExpr::Union(cases) => {
                write!(f, "[{}]", cases.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" "))
            }
            TypeExpr::Keyword(k) => write!(f, ":{k}"),
            TypeExpr::Fn(params, with, ret) => {
                write!(f, "((\\{})", args_text(params))?;
                if let Some(w) = with {
                    write!(f, " with: {w}")?;
                }
                write!(f, " : {ret})")
            }
            TypeExpr::Caps(caps) => write!(f, "{{{}}}", caps.join(" ")),
        }
    }
}

/// `(claim name : type)` or `(claim (name params ..) with: caps : type)`:
/// the signature of a `define` of the same name in the same source. A
/// `type` is written the same way, `(type name : type)` or
/// `(type (name ?a ..) : type)`, and declares what a name is: an
/// attribute, a record, a variant, or a constructor of one of those.
#[derive(Clone, Debug)]
pub struct Claim {
    pub name: String,
    /// Whether the name is written marked `!`: a definition that makes a
    /// task.
    pub performs: bool,
    /// The parameters, when the claim is written `(name params ..)`.
    pub params: Option<Vec<(Option<String>, TypeExpr)>>,
    pub with: Option<TypeExpr>,
    pub ty: TypeExpr,
    pub span: Span,
}

impl Claim {
    /// The claim as written, without its name: what a signature adds to
    /// its definition's hash.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(params) = &self.params {
            out.push_str(&format!("({}{})", self.name, args_text(params)));
        }
        if let Some(w) = &self.with {
            out.push_str(&format!(" with: {w}"));
        }
        out.push_str(&format!(" : {}", self.ty));
        out
    }
}

/// `(macro (name pattern ..) template)`: one rule of a macro. A call
/// whose forms match the pattern stands for the template, with the
/// pattern's variables replaced by what they matched.
#[derive(Clone, Debug)]
pub struct Macro {
    pub name: String,
    pub pattern: Vec<Sexp>,
    pub template: Sexp,
    pub span: Span,
}

/// A top-level form.
#[derive(Clone, Debug)]
pub enum Item {
    /// `(macro (name pattern ..) template)`.
    Macro(Macro),
    Def(Def),
    /// `(claim name : type)`: what type a definition has.
    Claim(Claim),
    /// `(type name : type)`: what a name is, an attribute, a record type,
    /// a variant, or a constructor of one, written as a claim is.
    Type(Claim),
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
    &["define", "claim", "type", "macro", "fn", "let", "do", "if", "match", "nil", "true", "false"];

/// Whether `name` is a word of the language: a form, or a name no
/// definition may take.
pub fn reserved(name: &str) -> bool {
    RESERVED.contains(&name) || matches!(name, "fn!" | "perform" | "perform!" | "_" | "...")
}

/// What to write instead of the forms conduct no longer has.
fn retired(name: &str) -> Option<&'static str> {
    Some(match name {
        "nil" => {
            "conduct has no `nil`: leave an attribute out of a record, return a variant such as \
             `{error: :not-found}`, or use `[]` for no children"
        }
        "??" => {
            "conduct has no `??`: read an attribute with a default, `(title todo \"\")`, or `(get :key of: dictionary or: default)`"
        }
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

const TOP: &str = "a top-level form is `(define name value)`, `(define (name params ..) body)`, a `claim` of one, \
                   a `type`, or a `macro`";

pub fn item(form: &Sexp) -> Result<Item> {
    let Sexp::List(items, span) = form else {
        return err(TOP, form.span());
    };
    match items.first().and_then(Sexp::as_symbol) {
        Some("define") => define(&items[1..], *span).map(Item::Def),
        Some("macro") => macro_item(&items[1..], *span).map(Item::Macro),
        // Types and signatures name global things, so the marks of the
        // expansion that may have written them do not matter.
        Some("claim") => claim(&unmarked_all(&items[1..]), *span, false).map(Item::Claim),
        Some("type") => {
            let c = claim(&unmarked_all(&items[1..]), *span, true)?;
            if c.performs {
                return err(format!("a type is not a task: write `{}`, not `{}!`", c.name, c.name), *span);
            }
            Ok(Item::Type(c))
        }
        Some(name) if retired(name).is_some() => err(retired(name).unwrap(), *span),
        _ => err(TOP, *span),
    }
}

/// A name as a definition's head or a claim writes it: `show!` marks a
/// definition that makes a task, and names `show`.
fn marked(form: &Sexp) -> Result<(String, bool)> {
    if let Sexp::Symbol(name, span) = form {
        let b = bare(name);
        if b.len() > 1 && b.ends_with('!') {
            let unbanged = format!("{}{}", &b[..b.len() - 1], mark_of(name));
            return binder(&Sexp::Symbol(unbanged, *span)).map(|n| (n, true));
        }
    }
    binder(form).map(|n| (n, false))
}

/// A form with every symbol's mark taken off.
fn unmarked(form: &Sexp) -> Sexp {
    match form {
        Sexp::Symbol(name, span) => Sexp::Symbol(bare(name).to_string(), *span),
        Sexp::List(items, span) => Sexp::List(unmarked_all(items), *span),
        Sexp::Vector(items, span) => Sexp::Vector(unmarked_all(items), *span),
        Sexp::Map(items, span) => Sexp::Map(unmarked_all(items), *span),
        other => other.clone(),
    }
}

fn unmarked_all(forms: &[Sexp]) -> Vec<Sexp> {
    forms.iter().map(unmarked).collect()
}

fn define(args: &[Sexp], span: Span) -> Result<Def> {
    // A definition's name is global, whatever expansion wrote it.
    let global = |name: String| bare(&name).to_string();
    match args {
        [Sexp::Symbol(name, nspan), ..] if bare(name).ends_with('!') => {
            let name = bare(name);
            err(format!("`{name}` is a task, so its body is a task's: `(define ({name}) body)`"), *nspan)
        }
        [Sexp::Symbol(..), value] => Ok(Def { name: global(binder(&args[0])?), body: expr(value)?, span }),
        [Sexp::Symbol(..), ..] => err("`(define name value)` takes one value", span),
        [Sexp::List(head, hspan), body @ ..] => {
            let Some((name, params)) = head.split_first() else {
                return err("`(define (name params ..) body)` needs a name", *hspan);
            };
            let (name, performs) = marked(name)?;
            let name = global(name);
            let body = sequence(body, span)?;
            let body = if performs { Expr::Task(Box::new(body)) } else { body };
            if params.is_empty() {
                // A task needs nothing to be made: it is the value.
                if performs {
                    return Ok(Def { name, body, span });
                }
                return err(format!("`{name}` takes no parameters, so it is a value: `(define {name} value)`"), *hspan);
            }
            // Names alone make a function; any pattern makes a clause of
            // one.
            let literal = |p: &Sexp| matches!(p.as_symbol(), Some("true" | "false"));
            let plain = params.iter().all(|p| matches!(p, Sexp::Symbol(..) | Sexp::Label(..)) && !literal(p));
            if plain {
                let params = parameters(params, *hspan)?;
                return Ok(Def { name, body: Expr::Fn(params, Box::new(body)), span });
            }
            let params = clause_parameters(params, *hspan)?;
            Ok(Def { name, body: Expr::Clauses(vec![Clause { params, body }]), span })
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
                // The label is the parameter's name as written, whatever
                // expansion wrote it: labels are part of the hash.
                let name = binder(item)?;
                Param { label: bare(&name).to_string(), local: name }
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

/// `(macro (cond test then rest ...) (if test then (cond rest ...)))`.
fn macro_item(args: &[Sexp], span: Span) -> Result<Macro> {
    const FORM: &str = "a macro rule is `(macro (name pattern ..) template)`";
    let [Sexp::List(head, hspan), template] = args else {
        return err(FORM, span);
    };
    let Some((name, pattern)) = head.split_first() else {
        return err(FORM, *hspan);
    };
    // A macro's name is the word its calls start with, `!` and all: a
    // macro may stand for a form that performs.
    let name = match name {
        Sexp::Symbol(s, sspan) if reserved(bare(s)) => {
            return err(format!("`{}` is reserved and cannot be bound", bare(s)), *sspan);
        }
        Sexp::Symbol(s, _) => bare(s).to_string(),
        other => return err(format!("a macro is named by a symbol, not `{other}`"), other.span()),
    };
    Ok(Macro { name, pattern: pattern.to_vec(), template: template.clone(), span })
}

/// `[] by: 0`: a pattern for each parameter, the first unlabeled.
fn clause_parameters(items: &[Sexp], span: Span) -> Result<Vec<(Option<String>, Pat)>> {
    let mut out: Vec<(Option<String>, Pat)> = Vec::new();
    let mut rest = items.iter();
    while let Some(item) = rest.next() {
        match item {
            Sexp::Label(label, lspan) if !out.is_empty() => {
                let Some(p) = rest.next() else {
                    return err(format!("the label `{label}:` needs a pattern"), *lspan);
                };
                if out.iter().any(|(l, _)| l.as_deref() == Some(label)) {
                    return err(format!("the label `{label}:` appears twice"), *lspan);
                }
                out.push((Some(label.clone()), pattern(p)?));
            }
            Sexp::Label(_, lspan) => {
                return err("the first parameter is passed without a label, so it is a pattern alone", *lspan);
            }
            p if out.is_empty() => out.push((None, pattern(p)?)),
            // A name alone is its own label, as in a function's head.
            Sexp::Symbol(..) => {
                let name = binder(item)?;
                let label = bare(&name).to_string();
                if out.iter().any(|(l, _)| l.as_deref() == Some(&label)) {
                    return err(format!("the label `{label}:` appears twice"), item.span());
                }
                out.push((Some(label), Pat::Name(name)));
            }
            p => {
                return err(
                    format!("after the first parameter, each is a name or `label: pattern`, not `{p}`"),
                    p.span(),
                );
            }
        }
    }
    let mut names: Vec<String> = out.iter().flat_map(|(_, p)| p.binders()).collect();
    names.sort();
    if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
        return err(format!("the parameters bind `{}` twice", bare(&w[0])), span);
    }
    Ok(out)
}

fn claim(args: &[Sexp], span: Span, is_type: bool) -> Result<Claim> {
    let form = if is_type {
        "a type is `(type name : type)` or `(type (name ?a ..) : type)`"
    } else {
        "a claim is `(claim name : type)` or `(claim (name params ..) with: capabilities : type)`"
    };
    let form = form.to_string();
    let Some((head, rest)) = args.split_first() else {
        return err(form, span);
    };
    let ((name, performs), params) = match head {
        Sexp::Symbol(..) => (marked(head)?, None),
        Sexp::List(items, hspan) => {
            let Some((name, params)) = items.split_first() else {
                return err(form, *hspan);
            };
            let params = if params.is_empty() { None } else { Some(type_args(params, *hspan)?) };
            (marked(name)?, params)
        }
        other => return err(form, other.span()),
    };
    let (with, rest) = match rest {
        [Sexp::Label(l, _), ..] if l == "with" && is_type => {
            return err("a type needs no capabilities; only a task's signature says `with:`", span);
        }
        [Sexp::Label(l, _), caps, rest @ ..] if l == "with" => (Some(capabilities(caps)?), rest),
        _ => (None, rest),
    };
    let [Sexp::Label(colon, _), ty] = rest else {
        return err(form, span);
    };
    if !colon.is_empty() {
        return err(format!("{form}, with `:` before the type"), span);
    }
    if with.is_some() && !performs {
        return err(format!("only a task needs capabilities: claim `{name}!` with them"), span);
    }
    Ok(Claim { name, performs, params, with, ty: type_expr(ty)?, span })
}

/// `?a error: ?e`: types, each but the first after its label.
fn type_args(items: &[Sexp], span: Span) -> Result<Vec<(Option<String>, TypeExpr)>> {
    let mut out: Vec<(Option<String>, TypeExpr)> = Vec::new();
    let mut rest = items.iter();
    while let Some(item) = rest.next() {
        match item {
            Sexp::Label(label, lspan) => {
                let Some(t) = rest.next() else {
                    return err(format!("the label `{label}:` needs a type"), *lspan);
                };
                if out.iter().any(|(l, _)| l.as_deref() == Some(label)) {
                    return err(format!("the label `{label}:` appears twice"), *lspan);
                }
                out.push((Some(label.clone()), type_expr(t)?));
            }
            t if out.is_empty() => out.push((None, type_expr(t)?)),
            t => return err(format!("only the first parameter goes without a label, not `{t}`"), span),
        }
    }
    Ok(out)
}

/// What follows `with:`: a set of capabilities, `{print model}`, or a
/// variable, `?fx`.
fn capabilities(form: &Sexp) -> Result<TypeExpr> {
    match form {
        Sexp::Map(items, _) => Ok(TypeExpr::Caps(
            items
                .iter()
                .map(|c| match c {
                    Sexp::Symbol(c, _) => Ok(c.clone()),
                    other => err("capabilities are names, `{print model}`", other.span()),
                })
                .collect::<Result<_>>()?,
        )),
        Sexp::Symbol(v, _) if v.starts_with('?') && v.len() > 1 => Ok(TypeExpr::Var(v[1..].to_string())),
        other => err("`with:` takes capabilities, `{print model}`, or a variable, `?fx`", other.span()),
    }
}

fn type_expr(form: &Sexp) -> Result<TypeExpr> {
    Ok(match form {
        Sexp::Symbol(name, _) if name.starts_with('?') && name.len() > 1 => TypeExpr::Var(name[1..].to_string()),
        Sexp::Symbol(name, _) if name == "?" => {
            return err("a type variable has a name, `?a`, so that its uses can be tied together", form.span());
        }
        Sexp::Symbol(name, span) => TypeExpr::Name(name.clone(), *span),
        Sexp::Keyword(k, _) => TypeExpr::Keyword(k.clone()),
        Sexp::List(items, span) => match items.as_slice() {
            // `((\ params ..) with: caps : result)`
            [Sexp::List(lambda, lspan), rest @ ..] if matches!(lambda.first(), Some(Sexp::Symbol(s, _)) if s == "\\") =>
            {
                let params = type_args(&lambda[1..], *lspan)?;
                let (with, rest) = match rest {
                    [Sexp::Label(l, _), caps, rest @ ..] if l == "with" => (Some(Box::new(capabilities(caps)?)), rest),
                    _ => (None, rest),
                };
                let [Sexp::Label(colon, _), ret] = rest else {
                    return err("a function type is `((\\ params ..) : result)`", *span);
                };
                if !colon.is_empty() {
                    return err("a function type is `((\\ params ..) : result)`", *span);
                }
                TypeExpr::Fn(params, with, Box::new(type_expr(ret)?))
            }
            [Sexp::Symbol(global, _), value] if global.contains('/') => {
                TypeExpr::Attribute(global.clone(), Box::new(type_expr(value)?), *span)
            }
            [Sexp::Symbol(head, _), args @ ..] if !args.is_empty() => {
                TypeExpr::App(head.clone(), type_args(args, *span)?, *span)
            }
            _ => return err(format!("expected a type, found `{form}`"), *span),
        },
        Sexp::Vector(items, _) => TypeExpr::Union(items.iter().map(type_expr).collect::<Result<_>>()?),
        Sexp::Map(items, _) => {
            let mut members = Vec::new();
            let mut optional = None;
            let mut rest = items.iter();
            while let Some(item) = rest.next() {
                match item {
                    Sexp::Symbol(s, sspan) if s == "?" => {
                        if optional.is_some() {
                            return err("a record type has one `?`, before its optional attributes", *sspan);
                        }
                        optional = Some(members.len());
                    }
                    Sexp::Symbol(s, sspan) => members.push(Member::Name(s.clone(), *sspan)),
                    // `io.gozala.point/x: integer`, as a record writes its
                    // value.
                    Sexp::Label(global, lspan) if global.contains('/') => {
                        let Some(value) = rest.next() else {
                            return err(format!("the attribute `{global}:` needs the type of its values"), *lspan);
                        };
                        members.push(Member::Inline(global.clone(), type_expr(value)?))
                    }
                    Sexp::List(parts, pspan) => match parts.as_slice() {
                        [Sexp::Symbol(global, _), value] if global.contains('/') => {
                            members.push(Member::Inline(global.clone(), type_expr(value)?))
                        }
                        _ => return err("a record type lists attributes: names, or `(domain/name type)`", *pspan),
                    },
                    other => {
                        return err("a record type lists attributes: names, or `(domain/name type)`", other.span());
                    }
                }
            }
            let optional = optional.unwrap_or(members.len());
            TypeExpr::Record(members, optional)
        }
        other => return err(format!("expected a type, found `{other}`"), other.span()),
    })
}

/// A name a form binds, with the mark of the expansion that wrote it,
/// if any: a macro's binder is distinct from every name its call's
/// forms bind.
fn binder(form: &Sexp) -> Result<String> {
    match form {
        Sexp::Symbol(full, span) => {
            let name = bare(full);
            if RESERVED.contains(&name) {
                return err(format!("`{name}` is reserved and cannot be bound"), *span);
            }
            if name.len() > 1 && name.ends_with('!') {
                return err(
                    format!("`!` marks a call that performs, not a name: call it `{}`", &name[..name.len() - 1]),
                    *span,
                );
            }
            Ok(full.clone())
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
        Sexp::Symbol(name, span) => Ok(match bare(name) {
            n if retired(n).is_some() => return err(retired(n).unwrap(), *span),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            n if n.len() > 1 && n.ends_with('!') => {
                let bare = &n[..n.len() - 1];
                return err(
                    format!("`!` performs, so it goes where a call does, `({n} ..)`; as a value this is `{bare}`"),
                    *span,
                );
            }
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
    if let Some((Sexp::Str(key, kspan), _)) = pairs.first() {
        return err(format!("a dictionary's keys are keywords: write `:{key}`, not `\"{key}\"`"), *kspan);
    }
    // `{}` is nothing at all, a dictionary or a record; keyword keys make
    // a dictionary, its entries sorted by key.
    if pairs.is_empty() || matches!(pairs[0].0, Sexp::Keyword(..)) {
        let mut entries: Vec<(String, Expr)> = Vec::new();
        for (k, v) in pairs {
            let key = match k {
                Sexp::Keyword(key, _) => key,
                Sexp::Str(key, kspan) => {
                    return err(format!("a dictionary's keys are keywords: write `:{key}`, not `\"{key}\"`"), *kspan);
                }
                other => return err("a dictionary's keys are all keywords, `:name`", other.span()),
            };
            if entries.iter().any(|(name, _)| name == key) {
                return err(format!("the key `:{key}` appears twice"), k.span());
            }
            entries.push((key.clone(), expr(v)?));
        }
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        return Ok(Expr::Dict(entries));
    }
    let mut fields: Vec<(String, Expr)> = Vec::new();
    for (k, v) in pairs {
        let name = match k {
            Sexp::Label(name, _) if !name.is_empty() => name,
            Sexp::Keyword(name, kspan) => {
                return err(
                    format!("a record's keys are attributes, a dictionary's keywords: write `{name}:`, not `:{name}`"),
                    *kspan,
                );
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
            "define" | "claim" | "type" | "macro" => {
                return err(format!("`{name}` is only allowed at the top level"), span);
            }
            "fn" | "fn!" => {
                let [Sexp::Vector(params, pspan), body @ ..] = args else {
                    return err(format!("`{name}` takes a parameter vector and a body"), span);
                };
                let body = sequence(body, span)?;
                if name == "fn!" {
                    let task = Expr::Task(Box::new(body));
                    if params.is_empty() {
                        return Ok(task);
                    }
                    return Ok(Expr::Fn(parameters(params, *pspan)?, Box::new(task)));
                }
                if params.is_empty() {
                    return err("a function takes at least one parameter", *pspan);
                }
                return Ok(Expr::Fn(parameters(params, *pspan)?, Box::new(body)));
            }
            "perform" | "perform!" => {
                let out = perform(args, span)?;
                return Ok(if name == "perform!" { Expr::Perform(Box::new(out)) } else { out });
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
            "match" => return match_form(args, span),
            _ => {}
        }
        if name == "assoc"
            && let [record, Sexp::Label(..), ..] = args
        {
            return Ok(Expr::Update(boxed(record)?, labeled(&args[1..], span)?, span));
        }
    }
    // `(show! x)` performs what `(show x)` answers, and `(t!)`, with no
    // arguments, performs `t` itself.
    if let Sexp::Symbol(name, nspan) = head
        && bare(name).len() > 1
        && bare(name).ends_with('!')
    {
        let b = bare(name);
        let callee = Expr::Var(format!("{}{}", &b[..b.len() - 1], mark_of(name)), *nspan);
        if args.is_empty() {
            return Ok(Expr::Perform(Box::new(callee)));
        }
        return Ok(Expr::Perform(Box::new(Expr::App(Box::new(callee), arguments(args)?, span))));
    }
    let callee = expr(head)?;
    // A call with no arguments is the value itself: no function takes
    // none.
    if args.is_empty() {
        return Ok(callee);
    }
    Ok(Expr::App(Box::new(callee), arguments(args)?, span))
}

/// `(perform task)` is the task; `(perform task with: env)` is a task
/// that performs it with the environment's implementations.
fn perform(args: &[Sexp], span: Span) -> Result<Expr> {
    match args {
        [task] => expr(task),
        [task, Sexp::Label(l, _), Sexp::Map(caps, mspan)] if l == "with" => {
            let mut provided: Vec<(String, Vec<(String, Expr)>)> = Vec::new();
            if !caps.len().is_multiple_of(2) {
                return err("an environment is `{capability: {operation: implementation ..} ..}`", *mspan);
            }
            for pair in caps.chunks(2) {
                let (Sexp::Label(cap, _), Sexp::Map(ops, ospan)) = (&pair[0], &pair[1]) else {
                    return err("an environment is `{capability: {operation: implementation ..} ..}`", *mspan);
                };
                if !ops.len().is_multiple_of(2) {
                    return err("a capability's implementation is `{operation: function ..}`", *ospan);
                }
                let mut impls: Vec<(String, Expr)> = Vec::new();
                for op in ops.chunks(2) {
                    let Sexp::Label(name, lspan) = &op[0] else {
                        return err("a capability's implementation is `{operation: function ..}`", *ospan);
                    };
                    if impls.iter().any(|(n, _)| n == name) {
                        return err(format!("the operation `{name}:` appears twice"), *lspan);
                    }
                    impls.push((name.clone(), expr(&op[1])?));
                }
                if provided.iter().any(|(c, _)| c == cap) {
                    return err(format!("the capability `{cap}:` appears twice"), *mspan);
                }
                provided.push((cap.clone(), impls));
            }
            Ok(Expr::Handle(Box::new(expr(task)?), provided))
        }
        _ => err("`perform` takes a task, and maybe `with:` an environment", span),
    }
}

fn match_form(args: &[Sexp], span: Span) -> Result<Expr> {
    let Some((scrutinee, clauses)) = args.split_first() else {
        return err("`match` takes a value and pairs of a pattern and a result", span);
    };
    if clauses.is_empty() || !clauses.len().is_multiple_of(2) {
        return err("`match` takes a value and pairs of a pattern and a result", span);
    }
    let mut arms: Vec<(Pat, Expr)> = Vec::new();
    let mut default = None;
    let pairs: Vec<&[Sexp]> = clauses.chunks(2).collect();
    for (i, pair) in pairs.iter().enumerate() {
        if let Sexp::Symbol(s, sspan) = &pair[0]
            && bare(s) == "_"
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

fn pattern(form: &Sexp) -> Result<Pat> {
    Ok(match form {
        Sexp::Keyword(k, _) => Pat::Keyword(k.clone()),
        Sexp::Str(s, _) => Pat::Str(s.clone()),
        Sexp::Int(n, _) => Pat::Int(*n),
        Sexp::Symbol(s, _) if bare(s) == "_" => Pat::Any,
        Sexp::Symbol(s, _) if bare(s) == "true" || bare(s) == "false" => Pat::Bool(bare(s) == "true"),
        Sexp::Symbol(s, sspan) => {
            if let Some(why) = retired(bare(s)) {
                return err(why, *sspan);
            }
            if RESERVED.contains(&bare(s)) {
                return err(format!("`{}` is reserved, so it cannot name a value a pattern binds", bare(s)), *sspan);
            }
            Pat::Name(s.clone())
        }
        Sexp::Vector(items, _) => {
            let mut out = Vec::new();
            let mut rest = None;
            let mut i = 0;
            while i < items.len() {
                if items[i].as_symbol() == Some("...") {
                    return err("`...` follows the pattern it repeats: `[first rest ...]`", items[i].span());
                }
                if items.get(i + 1).is_some_and(|f| f.as_symbol() == Some("...")) {
                    if i + 2 != items.len() {
                        return err("`...` repeats the last pattern of a list, so nothing follows it", items[i].span());
                    }
                    rest = Some(match pattern(&items[i])? {
                        Pat::Name(n) => Some(n),
                        Pat::Any => None,
                        _ => {
                            return err(
                                "in a list pattern, `...` follows a name for the rest of the list, or `_`",
                                items[i].span(),
                            );
                        }
                    });
                    break;
                }
                out.push(pattern(&items[i])?);
                i += 1;
            }
            Pat::List(out, rest)
        }
        Sexp::Map(items, _) if !items.is_empty() && items.len().is_multiple_of(2) => {
            let mut fields: Vec<(String, Pat)> = Vec::new();
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
            Pat::Record(fields)
        }
        Sexp::List(items, lspan) => {
            let Some((Sexp::Symbol(name, _), args)) = items.split_first() else {
                return err("a constructor pattern names a record type: `(ok value)`", *lspan);
            };
            let mut out: Vec<(Option<String>, Pat)> = Vec::new();
            let mut rest = args.iter();
            while let Some(item) = rest.next() {
                match item {
                    Sexp::Label(label, l) => {
                        let Some(p) = rest.next() else {
                            return err(format!("the label `{label}:` needs a pattern"), *l);
                        };
                        out.push((Some(label.clone()), pattern(p)?));
                    }
                    p if out.is_empty() => out.push((None, pattern(p)?)),
                    p => return err(format!("only the first pattern goes without a label, not `{p}`"), p.span()),
                }
            }
            Pat::Ctor(bare(name).to_string(), out)
        }
        other => {
            return err(
                format!(
                    "a `match` pattern is a keyword, string, or integer, `_`, a name, a record of patterns, \
                     or a constructor, `(ok value)`, not `{other}`"
                ),
                other.span(),
            );
        }
    })
}
