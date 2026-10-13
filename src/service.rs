//! What an editor and a host ask of the compiler about one source: its
//! component, or the type of each definition it adds, and when either
//! fails, where in the source the failure is.
//!
//! The compiler component exports these as they are, so a page that
//! runs it on the polyfill compiles and checks conduct as the command
//! does.

use crate::codebase::Error;
use crate::reader::{self, Sexp, Span};
use crate::{Codebase, element, route};

/// The world a source compiles to: the demo's element world, or its
/// route world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum World {
    Element,
    Route,
}

impl World {
    /// The world named `name`, as the WIT documents name them.
    pub fn named(name: &str) -> Option<World> {
        match name {
            "element-component" | "element" => Some(World::Element),
            "route-component" | "route" => Some(World::Route),
            _ => None,
        }
    }

    /// The codebase a source of this world adds to.
    fn codebase(self) -> Result<Codebase, Problem> {
        match self {
            World::Element => Ok(element::codebase()),
            World::Route => route::codebase().map_err(|e| Problem::at_start(e.to_string())),
        }
    }
}

/// Why a source does not compile, and the bytes of the source it is
/// about. A problem with no place in the source, such as a missing
/// definition, is at its start, with no length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub span: Span,
    pub message: String,
}

impl Problem {
    fn at_start(message: String) -> Problem {
        Problem { span: Span::default(), message }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// A definition a source adds, where its name is, and its type as a
/// claim would write it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub name: String,
    pub span: Span,
    pub ty: String,
}

/// The component `source` compiles to in `world`.
pub fn compile(source: &str, world: World) -> Result<Vec<u8>, Problem> {
    let cb = added(source, world)?;
    match world {
        World::Element => element::component(&cb).map_err(|e| contract(source, e.to_string(), contract_name(&e))),
        World::Route => route::component(&cb).map_err(|e| contract(source, e.to_string(), route_name(&e))),
    }
}

/// The definitions `source` adds in `world`, each with its type, in the
/// order the source writes them.
pub fn check(source: &str, world: World) -> Result<Vec<Shown>, Problem> {
    let cb = added(source, world)?;
    let mut shown = Vec::new();
    for (name, span) in names(source) {
        if shown.iter().any(|s: &Shown| s.name == name) {
            continue;
        }
        if let Some(ty) = cb.type_of(&name) {
            shown.push(Shown { name, span, ty });
        }
    }
    Ok(shown)
}

/// The codebase of `world` with `source` added.
fn added(source: &str, world: World) -> Result<Codebase, Problem> {
    let mut cb = world.codebase()?;
    cb.add_source(source).map_err(|e| locate(source, &e))?;
    Ok(cb)
}

/// Where in `source` the error `e` is.
fn locate(source: &str, e: &Error) -> Problem {
    let message = e.to_string();
    let span = match e {
        Error::Read(e) => e.span,
        Error::Syntax(e) => e.span,
        Error::Unbound { def, name } => within(source, def, name).unwrap_or_else(|| form_of(source, def)),
        Error::Resolve { def, .. } | Error::Type { def, .. } => form_of(source, def),
    };
    Problem { span, message }
}

/// A failure to fit a contract, at the definition it names.
fn contract(source: &str, message: String, name: Option<&str>) -> Problem {
    let span = name.map(|name| form_of(source, name)).unwrap_or_default();
    Problem { span, message }
}

fn contract_name(e: &element::ElementError) -> Option<&'static str> {
    match e {
        element::ElementError::Contract { name, .. } => Some(name),
        _ => None,
    }
}

fn route_name(e: &route::RouteError) -> Option<&'static str> {
    match e {
        route::RouteError::Contract { name, .. } => Some(name),
        _ => None,
    }
}

/// The name of each top-level definition, claim, type, and macro in
/// `source`, with where it is written, in source order.
fn names(source: &str) -> Vec<(String, Span)> {
    let Ok(forms) = reader::read(source) else { return Vec::new() };
    forms.iter().filter_map(named).map(|(name, span, _)| (name, span)).collect()
}

/// The name a top-level form gives, where the name is, and the form's
/// span.
fn named(form: &Sexp) -> Option<(String, Span, Span)> {
    let Sexp::List(items, whole) = form else { return None };
    let head = items.first()?.as_symbol()?;
    if !["define", "claim", "type", "macro"].contains(&head) {
        return None;
    }
    let target = match items.get(1)? {
        Sexp::List(inner, _) => inner.first()?,
        other => other,
    };
    let Sexp::Symbol(name, span) = target else { return None };
    Some((name.trim_end_matches('!').to_string(), *span, *whole))
}

/// The span of the first form that defines `def`, or of the first
/// form that names it at all, or the start of the source.
fn form_of(source: &str, def: &str) -> Span {
    let Ok(forms) = reader::read(source) else { return Span::default() };
    let naming = |f: &&Sexp| named(f).is_some_and(|(name, ..)| name == def);
    let defining = |f: &&Sexp| matches!(f, Sexp::List(items, _) if items[0].as_symbol() == Some("define"));
    let mut candidates = forms.iter().filter(naming);
    let first = candidates.clone().next();
    candidates.find(defining).or(first).map(Sexp::span).unwrap_or_default()
}

/// Where `name` is first written inside the definition of `def`.
fn within(source: &str, def: &str, name: &str) -> Option<Span> {
    let whole = form_of(source, def);
    let forms = reader::read(source).ok()?;
    let Some(Sexp::List(items, _)) = forms.iter().find(|f| f.span() == whole) else { return None };
    // The form's keyword and the name it defines come first; a use is
    // after them.
    items.iter().skip(2).find_map(|item| find_symbol(item, name))
}

fn find_symbol(form: &Sexp, name: &str) -> Option<Span> {
    match form {
        Sexp::Symbol(s, span) if s.trim_end_matches('!') == name.trim_end_matches('!') => Some(*span),
        Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
            items.iter().find_map(|item| find_symbol(item, name))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str = include_str!("../examples/todo-item.cdt");

    #[test]
    fn compiles_an_element() {
        let bytes = compile(ITEM, World::Element).unwrap();
        assert_eq!(&bytes[..4], b"\0asm");
    }

    #[test]
    fn shows_each_definition_where_it_is_written() {
        let shown = check("(define (twice x) (* 2 x))\n(define one 1)", World::Element).unwrap();
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].name, "twice");
        assert_eq!(shown[0].span, Span { start: 9, end: 14 });
        assert_eq!(shown[0].ty, "((\\ integer) : integer)");
        assert_eq!(shown[1].ty, "integer");
    }

    #[test]
    fn places_a_problem() {
        let source = "(define one 1)\n(define (f x) (+ x \"a\"))";
        let problem = check(source, World::Element).unwrap_err();
        assert_eq!(&source[problem.span.start..problem.span.end], "(define (f x) (+ x \"a\"))");
        let problem = check("(define (f x) (g x))", World::Element).unwrap_err();
        assert_eq!(problem.span, Span { start: 15, end: 16 });
        let problem = check("(define (f x)", World::Element).unwrap_err();
        assert!(problem.message.contains("read error"), "{}", problem.message);
        let problem = compile("(define one 1)", World::Element).unwrap_err();
        assert!(problem.message.contains("needs a definition"), "{}", problem.message);
        assert!(World::named("nothing").is_none());
    }

    #[test]
    fn compiles_a_route() {
        let bytes = compile(include_str!("../examples/todos-route.cdt"), World::Route).unwrap();
        assert_eq!(&bytes[..4], b"\0asm");
    }
}
