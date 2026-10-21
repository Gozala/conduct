//! The codebase: definitions stored by hash, and names that point at
//! them.
//!
//! Adding source resolves its names, groups mutually recursive
//! definitions into cycles, hashes each definition (a cycle is hashed
//! as a unit, as Unison does), and type checks only what the codebase
//! does not already hold. A definition is checked once, ever: adding the
//! same code again, under any names, finds its hash already present.

use crate::claims::{self, Claims, Env, Mode, TypeDef};
use crate::infer::{Binding, Globals, Infer};
use crate::macros::{self, Rule};
use crate::methods::{self, Impl, Method, Receiver, Shape};
use crate::reader::{self, Sexp, bare, pin_of};
use crate::syntax::{self, Arg, Claim, Clause, Def, Expr, Item, Param, Pat, TypeExpr};
use crate::term::{Hash, Op, Pattern, Sig, Term};
use crate::transform;
use crate::types::{Requirement, Scheme, Solver, Ty, TypeError, coalesce, coalesce_with};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::rc::Rc;

pub struct Definition {
    pub hash: Hash,
    pub term: Term,
    /// The members of this definition's cycle, in canonical order,
    /// when it is recursive. `Term::CycleRef(i)` refers to `cycle[i]`.
    pub cycle: Vec<Hash>,
    pub scheme: Scheme,
}

impl Definition {
    /// The definition's type, with attributes by their global names.
    pub fn type_string(&self) -> String {
        coalesce(&self.scheme.body).to_string()
    }
}

/// What every codebase starts with: the types that say how something
/// went.
pub const CORE: &str = include_str!("core.cdt");

#[derive(Default)]
pub struct Codebase {
    pub solver: Solver,
    pub defs: HashMap<Hash, Definition>,
    pub names: BTreeMap<String, Hash>,
    /// The attributes and types declared.
    pub claims: Claims,
    /// Host functions by the capability they need: each one's name and
    /// hash.
    pub hosts: BTreeMap<String, BTreeMap<String, Hash>>,
    /// Macros by name: each the closed set of its rules.
    pub macros: BTreeMap<String, macros::Macro>,
    /// Macros that are functions, `(macro name f)`, by name: the hash of
    /// the definition of `f`.
    pub transformers: BTreeMap<String, Hash>,
    /// What each type and attribute was declared as, as written: the form
    /// after `:` in its `type`, by the name it was declared under. A macro
    /// function is given these for the names its call holds.
    pub declarations: BTreeMap<String, Sexp>,
    /// Methods by the whole name of their claim: claims with no
    /// definition, which each type defines for itself.
    pub methods: BTreeMap<String, Method>,
    /// Macro functions compiled, by hash, so a source compiles each once.
    compiled: std::cell::RefCell<HashMap<Hash, std::rc::Rc<transform::Compiled>>>,
    /// The last mark an expansion gave its template's symbols.
    marks: u64,
}

/// The domain a built-in's whole name is in: `conduct/count` is the
/// built-in `count`, whatever else is named `count`.
pub const BUILTIN_DOMAIN: &str = "conduct";

/// How many expansions deep one form may go: a macro whose expansion
/// always calls it again is an error, not a hang.
const EXPANSION_DEPTH: usize = 512;

/// What adding a definition did.
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    pub name: String,
    pub hash: Hash,
    /// False when the codebase already held this hash.
    pub new: bool,
}

#[derive(Debug)]
pub enum Error {
    Read(reader::ReadError),
    Syntax(syntax::SyntaxError),
    Unbound { name: String, def: String },
    Resolve { def: String, message: String },
    Type { def: String, error: TypeError },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Read(e) => write!(f, "read error: {e}"),
            Error::Syntax(e) => write!(f, "syntax error: {e}"),
            Error::Unbound { name, def } => write!(f, "in `{def}`: `{name}` is not defined"),
            Error::Resolve { def, message } => write!(f, "in `{def}`: {message}"),
            Error::Type { def, error } => write!(f, "in `{def}`: type error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl Codebase {
    /// A codebase holding the core types.
    pub fn new() -> Self {
        let mut cb = Self::default();
        cb.add_source(CORE).expect("the core prelude is valid");
        cb
    }

    /// A definition's type as shown to a person: attributes by the local
    /// names this codebase claims them under.
    pub fn type_of(&self, name: &str) -> Option<String> {
        self.lookup(name).map(|d| self.show_scheme(&d.scheme))
    }

    /// A scheme as shown to a person: its type, and the methods it needs
    /// for the types they are called on, `where ?a has ord/compare`.
    pub fn show_scheme(&self, scheme: &Scheme) -> String {
        let receivers: Vec<Ty> = scheme.requires.iter().map(|r| r.receiver.clone()).collect();
        let (body, on) = coalesce_with(&scheme.body, &receivers);
        let rename = |t: crate::types::Type| {
            t.rename_keys(&|k| {
                self.claims.shown(k).or_else(|| self.local_attribute(k)).unwrap_or_else(|| k.to_string())
            })
        };
        let mut out = rename(body).to_string();
        let mut seen = Vec::new();
        for (r, t) in scheme.requires.iter().zip(on) {
            let need = format!("{} has {}", rename(t), r.method);
            if !seen.contains(&need) {
                seen.push(need);
            }
        }
        if !seen.is_empty() {
            out.push_str(" where ");
            out.push_str(&seen.join(", "));
        }
        out
    }

    /// A type as shown to a person, with attributes by local name.
    pub fn show(&self, ty: &Ty) -> String {
        coalesce(ty)
            .rename_keys(&|k| self.claims.shown(k).or_else(|| self.local_attribute(k)).unwrap_or_else(|| k.to_string()))
            .to_string()
    }

    /// The local name an attribute's global name is declared under, if
    /// any.
    pub fn local_attribute(&self, global: &str) -> Option<String> {
        self.claims.local(global)
    }

    /// The type a claim writes, with its variables free for callers to
    /// choose.
    pub fn claimed(&self, name: &str) -> Option<Result<Ty, String>> {
        let def = self.claims.types.get(name)?;
        let mut env = Env::new(Mode::Flexible(&self.solver, 1));
        Some(self.claims.ty(&def.body, &mut env))
    }

    /// Declare a function the host provides, under `name`: a call takes
    /// `params` and answers a task that needs `capability` and answers
    /// `ret`, performed `(name! ..)`. With no parameters it is that task.
    /// Its hash covers the name it is imported by and its type.
    pub fn add_host(&mut self, name: &str, capability: &str, params: Vec<Ty>, ret: Ty) -> Result<Hash, Error> {
        if name.ends_with('!') {
            return Err(Error::Resolve {
                def: name.into(),
                message: "a host function is named without `!`; a call performs it with one".into(),
            });
        }
        let arity = params.len() as u32;
        let args = (0..arity).rev().map(Term::Local).collect();
        let task = Ty::task(ret, Ty::caps([capability.to_string()]));
        let body = Term::Task(Box::new(Term::Host(name.to_string(), args)));
        // A host function takes at most one argument, by position; the
        // labels are only there to make the term well formed.
        let labels: Vec<String> =
            (0..arity).map(|i| if i == 0 { "value".to_string() } else { format!("arg{i}") }).collect();
        let (term, ty) = if arity == 0 {
            (body, task)
        } else {
            let rest = labels[1..].to_vec();
            (Term::Fn(labels, Box::new(body)), Ty::func_eff(params, task, Ty::pure(), rest))
        };
        let mut bytes = Vec::new();
        term.encode(&mut bytes, &|i| i);
        bytes.extend_from_slice(coalesce(&ty).to_string().as_bytes());
        let hash = Hash::of(&bytes);
        let scheme = Scheme::new(0, ty);
        self.defs.insert(hash, Definition { hash, term, cycle: vec![], scheme });
        self.names.insert(name.to_string(), hash);
        self.hosts.entry(capability.to_string()).or_default().insert(name.to_string(), hash);
        Ok(hash)
    }

    pub fn lookup(&self, name: &str) -> Option<&Definition> {
        self.names.get(name).and_then(|h| self.defs.get(h))
    }

    /// Parse, resolve, hash, and type check `source`, then point its
    /// names at the resulting hashes. Nothing changes on an error.
    ///
    /// A source is one compilation. Its macro rules come first: each
    /// name's, with the rules the codebase already has for it, are
    /// closed into one macro, which every form of the source then
    /// expands by, wherever the rules are written.
    pub fn add_source(&mut self, source: &str) -> Result<Vec<Added>, Error> {
        let forms = reader::read(source).map_err(Error::Read)?;
        let saved = (
            self.claims.clone(),
            self.names.clone(),
            self.macros.clone(),
            self.transformers.clone(),
            self.declarations.clone(),
            self.methods.clone(),
        );
        let out = flatten_domains(&forms).and_then(|forms| self.add_forms(&forms));
        if out.is_err() {
            (self.claims, self.names, self.macros, self.transformers, self.declarations, self.methods) = saved;
        }
        out
    }

    /// The whole name in the codebase that `written` refers to: the
    /// definition with that whole name, else the one whose name ends in
    /// the segments written (`parser/run` and `run` are both
    /// `conduct.parser/run`), with a built-in of that name a candidate
    /// too. When several match, the name is ambiguous, and the error
    /// lists each whole name.
    fn resolve_name(&self, written: &str, defined: &dyn Fn(&str) -> bool) -> Result<Option<String>, Vec<String>> {
        if defined(written) {
            return match builtin_named(written) {
                Some(op) => Err(vec![written.to_string(), op.whole_name()]),
                None => Ok(Some(written.to_string())),
            };
        }
        let mut candidates: Vec<String> =
            self.names.keys().chain(self.methods.keys()).filter(|n| ends_with_segments(n, written)).cloned().collect();
        let builtin = Op::from_written(written);
        match (candidates.len(), builtin) {
            (0, _) => Ok(None),
            (1, None) => Ok(candidates.pop()),
            _ => {
                if let Some(op) = builtin {
                    candidates.push(op.whole_name());
                }
                Err(candidates)
            }
        }
    }

    /// The method a bare name means: the one method whose name ends with
    /// it, unless a built-in has the name.
    fn method_named(&self, written: &str) -> Option<&str> {
        if written.contains('/') || Op::from_written(written).is_some() {
            return None;
        }
        let mut found = self.methods.keys().filter(|m| m.rsplit('/').next() == Some(written));
        match (found.next(), found.next()) {
            (Some(m), None) => Some(m.as_str()),
            _ => None,
        }
    }

    fn add_forms(&mut self, forms: &[(Sexp, Option<String>)]) -> Result<Vec<Added>, Error> {
        // Macro functions come first: each, with the types of the source
        // and the definitions of it they use, is added before anything
        // else is expanded, so that the rest may call them.
        let functions: Vec<(String, Option<String>)> = forms
            .iter()
            .filter_map(|(form, prefix)| macro_function(form).map(|(name, _)| (name.to_string(), prefix.clone())))
            .collect();
        if !functions.is_empty() {
            let (first, then) = macro_phases(forms);
            let mut added = self.add_forms(&first)?;
            for (name, prefix) in functions {
                let error = |message: String| Error::Resolve { def: format!("({name} ..)"), message };
                if self.macros.contains_key(&name) || self.transformers.contains_key(&name) {
                    return Err(error(format!("`{name}` is already a macro")));
                }
                if Op::from_name(&name).is_some() || syntax::reserved(&name) {
                    return Err(error(format!("`{name}` is built in, so it cannot also name a macro")));
                }
                let whole = match prefix {
                    Some(p) => format!("{p}/{name}{MACRO_SUFFIX}"),
                    None => format!("{name}{MACRO_SUFFIX}"),
                };
                self.transformers.insert(name, self.names[&whole]);
            }
            added.extend(self.add_forms(&then)?);
            return Ok(added);
        }
        let mut rules: BTreeMap<String, Vec<Rule>> = BTreeMap::new();
        let mut rest = Vec::new();
        for (form, prefix) in forms {
            if let Sexp::List(items, _) = form
                && items.first().and_then(Sexp::as_symbol) == Some("macro")
            {
                let Item::Macro(m) = syntax::item(form).map_err(Error::Syntax)? else { unreachable!() };
                let rule = Rule::new(&m.name, &m.pattern, &m.template)
                    .map_err(|message| Error::Resolve { def: format!("({} ..)", m.name), message })?;
                rules.entry(m.name).or_default().push(rule);
            } else {
                rest.push((form, prefix));
            }
        }
        let closed: Vec<String> = rules.keys().cloned().collect();
        for (name, new) in rules {
            let error = |message: String| Error::Resolve { def: format!("({name} ..)"), message };
            let unbanged = name.strip_suffix('!').unwrap_or(&name);
            if self.names.contains_key(unbanged) || self.claims.claims(unbanged) {
                return Err(error(format!("`{name}` is already defined, so it cannot also name a macro")));
            }
            if Op::from_name(&name).is_some() || syntax::reserved(&name) {
                return Err(error(format!("`{name}` is built in, so it cannot also name a macro")));
            }
            let mut all = self.macros.get(&name).map(|m| m.rules.clone()).unwrap_or_default();
            all.extend(new);
            self.macros.insert(name.clone(), macros::Macro::close(&name, all).map_err(error)?);
        }
        // What the source declares, a macro function may read wherever in
        // the source it is called: types do not depend on order.
        for (form, _) in &rest {
            if let (Some(name), Some(written)) = (declared_name(form), declared_form(form)) {
                self.declarations.insert(name.to_string(), written);
            }
        }
        let mut items = Vec::new();
        let mut declared = Vec::new();
        for (form, prefix) in rest {
            let form = self.expand_top(form, 0)?;
            let mut item = syntax::item(&form).map_err(Error::Syntax)?;
            if let (Item::Type(t), Some(written)) = (&item, declared_form(&form)) {
                declared.push((t.name.clone(), written));
            }
            // A domain prefixes the names its definitions and claims
            // write, unless they write one already.
            if let Some(prefix) = prefix {
                let name = match &mut item {
                    Item::Def(d) => Some(&mut d.name),
                    Item::Claim(c) => Some(&mut c.name),
                    _ => None,
                };
                if let Some(name) = name
                    && !name.contains('/')
                {
                    *name = format!("{prefix}/{name}");
                }
            }
            items.push(item);
        }
        let added = self.add_items(items)?;
        self.declarations.extend(declared);
        // What a macro's template names, it names as of its closing.
        for name in closed {
            let mut m = self.macros.remove(&name).unwrap();
            for rule in &mut m.rules {
                rule.pin(&|s| self.names.get(s.strip_suffix('!').unwrap_or(s)).copied());
            }
            let m = macros::Macro::close(&name, m.rules).expect("pinning keeps the rules' patterns");
            self.macros.insert(name, m);
        }
        Ok(added)
    }

    /// A top-level form with its macro calls expanded. A macro may write
    /// a definition or a type.
    fn expand_top(&mut self, form: &Sexp, depth: usize) -> Result<Sexp, Error> {
        if let Sexp::List(items, span) = form
            && let Some(head) = items.first().and_then(Sexp::as_symbol)
        {
            match head {
                // A definition's head is not a call.
                "define" if items.len() > 2 => {
                    let mut out = items[..2].to_vec();
                    for item in &items[2..] {
                        out.push(self.expand(item, 0)?);
                    }
                    return Ok(Sexp::List(out, *span));
                }
                "define" | "claim" | "type" => return Ok(form.clone()),
                "macro" => {
                    return Err(Error::Resolve {
                        def: "macro".into(),
                        message: "a macro's expansion cannot write a macro rule yet".into(),
                    });
                }
                name if self.macros.contains_key(name) => {
                    let out = self.expand_call(name, &items[1..], *span, depth)?;
                    return self.expand_top(&out, depth + 1);
                }
                name if self.transformers.contains_key(name) => {
                    let out = self.transform(name, form, depth)?;
                    return self.expand_top(&out, depth + 1);
                }
                _ => {}
            }
        }
        self.expand(form, 0)
    }

    /// `form` with every call of a macro replaced by its expansion.
    fn expand(&mut self, form: &Sexp, depth: usize) -> Result<Sexp, Error> {
        let all =
            |cb: &mut Self, items: &[Sexp]| items.iter().map(|i| cb.expand(i, depth)).collect::<Result<Vec<_>, _>>();
        Ok(match form {
            Sexp::List(items, span) => match items.first().and_then(Sexp::as_symbol) {
                Some("quasiquote") => match &items[1..] {
                    [quoted] => self.quasiquote(quoted, depth)?,
                    _ => return Err(quasi_error("`` ` `` quotes one form")),
                },
                Some(name @ ("unquote" | "unquote-splicing")) => {
                    return Err(quasi_error(&format!("`{name}` is only meaningful inside `` ` ``")));
                }
                Some(name) if self.macros.contains_key(name) => {
                    let out = self.expand_call(name, &items[1..], *span, depth)?;
                    self.expand(&out, depth + 1)?
                }
                Some(name) if self.transformers.contains_key(name) => {
                    let out = self.transform(name, form, depth)?;
                    self.expand(&out, depth + 1)?
                }
                _ => Sexp::List(all(self, items)?, *span),
            },
            Sexp::Vector(items, span) => Sexp::Vector(all(self, items)?, *span),
            Sexp::Map(items, span) => Sexp::Map(all(self, items)?, *span),
            other => other.clone(),
        })
    }

    /// What a quasiquoted form is: code that makes it as a `form` value,
    /// with each `~x` the value of `x` and each `~@xs` the forms of `xs`
    /// in its place.
    fn quasiquote(&mut self, quoted: &Sexp, depth: usize) -> Result<Sexp, Error> {
        let span = quoted.span();
        let symbol = |s: &str| Sexp::Symbol(s.to_string(), span);
        let make = |ctor: &str, arg: Sexp| Sexp::List(vec![symbol(ctor), arg], span);
        Ok(match quoted {
            Sexp::Symbol(s, _) => make("form-symbol", Sexp::Str(reader::bare(s).to_string(), span)),
            Sexp::Keyword(k, _) => make("form-keyword", Sexp::Str(k.clone(), span)),
            Sexp::Label(l, _) => make("form-label", Sexp::Str(l.clone(), span)),
            Sexp::Str(t, _) => make("form-text", Sexp::Str(t.clone(), span)),
            Sexp::Int(n, _) => make("form-integer", Sexp::Int(*n, span)),
            Sexp::List(items, _) if items.first().and_then(Sexp::as_symbol) == Some("unquote") => match &items[1..] {
                [x] => self.expand(x, depth)?,
                _ => return Err(quasi_error("`~` takes one form")),
            },
            Sexp::List(items, _) if items.first().and_then(Sexp::as_symbol) == Some("unquote-splicing") => {
                return Err(quasi_error("`~@` splices forms into a list, a vector or a map"));
            }
            Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
                let kind = match quoted {
                    Sexp::List(..) => "form-list",
                    Sexp::Vector(..) => "form-vector",
                    _ => "form-map",
                };
                // Runs of quoted forms, as lists, joined with what each
                // `~@` splices.
                let mut parts: Vec<Sexp> = Vec::new();
                let mut run: Vec<Sexp> = Vec::new();
                for item in items {
                    match item {
                        Sexp::List(xs, _) if xs.first().and_then(Sexp::as_symbol) == Some("unquote-splicing") => {
                            let [_, x] = &xs[..] else { return Err(quasi_error("`~@` takes one form")) };
                            if !run.is_empty() {
                                parts.push(Sexp::Vector(std::mem::take(&mut run), span));
                            }
                            parts.push(self.expand(x, depth)?);
                        }
                        _ => run.push(self.quasiquote(item, depth)?),
                    }
                }
                if !run.is_empty() || parts.is_empty() {
                    parts.push(Sexp::Vector(run, span));
                }
                let joined = parts
                    .into_iter()
                    .rev()
                    .reduce(|acc, part| Sexp::List(vec![symbol(&format!("{BUILTIN_DOMAIN}/concat")), part, acc], span))
                    .unwrap();
                make(kind, joined)
            }
        })
    }

    /// The expansion of a call of the macro function `name`: the function
    /// run on the call's form, with the declarations of the names in it.
    fn transform(&mut self, name: &str, call: &Sexp, depth: usize) -> Result<Sexp, Error> {
        let error = |message: String| Error::Resolve { def: format!("({name} ..)"), message };
        if depth >= EXPANSION_DEPTH {
            return Err(error(format!(
                "expands more than {EXPANSION_DEPTH} macro calls deep: does its expansion always call it again?"
            )));
        }
        let hash = self.transformers[name];
        let cached = self.compiled.borrow().get(&hash).cloned();
        let compiled = match cached {
            Some(c) => c,
            None => {
                let c = std::rc::Rc::new(transform::compile(self, hash).map_err(error)?);
                self.compiled.borrow_mut().insert(hash, c.clone());
                c
            }
        };
        // What the names in the call are declared as, and what the names
        // in those declarations are, and so on: a type's declaration names
        // the types it is made of.
        let mut names = Vec::new();
        symbols(call, &mut names);
        let mut declared: BTreeMap<String, Sexp> = BTreeMap::new();
        while let Some(n) = names.pop() {
            if declared.contains_key(&n) {
                continue;
            }
            if let Some(d) = self.declarations.get(&n) {
                symbols(d, &mut names);
                declared.insert(n, d.clone());
            }
        }
        let declared: Vec<(String, Sexp)> = declared.into_iter().collect();
        transform::run(&compiled, call, &declared).map_err(error)
    }

    fn expand_call(&mut self, name: &str, args: &[Sexp], span: reader::Span, depth: usize) -> Result<Sexp, Error> {
        let error = |message: String| Error::Resolve { def: format!("({name} ..)"), message };
        if depth >= EXPANSION_DEPTH {
            return Err(error(format!(
                "expands more than {EXPANSION_DEPTH} macro calls deep: does its expansion always call it again?"
            )));
        }
        self.marks += 1;
        self.macros[name].expand(name, args, self.marks, span).map_err(error)
    }

    fn add_items(&mut self, items: Vec<Item>) -> Result<Vec<Added>, Error> {
        let mut defs = Vec::new();
        let mut claims = Vec::new();
        let mut types = Vec::new();
        for item in items {
            match item {
                Item::Def(d) => defs.push(d),
                Item::Claim(c) => claims.push(c),
                Item::Type(t) => types.push(t),
                Item::Macro(_) => unreachable!("macro rules are closed before anything else"),
                Item::Domain(_) => {
                    return Err(Error::Resolve {
                        def: "(domain ..)".into(),
                        message: "a domain wraps what it names: `(domain io.gozala.parser (define ..) ..)`".into(),
                    });
                }
            }
        }
        for name in defs.iter().map(|d| &d.name).chain(types.iter().map(|t| &t.name)) {
            if self.macros.contains_key(name) || self.macros.contains_key(&format!("{name}!")) {
                return Err(Error::Resolve {
                    def: name.clone(),
                    message: format!("`{name}` is a macro, so it cannot also name a definition or a type"),
                });
            }
        }
        // A claim is the signature of a definition in the same source;
        // a type declares what a name is. Types come first: they declare
        // what definitions use, and nothing about them depends on order.
        let mut signatures: HashMap<String, Claim> = HashMap::new();
        let mut method_claims: Vec<Claim> = Vec::new();
        for c in claims {
            let error = |message: String| Err(Error::Resolve { def: c.name.clone(), message });
            // A claim with no definition, of a function of some type, is a
            // method: each type defines it for itself.
            if !defs.iter().any(|d| d.name == c.name) && is_method_claim(&c) && !self.names.contains_key(&c.name) {
                method_claims.push(c);
                continue;
            }
            if !defs.iter().any(|d| d.name == c.name) {
                let bang = if c.performs { "!" } else { "" };
                return if self.names.contains_key(&c.name) {
                    error(format!(
                        "`{}{bang}` is defined in another source: a claim goes with its definition, in one source",
                        c.name
                    ))
                } else if c.performs {
                    error(format!("`{}!` claims a task, so it goes with a definition of `{}!`", c.name, c.name))
                } else {
                    let head = match &c.params {
                        Some(_) => format!("({} ..)", c.name),
                        None => c.name.clone(),
                    };
                    error(format!(
                        "`{}` has no definition in this source: a claim is a signature; declare a type with \
                         `(type {head} : ..)`",
                        c.name
                    ))
                };
            }
            if signatures.contains_key(&c.name) {
                return error("a definition has one signature".into());
            }
            signatures.insert(c.name.clone(), c);
        }
        for t in &types {
            if defs.iter().any(|d| d.name == t.name) {
                return Err(Error::Resolve {
                    def: t.name.clone(),
                    message: format!(
                        "`{}` names a definition, so it cannot also name a type: its signature is `(claim {} : ..)`",
                        t.name, t.name
                    ),
                });
            }
        }
        let saved = (self.claims.clone(), self.methods.clone());
        let out = self
            .claim(&types, &defs)
            .and_then(|()| self.declare_methods(&method_claims))
            .and_then(|()| self.add_checked(defs, &signatures));
        if out.is_err() {
            (self.claims, self.methods) = saved;
        }
        out
    }

    /// Declare each method a claim with no definition claims: the type
    /// its definitions must fit, read once the source's types are.
    fn declare_methods(&mut self, claims: &[Claim]) -> Result<(), Error> {
        for c in claims {
            let error = |message: String| Err(Error::Resolve { def: c.name.clone(), message });
            if self.claims.claims(&c.name) {
                return error(format!("`{}` is declared as a type or an attribute", c.name));
            }
            let text = c.text();
            if let Some(before) = self.methods.get(&c.name) {
                if before.text != text {
                    return error(format!("the method `{}` is already claimed as `{}`", c.name, before.text));
                }
                continue;
            }
            let params = c.params.as_ref().expect("a method's claim has parameters");
            let mut env = Env::new(Mode::Flexible(&self.solver, 1));
            let read = (|| {
                let ps = self.claims.params(params, &mut env)?;
                let ret = self.claims.ty(&c.ty, &mut env)?;
                claims::function(ps, ret)
            })();
            let ty = match read {
                Ok(t) => t,
                Err(message) => return error(message),
            };
            let Ty::Fn(ps, _, _, rest) = &ty else { unreachable!("a claim of parameters is of a function") };
            let receiver = ps[0].clone();
            let first = params[0].0.clone().unwrap_or_else(|| "value".to_string());
            let labels = std::iter::once(first).chain(rest.iter().cloned()).collect();
            let scheme =
                Scheme { level: 0, body: ty.clone(), requires: vec![Requirement { method: c.name.clone(), receiver }] };
            self.methods.insert(c.name.clone(), Method { scheme, labels, text });
        }
        Ok(())
    }

    /// Record type claims, after checking each agrees with what is
    /// already declared and every type they name is declared.
    fn claim(&mut self, claims: &[Claim], defs: &[Def]) -> Result<(), Error> {
        // Attributes new in this source, whose values' types are read once
        // every type the source declares is, so types may refer to each
        // other in any order.
        let mut fresh: Vec<(String, String, TypeExpr)> = Vec::new();
        for c in claims {
            let error = |message: String| Err(Error::Resolve { def: c.name.clone(), message });
            if self.names.contains_key(&c.name) || defs.iter().any(|d| d.name == c.name) {
                return error(format!("`{}` names a definition, so it cannot also name a type", c.name));
            }
            match (&c.params, &c.ty) {
                (None, TypeExpr::Attribute(global, value, _)) => {
                    self.claim_attribute(&c.name, global, value, &mut fresh)?
                }
                (params, body) => {
                    if self.claims.attributes.contains_key(&c.name) {
                        return error(format!("`{}` is declared as an attribute", c.name));
                    }
                    let mut vars = Vec::new();
                    for (label, t) in params.iter().flatten() {
                        let TypeExpr::Var(v) = t else {
                            return error(format!("a type's parameters are variables, `?a`, not `{t}`"));
                        };
                        vars.push((label.clone(), v.clone()));
                    }
                    let mut free = BTreeSet::new();
                    claims::vars(body, &mut free);
                    if let Some(v) = free.iter().find(|v| !vars.iter().any(|(_, p)| p == *v)) {
                        return error(format!("`?{v}` is not one of the type's parameters"));
                    }
                    let text = c.text();
                    if let Some(before) = self.claims.types.get(&c.name)
                        && before.text != text
                    {
                        return error(format!("`{}` is already declared as `{}`", c.name, before.text));
                    }
                    // An attribute written in place is declared with it.
                    if let TypeExpr::Record(members, _) = body {
                        for m in members {
                            if let syntax::Member::Inline(global, value) = m {
                                self.claim_global(&c.name, global, value, &mut fresh)?;
                            }
                        }
                    }
                    self.claims.types.insert(c.name.clone(), TypeDef { params: vars, body: body.clone(), text });
                }
            }
        }
        let new_globals: Vec<String> = fresh.iter().map(|(_, g, _)| g.clone()).collect();
        for (_, global, value) in fresh {
            // A variable an attribute does not tie to anything else asks
            // nothing of its values.
            let mut free = BTreeSet::new();
            claims::vars(&value, &mut free);
            if !free.is_empty() {
                continue;
            }
            self.claims.attribute_exprs.insert(global.clone(), value);
        }
        for global in new_globals {
            let Some(value) = self.claims.attribute_exprs.get(&global).cloned() else { continue };
            let mut env = Env::new(Mode::Flexible(&self.solver, 1));
            let ty =
                self.claims.ty(&value, &mut env).map_err(|message| Error::Resolve { def: global.clone(), message })?;
            self.claims.attribute_types.get_mut(&global).unwrap().1 = ty;
        }
        // Every type now refers to declared names, and a variant's ranks
        // can each be chosen.
        for c in claims.iter().filter(|c| self.claims.types.contains_key(&c.name)) {
            let def = &self.claims.types[&c.name];
            let mut env = Env::new(Mode::Flexible(&self.solver, 1));
            self.claims.ty(&def.body, &mut env).map_err(|message| Error::Resolve { def: c.name.clone(), message })?;
            if let Some(cases) = self.claims.variant(&c.name) {
                for (i, wider) in cases.iter().enumerate() {
                    for narrower in &cases[i + 1..] {
                        let (w, n) = (self.claims.required(wider), self.claims.required(narrower));
                        if w.is_subset(&n) {
                            return Err(Error::Resolve {
                                def: c.name.clone(),
                                message: format!(
                                    "`{wider}` is ranked above `{narrower}`, but every `{narrower}` also fits `{wider}`, \
                                     so `{narrower}` could never be chosen: rank the narrower case first"
                                ),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn claim_attribute(
        &mut self,
        name: &str,
        global: &str,
        value: &TypeExpr,
        fresh: &mut Vec<(String, String, TypeExpr)>,
    ) -> Result<(), Error> {
        let error = |message: String| Err(Error::Resolve { def: name.to_string(), message });
        if self.claims.types.contains_key(name) {
            return error(format!("`{name}` is declared as a type"));
        }
        if let Some(before) = self.claims.attributes.get(name)
            && before != global
        {
            return error(format!("`{name}` is already declared as the attribute `{before}`"));
        }
        self.claim_global(name, global, value, fresh)?;
        self.claims.attributes.insert(name.to_string(), global.to_string());
        Ok(())
    }

    /// The type of a global name's values, which every claim of it must
    /// agree on. A new one is read with the rest of its source's types:
    /// until then its values ask nothing.
    fn claim_global(
        &mut self,
        name: &str,
        global: &str,
        value: &TypeExpr,
        fresh: &mut Vec<(String, String, TypeExpr)>,
    ) -> Result<(), Error> {
        let error = |message: String| Err(Error::Resolve { def: name.to_string(), message });
        let Some((domain, last)) = global.rsplit_once('/') else { unreachable!() };
        if domain.is_empty() || last.is_empty() {
            return error(format!("`{global}` is not a global name: write `domain/name`"));
        }
        let written = value.to_string();
        if let Some((before, _)) = self.claims.attribute_types.get(global) {
            if *before != written {
                return error(format!("`{global}` is already declared with values `{before}`, not `{written}`"));
            }
            return Ok(());
        }
        self.claims.attribute_types.insert(global.to_string(), (written, Ty::Top));
        fresh.push((name.to_string(), global.to_string(), value.clone()));
        Ok(())
    }

    pub fn add_defs(&mut self, defs: Vec<Def>) -> Result<Vec<Added>, Error> {
        self.add_checked(defs, &HashMap::new())
    }

    fn add_checked(&mut self, defs: Vec<Def>, signatures: &HashMap<String, Claim>) -> Result<Vec<Added>, Error> {
        for d in &defs {
            if self.claims.claims(&d.name) {
                return Err(Error::Resolve {
                    def: d.name.clone(),
                    message: format!(
                        "`{}` is declared as an attribute or a type, so it cannot also name a definition",
                        d.name
                    ),
                });
            }
        }
        // Every definition of a function's name in a compilation is a
        // clause of it, as every rule of a Datalog predicate is.
        let mut batch: Vec<Def> = Vec::new();
        for d in defs {
            let Some(b) = batch.iter_mut().find(|b| b.name == d.name) else {
                batch.push(d);
                continue;
            };
            let clauses = |e: &Expr| match e {
                Expr::Fn(params, body) => Some(vec![Clause::of_fn(params, body)]),
                Expr::Clauses(cs) => Some(cs.clone()),
                _ => None,
            };
            match (clauses(&b.body), clauses(&d.body)) {
                (Some(mut all), Some(more)) => {
                    all.extend(more);
                    b.body = Expr::Clauses(all);
                }
                _ => {
                    return Err(Error::Resolve {
                        def: d.name.clone(),
                        message: format!(
                            "`{}` is defined twice: only a function may have several definitions, one per clause",
                            d.name
                        ),
                    });
                }
            }
        }
        let index: HashMap<&str, usize> = batch.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();

        // Which definitions of the batch each one mentions.
        let deps: Vec<Vec<usize>> = batch
            .iter()
            .map(|d| {
                let mut names = Vec::new();
                free_names(&d.body, &mut Vec::new(), &mut names);
                let mut out: Vec<usize> = names
                    .iter()
                    .filter_map(|n| match in_batch(bare(n), &index) {
                        Ok(Some(full)) => index.get(full.as_str()).copied(),
                        _ => None,
                    })
                    .collect();
                // A method called may be any of this source's definitions
                // of it: which one, checking finds.
                for n in names.iter().map(|n| bare(n)) {
                    if !matches!(in_batch(n, &index), Ok(None)) {
                        continue;
                    }
                    for m in self.methods.keys().filter(|m| *m == n || ends_with_segments(m, n)) {
                        let last = m.rsplit('/').next().unwrap_or(m);
                        out.extend(
                            batch
                                .iter()
                                .enumerate()
                                .filter(|(_, d)| d.name.rsplit('/').next() == Some(last))
                                .map(|(i, _)| i),
                        );
                    }
                }
                out.sort();
                out.dedup();
                out
            })
            .collect();

        let mut resolved: HashMap<String, Hash> = HashMap::new();
        let mut staged: Vec<Definition> = Vec::new();
        let mut added = Vec::new();
        for component in strongly_connected(&deps) {
            let members: Vec<&Def> = component.iter().map(|&i| &batch[i]).collect();
            let recursive = members.len() > 1 || deps[component[0]].contains(&component[0]);
            let names_in_cycle: Vec<&str> = members.iter().map(|d| d.name.as_str()).collect();

            // Resolve with provisional cycle indices: positions in
            // `members`.
            // One try at the component with the names in `chosen` taken
            // as written there: resolve it, hash it, and check it if it is
            // new. A name that could be several things fails the try as
            // ambiguous, and `settle` tries each of them.
            let attempt = |chosen: &HashMap<String, String>| -> Result<Attempt, Failure> {
                let mut terms = Vec::new();
                let ambiguous: std::cell::RefCell<Option<(String, Vec<String>)>> = std::cell::RefCell::new(None);
                for d in &members {
                    let lookup = |name: &str| -> Option<Target> {
                        // A name a macro's template wrote may be pinned to the
                        // definition it named there.
                        if let Some(h) = pin_of(name).and_then(Hash::from_hex) {
                            let labels = match self.defs.get(&h).map(|d| &d.term) {
                                Some(Term::Fn(labels, _)) => Some(labels.clone()),
                                _ => None,
                            };
                            return Some(Target::Hash(h, labels));
                        }
                        let written = bare(name);
                        // A name settled by its type is that name; a built-in
                        // is no definition, so it resolves as built in.
                        let settled = chosen.get(written).cloned();
                        if settled
                            .as_deref()
                            .is_some_and(|f| Op::from_written(f).is_some_and(|op| op.whole_name() == f))
                        {
                            return None;
                        }
                        // A bare name of a method means the method, not one
                        // of the definitions of it each type gives, unless a
                        // definition has that whole name.
                        if settled.is_none()
                            && !index.contains_key(written)
                            && !self.names.contains_key(written)
                            && let Some(m) = self.method_named(written)
                        {
                            return Some(Target::Method(m.to_string(), self.methods[m].labels.clone()));
                        }
                        // A definition of this source comes before what the
                        // codebase holds.
                        let found = match settled {
                            Some(full) => Ok(Some(full)),
                            // A definition of this source that a built-in's
                            // name also names is as ambiguous as two
                            // definitions would be: the use decides, or the
                            // name must be written whole.
                            None => match (in_batch(written, &index), builtin_named(written)) {
                                (Ok(Some(full)), Some(op)) => Err(vec![full, op.whole_name()]),
                                (other, _) => other,
                            },
                        };
                        let name = match found {
                            Ok(Some(full)) => full,
                            Err(candidates) => {
                                ambiguous.replace(Some((written.to_string(), candidates)));
                                return None;
                            }
                            Ok(None) => match self
                                .resolve_name(written, &|n| self.names.contains_key(n) || self.methods.contains_key(n))
                            {
                                Ok(Some(full)) => full,
                                Ok(None) => return None,
                                Err(candidates) => {
                                    ambiguous.replace(Some((written.to_string(), candidates)));
                                    return None;
                                }
                            },
                        };
                        let name = name.as_str();
                        if let Some(m) = self.methods.get(name) {
                            return Some(Target::Method(name.to_string(), m.labels.clone()));
                        }
                        if let Some(i) = names_in_cycle.iter().position(|n| *n == name)
                            && recursive
                        {
                            let labels = match &members[i].body {
                                Expr::Fn(params, _) => Some(canonical(params).into_iter().map(|p| p.label).collect()),
                                Expr::Clauses(clauses) => clause_labels(clauses).ok(),
                                _ => None,
                            };
                            return Some(Target::Cycle(i as u32, labels));
                        }
                        let h = resolved.get(name).or_else(|| self.names.get(name))?;
                        let def = staged.iter().find(|s| s.hash == *h).or_else(|| self.defs.get(h));
                        // A call passes its own parameters; the methods a
                        // definition is passed, checking passes.
                        let labels = match def {
                            Some(d) if matches!(d.term, Term::Fn(..)) => Some(split_labels(d).0),
                            _ => None,
                        };
                        Some(Target::Hash(*h, labels))
                    };
                    let domain = d.name.rsplit_once('/').map_or("", |(home, _)| home);
                    let names = Names { lookup: &lookup, claims: &self.claims, macros: &self.macros, domain };
                    let resolved_term = resolve(&d.body, &mut Vec::new(), &names);
                    // A name that could be several things is an error even
                    // where resolution went on to read it as a built-in.
                    let resolved_term = match (resolved_term, ambiguous.borrow().clone()) {
                        (Ok(_), Some((name, _))) => Err(ResolveError::Unbound(name)),
                        (other, _) => other,
                    };
                    // Whatever went wrong after a name was ambiguous may be
                    // the ambiguity's doing: read as a built-in, it is no
                    // value, say. Settling it comes first.
                    let term = resolved_term.map_err(|e| match (e, ambiguous.take()) {
                        (_, Some((name, candidates))) => Failure::Ambiguous { def: d.name.clone(), name, candidates },
                        (ResolveError::Unbound(name), None) => {
                            Failure::Error(Error::Unbound { name, def: d.name.clone() })
                        }
                        (ResolveError::Invalid(message), None) => {
                            Failure::Error(Error::Resolve { def: d.name.clone(), message })
                        }
                    })?;
                    terms.push(term);
                }

                // A signature is part of what a definition is: callers see
                // its type.
                let claim_text = |i: usize| signatures.get(&members[i].name).map(|c| c.text()).unwrap_or_default();
                let member_names: Vec<&str> = members.iter().map(|d| d.name.as_str()).collect();
                // Code that calls methods, or definitions that ask for them,
                // is what it is once checking finds which: it is hashed
                // after it is checked, as it then is.
                let scheme_of = |h: &Hash| {
                    staged
                        .iter()
                        .find(|d| d.hash == *h)
                        .map(|d| &d.scheme)
                        .or_else(|| self.defs.get(h).map(|d| &d.scheme))
                };
                let asks = |h: &Hash| scheme_of(h).is_some_and(|s| !s.requires.is_empty());
                let elaborate = terms.iter().any(|t| methods::has_sites(t, &asks));
                if elaborate {
                    let mut next = 0;
                    terms = terms.into_iter().map(|t| methods::mark_sites(t, &mut next, &asks, recursive)).collect();
                }
                let (mut hashes, mut cycle_order) = hash_members(&mut terms, recursive, &claim_text, &member_names);
                let is_new = |hashes: &[Hash]| {
                    !self.defs.contains_key(&hashes[0]) && !staged.iter().any(|d| d.hash == hashes[0])
                };
                let schemes = if elaborate {
                    let order: &[usize] = if recursive { &cycle_order } else { &[] };
                    let (schemes, elaborated) = self
                        .check(&members, &terms, order, &staged, signatures, Some(&resolved))
                        .map_err(Failure::Error)?;
                    let mut elaborated = elaborated.expect("checking with sites rewrites them");
                    if recursive {
                        let back = cycle_order.clone();
                        for t in elaborated.iter_mut() {
                            renumber_cycle(t, &|c| back[c as usize] as u32);
                        }
                    }
                    (hashes, cycle_order) = hash_members(&mut elaborated, recursive, &claim_text, &member_names);
                    terms = elaborated;
                    is_new(&hashes).then_some(schemes)
                } else if is_new(&hashes) {
                    let order: &[usize] = if recursive { &cycle_order } else { &[] };
                    Some(self.check(&members, &terms, order, &staged, signatures, None).map_err(Failure::Error)?.0)
                } else {
                    None
                };
                let cycle: Vec<Hash> =
                    if recursive { cycle_order.iter().map(|&p| hashes[p]).collect() } else { vec![] };
                Ok(Attempt { terms, hashes, cycle, schemes })
            };
            let Attempt { terms, hashes, cycle, schemes } =
                settle(&attempt, HashMap::new()).map_err(Failure::into_error)?;
            let fresh = schemes.is_some();
            if let Some(schemes) = schemes {
                for ((term, scheme), hash) in terms.into_iter().zip(schemes).zip(hashes.iter()) {
                    staged.push(Definition { hash: *hash, term, cycle: cycle.clone(), scheme });
                }
            }
            for (d, h) in members.iter().zip(hashes.iter()) {
                resolved.insert(d.name.clone(), *h);
                added.push(Added { name: d.name.clone(), hash: *h, new: fresh });
            }
        }

        for d in staged {
            self.defs.insert(d.hash, d);
        }
        for (name, hash) in resolved {
            self.names.insert(name, hash);
        }
        // Report in source order.
        added.sort_by_key(|a| index[a.name.as_str()]);
        Ok(added)
    }

    /// Type check one component and return each member's scheme. With
    /// the names this source has defined so far, its terms have sites,
    /// and each member also comes back with what its sites found passed.
    fn check(
        &self,
        members: &[&Def],
        terms: &[Term],
        cycle_order: &[usize],
        staged: &[Definition],
        signatures: &HashMap<String, Claim>,
        defined: Option<&HashMap<String, Hash>>,
    ) -> Result<(Vec<Scheme>, Option<Vec<Term>>), Error> {
        struct Layered<'a> {
            defs: &'a HashMap<Hash, Definition>,
            staged: &'a [Definition],
            attributes: &'a BTreeMap<String, (String, Ty)>,
            hosts: &'a BTreeMap<String, BTreeMap<String, Hash>>,
            methods: &'a BTreeMap<String, Method>,
        }
        impl Globals for Layered<'_> {
            fn scheme(&self, hash: &Hash) -> Option<Scheme> {
                self.defs.get(hash).or_else(|| self.staged.iter().find(|d| d.hash == *hash)).map(|d| d.scheme.clone())
            }
            fn attribute(&self, global: &str) -> Option<Ty> {
                self.attributes.get(global).map(|(_, t)| t.clone())
            }
            fn operations(&self, capability: &str) -> Vec<(String, Scheme)> {
                self.hosts
                    .get(capability)
                    .into_iter()
                    .flatten()
                    .filter_map(|(name, h)| Some((name.clone(), self.scheme(h)?)))
                    .collect()
            }
            fn method(&self, name: &str) -> Option<Scheme> {
                self.methods.get(name).map(|m| m.scheme.clone())
            }
        }
        let globals = Layered {
            defs: &self.defs,
            staged,
            attributes: &self.claims.attribute_types,
            hosts: &self.hosts,
            methods: &self.methods,
        };
        let s = &self.solver;
        // Cycle members are known to each other monomorphically while
        // their bodies are checked, by their canonical index.
        let member_vars: Vec<Ty> = (0..members.len()).map(|_| s.fresh_ty(1)).collect();
        // `terms` are in provisional order, and the cycle references in
        // them are canonical: canonical position i is provisional member
        // `cycle_order[i]`.
        let cycle_types: Vec<Ty> = cycle_order.iter().map(|&p| member_vars[p].clone()).collect();
        // Outside any function, code must be pure: a top-level value is
        // computed once, and nothing may happen when it is.
        let infer = Infer {
            solver: s,
            globals: &globals,
            cycle: cycle_types,
            effects: std::cell::RefCell::new(vec![None]),
            sites: std::cell::RefCell::new(Vec::new()),
        };
        let type_error = |d: &Def, error: TypeError| Error::Type { def: d.name.clone(), error };
        let fail = |d: &Def, message: String| Error::Type { def: d.name.clone(), error: TypeError(message) };
        for (i, (d, term)) in members.iter().zip(terms).enumerate() {
            let t = infer.term(term, &mut Vec::<Binding>::new(), 1).map_err(|e| type_error(d, e))?;
            s.constrain(&t, &member_vars[i]).map_err(|e| type_error(d, e))?;
        }
        let Some(defined) = defined else {
            // A signature is checked against what was inferred, and is
            // what callers then see.
            let mut schemes = Vec::new();
            for (i, (d, term)) in members.iter().zip(terms).enumerate() {
                let inferred = Scheme::new(0, member_vars[i].clone());
                schemes.push(match signatures.get(&d.name) {
                    Some(claim) => self.signature(claim, term, &inferred).map_err(|m| fail(d, m))?,
                    None => Scheme::new(0, s.compact(&member_vars[i], 0)),
                });
            }
            return Ok((schemes, None));
        };
        // A claim says what a definition's parameters are, and so what
        // the methods it calls are called on.
        for (i, d) in members.iter().enumerate() {
            if let Some(claim) = signatures.get(&d.name) {
                let mut env = Env::new(Mode::Flexible(s, 1));
                let (claimed, _) = self.claimed_type(claim, &mut env).map_err(|m| fail(d, m))?;
                s.constrain(&member_vars[i], &claimed)
                    .map_err(|e| fail(d, format!("the definition does not fit its claim: {e}")))?;
            }
        }
        let sites = infer.sites.take();
        let inputs = methods::inputs(&member_vars);
        let lookup =
            Lookup { members, terms, member_vars: &member_vars, cycle_order, staged, defined, inputs: &inputs };
        let (found, needed) = self.find_methods(&sites, &lookup).map_err(|(i, m)| fail(members[i], m))?;
        if !needed.is_empty()
            && let Some((d, _)) = members.iter().zip(terms).find(|(_, t)| !matches!(t, Term::Fn(..)))
        {
            return Err(fail(
                d,
                format!(
                    "`{}` is called on a value whose type is not known here, which only a function's caller can \
                     say: make `{}` a function, or say the value's type",
                    needed[0].method, d.name
                ),
            ));
        }
        let hidden: Vec<String> = needed.iter().enumerate().map(|(j, n)| format!("#{j} {}", n.method)).collect();
        let cycle_labels: Vec<Vec<String>> = cycle_order.iter().map(|&p| fn_labels(&terms[p])).collect();
        let definition_labels = |h: &Hash| -> (Vec<String>, Vec<String>) {
            let def = self.defs.get(h).or_else(|| staged.iter().find(|d| d.hash == *h)).expect("a site's definition");
            split_labels(def)
        };
        let elaborator = methods::Elaborator {
            found: &found,
            hidden: &hidden,
            cycle_labels: &cycle_labels,
            definition_labels: &definition_labels,
        };
        let elaborated: Vec<Term> = terms.iter().map(|t| elaborator.member(t)).collect();
        let receivers: Vec<Ty> = needed.iter().map(|n| n.receiver.clone()).collect();
        let mut schemes = Vec::new();
        for (i, (d, term)) in members.iter().zip(terms).enumerate() {
            let requires: Vec<Requirement> =
                needed.iter().map(|n| Requirement { method: n.method.clone(), receiver: n.receiver.clone() }).collect();
            schemes.push(match signatures.get(&d.name) {
                Some(claim) => {
                    let inferred = Scheme { level: 0, body: member_vars[i].clone(), requires };
                    self.signature(claim, term, &inferred).map_err(|m| fail(d, m))?
                }
                None => {
                    let (body, on) = s.compact_with(&member_vars[i], &receivers, 0);
                    let requires = requires
                        .into_iter()
                        .zip(on)
                        .map(|(r, receiver)| Requirement { method: r.method, receiver })
                        .collect();
                    Scheme { level: 0, body, requires }
                }
            });
        }
        Ok((schemes, Some(elaborated)))
    }

    /// What each site's calls of methods call: for each site, a
    /// definition of each method it needs, or the method the definitions
    /// being checked are passed for it; and what those are passed,
    /// each with the type it is called on. On an error, the member it is
    /// in.
    fn find_methods(
        &self,
        sites: &[crate::infer::Site],
        lookup: &Lookup,
    ) -> Result<(SiteImpls, Vec<Needed>), (usize, String)> {
        // Finding a method can tell more about the types other calls are
        // on, which a second look then finds known.
        let mut round = 0;
        loop {
            let mut needed = Vec::new();
            let mut found = HashMap::new();
            for site in sites {
                if matches!(site.kind, crate::infer::SiteKind::Cycle(_)) {
                    continue;
                }
                let mut impls = Vec::new();
                for (method, on) in &site.needs {
                    let member = lookup.member_of(site.id);
                    impls.push(self.find_method(method, on, &mut needed, lookup, 0).map_err(|m| (member, m))?);
                }
                found.insert(site.id, impls);
            }
            let settled = needed.iter().all(|n| matches!(methods::receiver(&n.receiver), Receiver::Unknown(_)));
            if settled || round == 3 {
                return Ok((found, needed));
            }
            round += 1;
        }
    }

    fn find_method(
        &self,
        method: &str,
        on: &Ty,
        needed: &mut Vec<Needed>,
        lookup: &Lookup,
        depth: usize,
    ) -> Result<Impl, String> {
        if depth > 32 {
            return Err(format!(
                "finding `{method}` needs it again for the types inside, without end: a definition of it for a type \
                 that holds itself needs its claim, `(claim (name (type ?a) ..) : ..)`, to know that what it holds is \
                 of that type"
            ));
        }
        let last = method.rsplit('/').next().unwrap_or(method);
        let defaulted = methods::DEFAULTED.contains(&method);
        match methods::receiver(on) {
            Receiver::Invalid(_) if defaulted => self.derive(method, on, needed, lookup, depth),
            Receiver::Invalid(what) => Err(format!("`{method}` is called on {what}")),
            Receiver::Unknown(sources) => {
                // A variable a `let` made generic stands for many types,
                // each use's own: no one method is passed for it.
                if sources.iter().any(|t| t.level() > 1) {
                    return Err(format!(
                        "`{method}` is called inside a function a `let` binds, on a value of a type the `let` makes \
                         generic: define the function at the top level, where its callers say the type"
                    ));
                }
                // Only what callers give is theirs to say; anything else
                // that is unknown is nothing at all.
                let from_callers: Vec<Ty> =
                    sources.into_iter().filter(|t| matches!(t, Ty::Var(v) if lookup.inputs.contains(&v.id))).collect();
                // A method with a default needs nothing from callers for
                // a value that must be of a built-in type, and for one no
                // caller says the type of, as a function value's
                // parameter, goes the built-in's way.
                if defaulted && (from_callers.is_empty() || from_callers.iter().all(methods::built_in_above)) {
                    return Ok(Impl::Derived {
                        method: method.to_string(),
                        labels: self.methods[method].labels.clone(),
                        shape: Shape::Plain,
                        parts: vec![],
                    });
                }
                if from_callers.is_empty() {
                    return Ok(Impl::Nothing(self.methods[method].labels.clone()));
                }
                let key = methods::key(&from_callers);
                // Values that all come from one variable: the method is
                // needed for it, the type callers say.
                let on = match from_callers.as_slice() {
                    [one] => one,
                    _ => on,
                };
                let at = match needed.iter().position(|n| n.method == method && n.key == key) {
                    Some(j) => j,
                    None => {
                        needed.push(Needed { method: method.to_string(), receiver: on.clone(), key });
                        needed.len() - 1
                    }
                };
                Ok(Impl::Passed(at))
            }
            Receiver::Known(domains, shown) => {
                for domain in &domains {
                    let name = format!("{domain}/{last}");
                    if let Some(target) = lookup.find(self, &name)? {
                        return self.use_method(method, on, &name, target, needed, lookup, depth);
                    }
                }
                if defaulted {
                    return self.derive(method, on, needed, lookup, depth);
                }
                Err(format!("{shown} has no `{last}`: `{method}` is defined for it as `{}/{last}`", domains[0]))
            }
        }
    }

    /// A method with a default, for a type that does not define it: what
    /// the shape of the type says, with the method found for each part.
    /// Past a depth, and for what has no parts the method goes through,
    /// the built-in's way.
    fn derive(
        &self,
        method: &str,
        on: &Ty,
        needed: &mut Vec<Needed>,
        lookup: &Lookup,
        depth: usize,
    ) -> Result<Impl, String> {
        let labels = self.methods[method].labels.clone();
        let made = |shape: Shape, parts: Vec<Impl>| Impl::Derived {
            method: method.to_string(),
            labels: labels.clone(),
            shape,
            parts,
        };
        let plain = || made(Shape::Plain, vec![]);
        if depth > 8 {
            return Ok(plain());
        }
        let (known, vars) = methods::sources(on);
        let from_callers = vars.iter().any(|t| matches!(t, Ty::Var(v) if lookup.inputs.contains(&v.id)));
        if from_callers && !known.is_empty() {
            return Err(format!(
                "`{method}` is called on a value whose type is known in some places it comes from and not in others: \
                 say which type it is, or call it where it is one"
            ));
        }
        let union = |ts: Vec<Ty>| if ts.len() == 1 { ts.into_iter().next().unwrap() } else { Ty::Union(Rc::new(ts)) };
        let local = |global: &str| {
            self.claims.local(global).unwrap_or_else(|| global.rsplit('/').next().unwrap_or(global).to_string())
        };
        let last = method.rsplit('/').next().unwrap_or(method);
        // What flows in, sorted: variants by key, records by their fields,
        // lists by their items, and anything else.
        let mut variants: BTreeMap<String, Vec<Ty>> = BTreeMap::new();
        let mut records: Vec<BTreeMap<String, Ty>> = Vec::new();
        let mut items: Vec<Ty> = Vec::new();
        let mut other = false;
        for k in &known {
            match k {
                Ty::Tags(tags) => {
                    for (key, payload) in tags.iter() {
                        match payload {
                            Some(t) => variants.entry(key.clone()).or_default().push(t.clone()),
                            None => other = true,
                        }
                    }
                }
                Ty::Update(_, key, v, true) => variants.entry(key.clone()).or_default().push((**v).clone()),
                Ty::Update(r, key, v, false) => {
                    let (inner, _) = methods::sources(r);
                    for i in inner {
                        let mut fields = match i {
                            Ty::Record(fields) => (*fields).clone(),
                            // A record of one attribute.
                            Ty::Tags(tags) if tags.len() == 1 && tags.values().all(Option::is_some) => {
                                tags.iter().map(|(k, t)| (k.clone(), t.clone().unwrap())).collect()
                            }
                            _ => {
                                other = true;
                                continue;
                            }
                        };
                        fields.insert(key.clone(), (**v).clone());
                        records.push(fields);
                    }
                }
                Ty::Record(fields) => records.push((**fields).clone()),
                Ty::List(t) => items.push((**t).clone()),
                Ty::Bot => {}
                _ => other = true,
            }
        }
        // A variant is a record of one attribute: among records, it is one.
        if !records.is_empty() {
            for (key, payloads) in std::mem::take(&mut variants) {
                records.push(BTreeMap::from([(key, union(payloads))]));
            }
        }
        if !variants.is_empty() {
            let mut shown = Vec::new();
            let mut parts = Vec::new();
            for (key, payloads) in variants {
                let domain = key.rsplit_once('/').map_or("", |(d, _)| d);
                let name = format!("{domain}/{last}");
                let payload = union(payloads);
                let found = if domain.is_empty() { None } else { lookup.find(self, &name)? };
                match found {
                    Some(target) => {
                        let whole = Ty::Tags(Rc::new(BTreeMap::from([(key.clone(), Some(payload))])));
                        parts.push(self.use_method(method, &whole, &name, target, needed, lookup, depth + 1)?);
                        shown.push((key.clone(), local(&key), true));
                    }
                    None => {
                        parts.push(self.find_method(method, &payload, needed, lookup, depth + 1)?);
                        shown.push((key.clone(), local(&key), false));
                    }
                }
            }
            return Ok(made(Shape::Variants(shown), parts));
        }
        if other || (!records.is_empty() && !items.is_empty()) {
            return Ok(plain());
        }
        if !records.is_empty() {
            let mut keys: Vec<String> = records.iter().flat_map(|r| r.keys().cloned()).collect();
            keys.sort();
            keys.dedup();
            let mut fields: Vec<(String, String, bool)> = Vec::new();
            let mut types: Vec<Ty> = Vec::new();
            for key in keys {
                let mut optional = false;
                let mut ts = Vec::new();
                for r in &records {
                    match r.get(&key) {
                        Some(Ty::Opt(t)) => {
                            optional = true;
                            ts.push((**t).clone());
                        }
                        Some(t) => ts.push(t.clone()),
                        None => optional = true,
                    }
                }
                fields.push((key.clone(), local(&key), optional));
                types.push(union(ts));
            }
            // Shown in the order of their local names, as `inspect` shows
            // them.
            let mut order: Vec<usize> = (0..fields.len()).collect();
            order.sort_by(|&a, &b| fields[a].1.cmp(&fields[b].1));
            let mut parts = Vec::new();
            for &i in &order {
                parts.push(self.find_method(method, &types[i], needed, lookup, depth + 1)?);
            }
            let fields = order.into_iter().map(|i| fields[i].clone()).collect();
            return Ok(made(Shape::Record(fields), parts));
        }
        if !items.is_empty() {
            let part = self.find_method(method, &union(items), needed, lookup, depth + 1)?;
            return Ok(made(Shape::List, vec![part]));
        }
        if known.is_empty() {
            return Ok(Impl::Nothing(labels));
        }
        Ok(plain())
    }

    /// The definition `name` of `method`, for a call on `on`: checked to
    /// fit the method's claim there, with the methods it needs in turn.
    #[allow(clippy::too_many_arguments)]
    fn use_method(
        &self,
        method: &str,
        on: &Ty,
        name: &str,
        target: Found,
        needed: &mut Vec<Needed>,
        lookup: &Lookup,
        depth: usize,
    ) -> Result<Impl, String> {
        let s = &self.solver;
        let claim = &self.methods[method];
        // The claim with its type variable the type called on.
        let (want, receivers) = s.instantiate_with(&claim.scheme, 1);
        s.constrain(on, &receivers[0]).map_err(|e| e.to_string())?;
        s.constrain(&receivers[0], on).map_err(|e| e.to_string())?;
        let labels = |labels: &[String]| -> Result<(), String> {
            if labels.get(1..) == claim.labels.get(1..) {
                return Ok(());
            }
            Err(format!(
                "`{name}` takes `{}`, and the claim of `{method}` `{}`",
                text(labels.get(1..).unwrap_or(&[])),
                text(&claim.labels[1..])
            ))
        };
        let fit = |e: TypeError| format!("`{name}` does not fit the claim of `{method}`: {e}");
        match target {
            Found::Member(canonical, ty, own) => {
                labels(&own)?;
                s.constrain(&ty, &want).map_err(fit)?;
                Ok(Impl::Member(canonical))
            }
            Found::Stored(hash, scheme, own, hidden) => {
                labels(&own)?;
                let (ty, ons) = s.instantiate_with(&scheme, 1);
                s.constrain(&ty, &want).map_err(fit)?;
                let needs = scheme
                    .requires
                    .iter()
                    .zip(ons)
                    .map(|(r, on)| self.find_method(&r.method, &on, needed, lookup, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Impl::Found { target: hash, needs, labels: own, hidden })
            }
        }
    }

    /// The type a claim writes, read in `env`, and the variable for the
    /// capabilities it leaves to inference, if it does.
    fn claimed_type(&self, claim: &Claim, env: &mut Env) -> Result<(Ty, Option<Ty>), String> {
        let s = &self.solver;
        let params = match &claim.params {
            Some(params) => Some(self.claims.params(params, env)?),
            None => None,
        };
        let ret = self.claims.ty(&claim.ty, env)?;
        // A claim may write the whole type, as it is shown: a task, or a
        // function that answers one.
        let task = |t: &TypeExpr| matches!(t, TypeExpr::App(head, _, _) if head == "task");
        let whole = claim.params.is_none()
            && claim.with.is_none()
            && (task(&claim.ty) || matches!(&claim.ty, TypeExpr::Fn(_, w, r) if w.is_some() || task(r)));
        let (ret, inferred) = if claim.performs && !whole {
            match &claim.with {
                Some(w) => (Ty::task(ret, self.claims.ty(w, env)?), None),
                None => {
                    let e = env.inferred(s, 1);
                    (Ty::task(ret, e.clone()), Some(e))
                }
            }
        } else {
            (ret, None)
        };
        match params {
            Some(params) => Ok((claims::function(params, ret)?, inferred)),
            None => Ok((ret, inferred)),
        }
    }

    /// The scheme a signature claims for `term`, once checked against the
    /// scheme inferred for it: its type, and the methods inferred to be
    /// needed, each on the claim's variable that is the type it is called
    /// on.
    fn signature(&self, claim: &Claim, term: &Term, inferred: &Scheme) -> Result<Scheme, String> {
        let s = &self.solver;
        if let Some(params) = &claim.params {
            let Term::Fn(labels, _) = term else {
                return Err("the claim is of a function, and the definition is not one".into());
            };
            let (first, rest) = params.split_first().ok_or("a function takes at least one parameter")?;
            if let Some(l) = &first.0
                && *l != labels[0]
            {
                return Err(format!("the claim's first parameter is `{l}:`, the definition's `{}:`", labels[0]));
            }
            let mut claimed: Vec<&str> = rest.iter().filter_map(|(l, _)| l.as_deref()).collect();
            claimed.sort();
            let defined: Vec<&str> = labels[1..].iter().map(String::as_str).collect();
            if claimed != defined {
                let text = |ls: &[&str]| ls.iter().map(|l| format!("{l}:")).collect::<Vec<_>>().join(" ");
                return Err(format!(
                    "the claim labels the parameters `{}`, the definition `{}`",
                    text(&claimed),
                    text(&defined)
                ));
            }
        }
        let performs = matches!(term, Term::Task(_)) || matches!(term, Term::Fn(_, b) if matches!(**b, Term::Task(_)));
        if claim.performs != performs {
            return Err(if performs {
                "the definition is marked `!`, so its claim is too".into()
            } else {
                "the claim is marked `!`, so the definition must be too".into()
            });
        }
        // Checked with its variables as types the caller chooses...
        let mut rigid_env = Env::new(Mode::Rigid(s));
        let (rigid, effect) = self.claimed_type(claim, &mut rigid_env)?;
        let (instance, ons) = s.instantiate_with(inferred, 1);
        s.constrain(&instance, &rigid).map_err(|e| format!("the definition does not fit its claim: {e}"))?;
        // ...and shown to callers with them free. Capabilities it left
        // out are what the body was found to use: named ones, and those
        // of the functions it was given, which are its own variables.
        let mut open_env = Env::new(Mode::Flexible(s, 1));
        let (open, open_effect) = self.claimed_type(claim, &mut open_env)?;
        if let (Some(found), Some(shown)) = (effect, open_effect) {
            for bound in lower_bounds(&found) {
                let bound = match bound {
                    Ty::Rigid(name) => open_env.vars[&*name].clone(),
                    other => other,
                };
                s.constrain(&bound, &shown).map_err(|e| e.to_string())?;
            }
        }
        // A method needed is on whichever of the claim's variables its
        // values came from.
        let mut requires = Vec::new();
        for (r, on) in inferred.requires.iter().zip(ons) {
            let receiver = match methods::rigid_names(&on).as_slice() {
                [name] => open_env.vars[name].clone(),
                [] => {
                    return Err(format!(
                        "`{}` is called on a value whose type the claim does not say: say it with a type variable, \
                         `?a`",
                        r.method
                    ));
                }
                names => {
                    return Err(format!(
                        "`{}` is called on values of the claim's `?{}`, which may differ: one method is passed for \
                         each",
                        r.method,
                        names.join("` and `?")
                    ));
                }
            };
            requires.push(Requirement { method: r.method.clone(), receiver });
        }
        Ok(Scheme { level: 0, body: open, requires })
    }
}

/// What each site was found to call, by its number.
type SiteImpls = HashMap<u32, Vec<Impl>>;

/// A method being passed: its claim's whole name, the type it is called
/// on, and the variables that type's values come from, by which calls
/// on the same values share one.
struct Needed {
    method: String,
    receiver: Ty,
    key: Vec<u32>,
}

/// A definition of a method, found by name.
enum Found {
    /// A member of the cycle being checked: its canonical index, its
    /// type, and its parameters' labels.
    Member(u32, Ty, Vec<String>),
    /// A definition already checked: its hash, scheme, own parameters'
    /// labels, and the labels it takes the methods it needs by.
    Stored(Hash, Scheme, Vec<String>, Vec<String>),
}

/// Where a component's checking finds definitions of methods.
struct Lookup<'a> {
    members: &'a [&'a Def],
    terms: &'a [Term],
    member_vars: &'a [Ty],
    cycle_order: &'a [usize],
    staged: &'a [Definition],
    /// The names this source has defined, in components checked before.
    defined: &'a HashMap<String, Hash>,
    /// The variables callers' values flow into: see [`methods::inputs`].
    inputs: &'a std::collections::HashSet<u32>,
}

impl Lookup<'_> {
    fn find(&self, cb: &Codebase, name: &str) -> Result<Option<Found>, String> {
        if let Some(p) = self.members.iter().position(|d| d.name == name) {
            // A member is one only of a cycle: one that calls itself.
            let Some(canonical) = self.cycle_order.iter().position(|&q| q == p) else { return Ok(None) };
            let labels = fn_labels(&self.terms[p]);
            if labels.is_empty() {
                return Err(format!("`{name}` is not a function"));
            }
            return Ok(Some(Found::Member(canonical as u32, self.member_vars[p].clone(), labels)));
        }
        let Some(hash) = self.defined.get(name).or_else(|| cb.names.get(name)) else { return Ok(None) };
        let def = cb.defs.get(hash).or_else(|| self.staged.iter().find(|d| d.hash == *hash)).expect("a defined name");
        if !matches!(def.term, Term::Fn(..)) {
            return Err(format!("`{name}` is not a function"));
        }
        let (own, hidden) = split_labels(def);
        Ok(Some(Found::Stored(*hash, def.scheme.clone(), own, hidden)))
    }

    /// The member a site is in.
    fn member_of(&self, site: u32) -> usize {
        self.terms
            .iter()
            .position(|t| {
                let mut found = false;
                t.walk(&mut |t| found |= matches!(t, Term::Site(id, _) if *id == site));
                found
            })
            .unwrap_or(0)
    }
}

/// A function's parameters' labels, or none.
fn fn_labels(term: &Term) -> Vec<String> {
    match term {
        Term::Fn(labels, _) => labels.clone(),
        _ => Vec::new(),
    }
}

/// A stored function's own parameters' labels, and the labels it takes
/// the methods it needs by, which come after them.
fn split_labels(def: &Definition) -> (Vec<String>, Vec<String>) {
    let labels = fn_labels(&def.term);
    let own = labels.len() - def.scheme.requires.len().min(labels.len());
    (labels[..own].to_vec(), labels[own..].to_vec())
}

/// What flows into a variable, through the variables that flow into it.
fn lower_bounds(t: &Ty) -> Vec<Ty> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    fn go(t: &Ty, out: &mut Vec<Ty>, seen: &mut BTreeSet<u32>) {
        match t {
            Ty::Var(v) => {
                if seen.insert(v.id) {
                    for l in v.lower.borrow().iter() {
                        go(l, out, seen);
                    }
                }
            }
            other => out.push(other.clone()),
        }
    }
    go(t, &mut out, &mut seen);
    out
}

enum ResolveError {
    Unbound(String),
    Invalid(String),
}

/// What a name refers to outside the definition: a definition by hash,
/// or a member of the cycle being resolved, with its parameters' labels
/// when it is a function.
enum Target {
    Hash(Hash, Option<Vec<String>>),
    Cycle(u32, Option<Vec<String>>),
    /// A method, by the whole name of its claim, and its labels.
    Method(String, Vec<String>),
}

/// Everything resolution looks names up in.
struct Names<'a> {
    lookup: &'a dyn Fn(&str) -> Option<Target>,
    claims: &'a Claims,
    macros: &'a BTreeMap<String, macros::Macro>,
    /// The domain of the definition being resolved, which alone may take
    /// apart the opaque types it defines.
    domain: &'a str,
}

/// The domains whose variants only their own definitions may take apart:
/// a secret is made by anyone and opened only by `reveal`.
const OPAQUE: &[&str] = &["conduct.secret"];

impl Names<'_> {
    /// A local may not take a macro's name: expansion does not see
    /// locals, so a call of it would expand the macro.
    fn local(&self, name: &str) -> Result<(), ResolveError> {
        let name = bare(name);
        if self.macros.contains_key(name) || self.macros.contains_key(&format!("{name}!")) {
            return Err(ResolveError::Invalid(format!("`{name}` is a macro, so it cannot also name a local")));
        }
        Ok(())
    }

    /// The global name of an attribute, written by its local name or its
    /// global one.
    fn attribute(&self, name: &str) -> Option<String> {
        self.claims.attribute(name)
    }

    /// An error if `global` is an attribute of an opaque domain other
    /// than the one being resolved in, which a pattern may not take apart.
    fn open(&self, global: &str) -> Result<(), ResolveError> {
        let home = global.rsplit_once('/').map_or("", |(d, _)| d);
        if OPAQUE.contains(&home) && home != self.domain {
            let local = global.rsplit('/').next().unwrap_or(global);
            return Err(ResolveError::Invalid(format!(
                "a `{local}` is opaque, and only `{home}` takes one apart: `reveal` gives what it holds"
            )));
        }
        Ok(())
    }

    fn key(&self, name: &str) -> Result<String, ResolveError> {
        self.attribute(name).ok_or_else(|| {
            ResolveError::Invalid(if name.contains('/') {
                format!("no type declares the attribute `{name}`")
            } else {
                format!("`{name}:` is not a declared attribute: declare it, `(type {name} : (domain/{name} type))`")
            })
        })
    }

    /// A pattern with its record keys resolved to global names, and the
    /// record type it names, if it is a constructor.
    fn pattern(&self, p: &Pat) -> Result<(Pattern, Option<String>), ResolveError> {
        Ok(match p {
            Pat::Keyword(k) => (Pattern::Keyword(k.clone()), None),
            Pat::Str(s) => (Pattern::Str(s.clone()), None),
            Pat::Int(n) => (Pattern::Int(*n), None),
            Pat::Bool(b) => (Pattern::Bool(*b), None),
            Pat::Any => (Pattern::Any, None),
            Pat::Name(n) if self.claims.record(bare(n)).is_some() => {
                (self.constructor(bare(n), &[])?, Some(bare(n).to_string()))
            }
            Pat::Name(n) => (Pattern::Bind(n.clone()), None),
            Pat::Record(fields) => {
                let mut out: Vec<(String, Pattern)> = Vec::new();
                for (k, p) in fields {
                    let global = self.key(k)?;
                    self.open(&global)?;
                    if out.iter().any(|(g, _)| *g == global) {
                        return Err(ResolveError::Invalid(format!("the attribute `{global}` appears twice")));
                    }
                    out.push((global, self.pattern(p)?.0));
                }
                out.sort_by(|a, b| a.0.cmp(&b.0));
                (Pattern::Fields(out), None)
            }
            Pat::Ctor(name, args) => (self.constructor(name, args)?, Some(name.clone())),
            Pat::Rest(name, p) => {
                let (inner, ctor) = self.pattern(p)?;
                (Pattern::Rest(name.clone(), Box::new(inner)), ctor)
            }
            Pat::List(items, rest) => {
                let items = items.iter().map(|p| Ok(self.pattern(p)?.0)).collect::<Result<_, ResolveError>>()?;
                let rest = rest.as_ref().map(|r| {
                    Box::new(match r {
                        Some(name) => Pattern::Bind(name.clone()),
                        None => Pattern::Any,
                    })
                });
                (Pattern::List(items, rest), None)
            }
        })
    }

    /// `(done description: d)`: the record type's required attributes,
    /// each matching the pattern given for it or anything.
    fn constructor(&self, name: &str, args: &[(Option<String>, Pat)]) -> Result<Pattern, ResolveError> {
        let fields = self.claims.record(name).ok_or_else(|| {
            ResolveError::Invalid(format!("`({name} ..)` is not a pattern: `{name}` is not a declared record type"))
        })?;
        let mut given: Vec<Option<Pattern>> = fields.iter().map(|_| None).collect();
        for (i, (label, p)) in args.iter().enumerate() {
            let at = match label {
                None if i == 0 => 0,
                None => return Err(ResolveError::Invalid("only the first pattern goes without a label".into())),
                Some(l) => fields
                    .iter()
                    .position(|f| f.label == *l)
                    .ok_or_else(|| ResolveError::Invalid(format!("`{name}` has no attribute labeled `{l}:`")))?,
            };
            if given[at].is_some() {
                return Err(ResolveError::Invalid(format!(
                    "the pattern gives `{name}`'s `{}:` twice",
                    fields[at].label
                )));
            }
            given[at] = Some(self.pattern(p)?.0);
        }
        let mut out: Vec<(String, Pattern)> = Vec::new();
        for f in fields.iter() {
            self.open(&f.global)?;
        }
        for (f, p) in fields.iter().zip(given) {
            match p {
                Some(p) => out.push((f.global.clone(), p)),
                None if !f.optional => out.push((f.global.clone(), Pattern::Any)),
                None => {}
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Pattern::Fields(out))
    }

    /// A record type as a value: the function its constructor is, when
    /// every attribute is required.
    fn constructor_value(&self, name: &str) -> Result<Term, ResolveError> {
        let fields = self.claims.record(name).unwrap();
        if fields.iter().any(|f| f.optional) {
            return Err(ResolveError::Invalid(format!(
                "`{name}` has optional attributes, so its constructor is not a value yet: wrap it in `fn`"
            )));
        }
        let params: Vec<Param> =
            fields.iter().map(|f| Param { label: f.label.clone(), local: f.label.clone() }).collect();
        let order = canonical(&params);
        let n = order.len() as u32;
        let mut record: Vec<(String, Term)> = fields
            .iter()
            .map(|f| {
                let at = order.iter().position(|p| p.label == f.label).unwrap() as u32;
                (f.global.clone(), Term::Local(n - 1 - at))
            })
            .collect();
        record.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Term::Fn(order.into_iter().map(|p| p.label).collect(), Box::new(Term::Record(record))))
    }

    /// `(person "Ada" age: 36)`: a record of the attributes given, which
    /// must include the required ones.
    fn construct(&self, name: &str, args: Vec<(Option<String>, Term)>) -> Result<Term, ResolveError> {
        let fields = self.claims.record(name).unwrap();
        let mut given: Vec<Option<Term>> = fields.iter().map(|_| None).collect();
        for (i, (label, t)) in args.into_iter().enumerate() {
            let at = match &label {
                None if i == 0 => 0,
                None => {
                    return Err(ResolveError::Invalid(format!(
                        "only the first argument goes without a label: `{name}` takes {}",
                        fields[1..].iter().map(|f| format!("{}:", f.label)).collect::<Vec<_>>().join(" ")
                    )));
                }
                Some(l) => fields
                    .iter()
                    .position(|f| f.label == *l)
                    .ok_or_else(|| ResolveError::Invalid(format!("`{name}` has no attribute labeled `{l}:`")))?,
            };
            if given[at].is_some() {
                return Err(ResolveError::Invalid(format!("`{name}` is given `{}:` twice", fields[at].label)));
            }
            given[at] = Some(t);
        }
        let mut out = Vec::new();
        for (f, t) in fields.iter().zip(given) {
            match t {
                Some(t) => out.push((f.global.clone(), t)),
                None if f.optional => {}
                None => return Err(ResolveError::Invalid(format!("`{name}` needs `{}:`", f.label))),
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Term::Record(out))
    }

    /// The order a `match` over these record types tests them in: the
    /// rank of the variant that lists them all. Records of one attribute
    /// are variants already and need none.
    fn rank(&self, cases: &[&str]) -> Result<Option<(String, Vec<String>)>, ResolveError> {
        let mut distinct: Vec<&str> = cases.to_vec();
        distinct.sort();
        distinct.dedup();
        let single = |c: &&str| self.claims.record(c).is_some_and(|f| f.len() == 1);
        if distinct.len() < 2 || distinct.iter().all(single) {
            return Ok(None);
        }
        for name in self.claims.types.keys() {
            if let Some(order) = self.claims.variant(name)
                && distinct.iter().all(|c| order.iter().any(|o| o == c))
            {
                return Ok(Some((name.clone(), order)));
            }
        }
        Err(ResolveError::Invalid(format!(
            "a value may be more than one of {}, so matching them needs an order: declare a variant that ranks \
             them, `(type name : [{}])`",
            distinct.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", "),
            distinct.join(" ")
        )))
    }
}

/// Parameters in the order they are passed: the first, then the others
/// by label.
fn canonical(params: &[Param]) -> Vec<Param> {
    let mut rest = params[1..].to_vec();
    rest.sort_by(|a, b| a.label.cmp(&b.label));
    std::iter::once(params[0].clone()).chain(rest).collect()
}

/// Built-ins that take labels: their labels after the first argument,
/// and the order the built-in itself takes all of them in, by the
/// position of each in `[first, labels..]`.
fn builtin_labels(op: Op) -> Option<(&'static [&'static str], &'static [usize])> {
    match op {
        Op::Map => Some((&["each"], &[1, 0])),
        Op::Filter => Some((&["where"], &[1, 0])),
        Op::Item => Some((&["at", "or"], &[0, 1, 2])),
        Op::Natural | Op::Byte | Op::FromUtf8 => Some((&["or"], &[0, 1])),
        Op::ByteAt | Op::CodePointAt => Some((&["at", "or"], &[0, 1, 2])),
        Op::Slice => Some((&["from", "to"], &[0, 1, 2])),
        Op::Join => Some((&["with"], &[0, 1])),
        Op::Occurs => Some((&["in", "at"], &[0, 1, 2])),
        Op::IndexOf => Some((&["in", "from", "or"], &[0, 1, 2, 3])),
        _ => None,
    }
}

fn text(labels: &[String]) -> String {
    labels.iter().map(|l| format!("{l}:")).collect::<Vec<_>>().join(" ")
}

/// A call's arguments in the order `labels` gives, the first of which
/// may also come without its label.
fn arrange(callee: &str, labels: &[String], args: Vec<(Option<String>, Term)>) -> Result<Vec<Term>, ResolveError> {
    let mut slots: Vec<Option<Term>> = labels.iter().map(|_| None).collect();
    for (i, (label, value)) in args.into_iter().enumerate() {
        let at = match &label {
            None if i == 0 => 0,
            None => {
                return Err(ResolveError::Invalid(format!(
                    "only the first argument goes without a label: `{callee}` takes {}",
                    text(&labels[1..])
                )));
            }
            Some(l) => labels.iter().position(|x| x == l).ok_or_else(|| {
                ResolveError::Invalid(format!(
                    "`{callee}` has no parameter labeled `{l}:`{}",
                    if labels.len() > 1 { format!("; it takes {}", text(&labels[1..])) } else { String::new() }
                ))
            })?,
        };
        if slots[at].is_some() {
            return Err(ResolveError::Invalid(format!("`{callee}` is given `{}:` twice", labels[at])));
        }
        slots[at] = Some(value);
    }
    let missing: Vec<String> = labels.iter().zip(&slots).filter(|(_, s)| s.is_none()).map(|(l, _)| l.clone()).collect();
    if !missing.is_empty() {
        return Err(ResolveError::Invalid(format!("`{callee}` needs {}", text(&missing))));
    }
    Ok(slots.into_iter().map(Option::unwrap).collect())
}

/// Replace names with de Bruijn indices and hashes, labels with
/// parameter positions, and attributes' local names with global ones.
fn resolve(expr: &Expr, scope: &mut Vec<String>, names: &Names) -> Result<Term, ResolveError> {
    let b = |e: &Expr, scope: &mut Vec<String>| resolve(e, scope, names).map(Box::new);
    let lookup = names.lookup;
    Ok(match expr {
        Expr::Int(n) => Term::Int(*n),
        Expr::Str(s) => Term::Str(s.clone()),
        Expr::Bool(v) => Term::Bool(*v),
        Expr::Var(name, _) => {
            if let Some(pos) = scope.iter().rposition(|n| n == name) {
                Term::Local((scope.len() - 1 - pos) as u32)
            } else {
                // A marked name no local binds is the macro's: it is
                // resolved as it was where the macro was closed.
                let target = lookup(name);
                let name = &bare(name).to_string();
                match target {
                    Some(Target::Hash(h, _)) => Term::Ref(h),
                    Some(Target::Cycle(i, _)) => Term::CycleRef(i),
                    Some(Target::Method(m, _)) => Term::Dispatch(m),
                    None if Op::from_written(name).is_some() => {
                        return Err(ResolveError::Invalid(format!(
                            "the built-in `{name}` is not a value yet; call it, or wrap it in `fn`"
                        )));
                    }
                    None if names.claims.record(name).is_some() => names.constructor_value(name)?,
                    None if names.attribute(name).is_some() => {
                        return Err(ResolveError::Invalid(format!(
                            "the attribute `{name}` is read by calling it, `({name} record)`"
                        )));
                    }
                    None => return Err(ResolveError::Unbound(name.clone())),
                }
            }
        }
        Expr::Fn(params, body) => {
            let params = canonical(params);
            for p in &params {
                names.local(&p.local)?;
            }
            scope.extend(params.iter().map(|p| p.local.clone()));
            let body = resolve(body, scope, names);
            scope.truncate(scope.len() - params.len());
            Term::Fn(params.into_iter().map(|p| p.label).collect(), Box::new(body?))
        }
        Expr::App(callee, args, _) => app(callee, args, scope, names)?,
        Expr::Clauses(clauses) => clauses_term(clauses, scope, names)?,
        Expr::Let(name, value, body) => {
            names.local(name)?;
            let value = b(value, scope)?;
            scope.push(name.clone());
            let body = resolve(body, scope, names);
            scope.pop();
            Term::Let(value, Box::new(body?))
        }
        Expr::If(c, t, e) => Term::If(b(c, scope)?, b(t, scope)?, b(e, scope)?),
        Expr::Record(fields, _) => {
            let mut out: Vec<(String, Term)> = Vec::new();
            for (k, v) in fields {
                let global = names.key(k)?;
                if out.iter().any(|(g, _)| *g == global) {
                    return Err(ResolveError::Invalid(format!("the attribute `{global}` appears twice")));
                }
                out.push((global, resolve(v, scope, names)?));
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            Term::Record(out)
        }
        Expr::Update(record, fields, _) => {
            let mut out = resolve(record, scope, names)?;
            for (k, v) in fields {
                out = Term::Update(Box::new(out), names.key(k)?, b(v, scope)?);
            }
            out
        }
        Expr::Keyword(k) => Term::Keyword(k.clone()),
        Expr::Match(scrutinee, clauses, default, _) => {
            let mut patterns = Vec::new();
            for (p, r) in clauses {
                let (p, ctor) = names.pattern(p)?;
                if patterns.iter().any(|(q, _, _)| *q == p) {
                    return Err(ResolveError::Invalid("a `match` has the same pattern twice".into()));
                }
                patterns.push((p, ctor, r));
            }
            // Records that overlap are tested in their variant's rank, so
            // a value lands in the first case it fits wherever the arm is
            // written.
            let cases: Vec<&str> = patterns.iter().filter_map(|(_, c, _)| c.as_deref()).collect();
            if let Some((variant, order)) = names.rank(&cases)? {
                if cases.len() != patterns.len() {
                    return Err(ResolveError::Invalid(
                        "a `match` over a ranked variant's cases names only cases, and `_`".into(),
                    ));
                }
                // Without `_`, the match covers what it is given only if it
                // names every case, and every value of the variant fits
                // its last case, the widest.
                if default.is_none() {
                    if let Some(missing) = order.iter().find(|o| !cases.contains(&o.as_str())) {
                        return Err(ResolveError::Invalid(format!(
                            "this `match` does not name `{missing}`: name every case of `{variant}`, or end with `_`"
                        )));
                    }
                    let widest = names.claims.required(order.last().unwrap());
                    if let Some(other) = order.iter().find(|o| !widest.is_subset(&names.claims.required(o))) {
                        return Err(ResolveError::Invalid(format!(
                            "a value may fit no case of `{variant}`: `{}` is not in `{other}`, so no case is in all \
                             the others; end the `match` with `_`",
                            order.last().unwrap()
                        )));
                    }
                }
                let at = |c: &Option<String>| order.iter().position(|o| Some(o) == c.as_ref()).unwrap();
                patterns.sort_by_key(|(_, c, _)| at(c));
            }
            let mut arms: Vec<(Pattern, Term)> = Vec::new();
            for (p, _, r) in patterns {
                let bound = p.binders();
                for b in &bound {
                    names.local(b)?;
                }
                scope.extend(bound.iter().cloned());
                let r = resolve(r, scope, names);
                scope.truncate(scope.len() - bound.len());
                arms.push((p, r?));
            }
            // Keywords are a closed set the type checker can see the
            // whole of; strings and integers never are, so leaving them
            // out must be said.
            if default.is_none() && !Pattern::cover(&arms.iter().map(|(p, _)| p).collect::<Vec<_>>()) {
                return Err(ResolveError::Invalid(
                    "this `match` cannot cover every value without a `_` clause: only variants (keywords and \
                     one-attribute records) whose payloads are covered, or a record of `_`s, can"
                        .into(),
                ));
            }
            Term::Match(
                b(scrutinee, scope)?,
                arms,
                match default {
                    Some(d) => Some(b(d, scope)?),
                    None => None,
                },
            )
        }
        Expr::Dict(entries) => Term::Dict(
            entries
                .iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, scope, names)?)))
                .collect::<Result<_, ResolveError>>()?,
        ),
        Expr::List(items) => Term::List(items.iter().map(|i| resolve(i, scope, names)).collect::<Result<_, _>>()?),
        Expr::Task(body) => Term::Task(b(body, scope)?),
        Expr::Perform(task) => Term::Perform(b(task, scope)?),
        Expr::Handle(task, provided) => Term::Handle(
            b(task, scope)?,
            provided
                .iter()
                .map(|(cap, ops)| {
                    let ops = ops.iter().map(|(op, i)| Ok((op.clone(), resolve(i, scope, names)?))).collect::<Result<
                        Vec<_>,
                        ResolveError,
                    >>(
                    )?;
                    Ok((cap.clone(), ops))
                })
                .collect::<Result<_, ResolveError>>()?,
        ),
    })
}

/// A call: of a definition, whose labels are known; of an attribute, a
/// read; of a built-in; or of a function value, whose first argument is
/// passed without its label and whose others go in label order.
fn app(callee: &Expr, args: &[Arg], scope: &mut Vec<String>, names: &Names) -> Result<Term, ResolveError> {
    if let Expr::Var(name, _) = callee
        && bare(name) == "get"
        && !scope.contains(name)
        && (names.lookup)(name).is_none()
    {
        return get(args, scope, names);
    }
    let resolved: Vec<(Option<String>, Term)> = args
        .iter()
        .map(|a| Ok((a.label.clone(), resolve(&a.value, scope, names)?)))
        .collect::<Result<_, ResolveError>>()?;
    let global = match callee {
        Expr::Var(name, _) if !scope.contains(name) => Some(name.as_str()),
        _ => None,
    };
    if let Some(full) = global {
        let target = (names.lookup)(full);
        let name = bare(full);
        match target {
            Some(target) => {
                let (callee, labels) = match target {
                    Target::Hash(h, labels) => (Term::Ref(h), labels),
                    Target::Cycle(i, labels) => (Term::CycleRef(i), labels),
                    Target::Method(m, labels) => (Term::Dispatch(m), Some(labels)),
                };
                if let Some(labels) = labels {
                    let args = arrange(name, &labels, resolved)?;
                    return Ok(Term::App(Box::new(callee), args, labels[1.min(labels.len())..].to_vec()));
                }
                return value_call(callee, resolved);
            }
            None => {
                if names.claims.record(name).is_some() {
                    return names.construct(name, resolved);
                }
                if names.claims.types.contains_key(name) {
                    return Err(ResolveError::Invalid(format!(
                        "`{name}` is a type, and only a record type constructs"
                    )));
                }
                if let Some(attribute) = names.attribute(name) {
                    names.open(&attribute)?;
                    return read(name, attribute, resolved);
                }
                if let Some(op) = Op::from_written(name) {
                    return builtin(op, resolved);
                }
                return Err(ResolveError::Unbound(name.to_string()));
            }
        }
    }
    value_call(resolve(callee, scope, names)?, resolved)
}

/// `(get key of: container or: default)`: an attribute of a record,
/// which needs a default only where the record may lack it, or a
/// keyword of a dictionary, which always may.
fn get(args: &[Arg], scope: &mut Vec<String>, names: &Names) -> Result<Term, ResolveError> {
    const FORM: &str = "`get` reads `(get key of: container or: default)`";
    let invalid = |m: String| Err(ResolveError::Invalid(m));
    let Some((first, rest)) = args.split_first() else { return invalid(FORM.into()) };
    if first.label.is_some() {
        return invalid(format!("{FORM}: the key goes first, without a label"));
    }
    let mut of = None;
    let mut or = None;
    for a in rest {
        match a.label.as_deref() {
            Some("of") => of = Some(resolve(&a.value, scope, names)?),
            Some("or") => or = Some(resolve(&a.value, scope, names)?),
            Some(l) => return invalid(format!("{FORM}; it has no `{l}:`")),
            None => return invalid(format!("{FORM}: only the key goes without a label")),
        }
    }
    let Some(of) = of else { return invalid(format!("{FORM}: it needs `of:`, what to read")) };
    // An attribute names itself; it is never a value to compute.
    if let Expr::Var(name, _) = &first.value
        && !scope.contains(name)
        && (names.lookup)(name).is_none()
        && let Some(attribute) = names.attribute(bare(name))
    {
        let args = std::iter::once(of).chain(or).map(|t| (None, t)).collect();
        return read(bare(name), attribute, args);
    }
    let key = resolve(&first.value, scope, names)?;
    let Some(or) = or else {
        return invalid(
            "a dictionary may lack any key, so reading one needs `or:`: `(get :key of: dictionary or: default)`".into(),
        );
    };
    builtin(Op::Get, vec![(None, of), (None, key), (None, or)])
}

/// A call through a function value: the first argument without its
/// label, the others in label order.
fn value_call(callee: Term, mut args: Vec<(Option<String>, Term)>) -> Result<Term, ResolveError> {
    if args.iter().skip(1).any(|(l, _)| l.is_none()) {
        return Err(ResolveError::Invalid("only the first argument goes without a label".into()));
    }
    if let Some((Some(l), _)) = args.first() {
        return Err(ResolveError::Invalid(format!(
            "a function value takes its first argument without a label, not `{l}:`"
        )));
    }
    let mut rest = args.split_off(args.len().min(1));
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    let labels = rest.iter().map(|(l, _)| l.clone().unwrap()).collect();
    let terms = args.into_iter().chain(rest).map(|(_, t)| t).collect();
    Ok(Term::App(Box::new(callee), terms, labels))
}

/// `(name record)` reads an attribute; `(name record default)` reads it
/// where the record has it.
fn read(name: &str, attribute: String, args: Vec<(Option<String>, Term)>) -> Result<Term, ResolveError> {
    if args.iter().any(|(l, _)| l.is_some()) {
        return Err(ResolveError::Invalid(format!("`({name} record)` takes no labels")));
    }
    let mut args = args.into_iter().map(|(_, t)| t);
    match (args.next(), args.next(), args.next()) {
        (Some(record), None, None) => Ok(Term::Field(attribute, Box::new(record))),
        (Some(record), Some(default), None) => Ok(Term::Match(
            Box::new(record),
            vec![(Pattern::Fields(vec![(attribute, Pattern::Bind(name.to_string()))]), Term::Local(0))],
            Some(Box::new(shift_up(default))),
        )),
        _ => Err(ResolveError::Invalid(format!("`({name} record)` takes a record, and maybe a default"))),
    }
}

/// `t` under no new binder: a `match`'s default clause binds nothing, so
/// a resolved term moves into it unchanged.
fn shift_up(t: Term) -> Term {
    t
}

/// A built-in applied to arguments, after checking how many it takes.
/// `h` fills in its optional attributes, children, and events.
fn builtin(op: Op, mut args: Vec<(Option<String>, Term)>) -> Result<Term, ResolveError> {
    // `join` without `with:` joins with nothing between.
    if op == Op::Join && args.len() == 1 {
        args.push((Some("with".into()), Term::Str(String::new())));
    }
    let mut args: Vec<Term> = match builtin_labels(op) {
        Some((labels, order)) => {
            let all: Vec<String> =
                std::iter::once("of".to_string()).chain(labels.iter().map(|l| l.to_string())).collect();
            let mut given = arrange(op.name(), &all, args)?.into_iter().map(Some).collect::<Vec<_>>();
            order.iter().map(|&i| given[i].take().unwrap()).collect()
        }
        None => {
            if let Some((Some(l), _)) = args.iter().find(|(l, _)| l.is_some()) {
                return Err(ResolveError::Invalid(format!(
                    "the built-in `{}` takes no labels, given `{l}:`",
                    op.name()
                )));
            }
            args.into_iter().map(|(_, t)| t).collect()
        }
    };
    if op == Op::H {
        if args.is_empty() || args.len() > 4 {
            return Err(ResolveError::Invalid(
                "`h` takes a tag, then optionally attributes, children, and events".into(),
            ));
        }
        if args.len() < 2 {
            args.push(Term::Dict(vec![]));
        }
        if args.len() < 3 {
            args.push(Term::List(vec![]));
        }
        if args.len() < 4 {
            args.push(Term::Dict(vec![]));
        }
    }
    if let Some(arity) = op.arity()
        && args.len() != arity
    {
        return Err(ResolveError::Invalid(format!("`{}` takes {arity} argument(s), given {}", op.name(), args.len())));
    }
    Ok(Term::Prim(op, args))
}

/// The names an expression mentions that no binder inside it binds.
fn free_names(expr: &Expr, scope: &mut Vec<String>, out: &mut Vec<String>) {
    match expr {
        Expr::Var(name, _) => {
            // A pinned name refers to a definition already added.
            if !scope.contains(name) && pin_of(name).is_none() && !out.iter().any(|n| n == bare(name)) {
                out.push(bare(name).to_string());
            }
        }
        Expr::Fn(params, body) => {
            scope.extend(params.iter().map(|p| p.local.clone()));
            free_names(body, scope, out);
            scope.truncate(scope.len() - params.len());
        }
        Expr::App(c, args, _) => {
            free_names(c, scope, out);
            args.iter().for_each(|a| free_names(&a.value, scope, out));
        }
        Expr::Let(name, value, body) => {
            free_names(value, scope, out);
            scope.push(name.clone());
            free_names(body, scope, out);
            scope.pop();
        }
        Expr::If(a, b, c) => {
            free_names(a, scope, out);
            free_names(b, scope, out);
            free_names(c, scope, out);
        }
        Expr::Record(fields, _) | Expr::Dict(fields) => fields.iter().for_each(|(_, e)| free_names(e, scope, out)),
        Expr::Update(r, fields, _) => {
            free_names(r, scope, out);
            fields.iter().for_each(|(_, e)| free_names(e, scope, out));
        }
        Expr::List(items) => items.iter().for_each(|e| free_names(e, scope, out)),
        Expr::Match(scrutinee, clauses, default, _) => {
            free_names(scrutinee, scope, out);
            for (p, r) in clauses {
                let names = p.binders();
                scope.extend(names.iter().cloned());
                free_names(r, scope, out);
                scope.truncate(scope.len() - names.len());
            }
            if let Some(d) = default {
                free_names(d, scope, out);
            }
        }
        Expr::Clauses(clauses) => {
            for c in clauses {
                let mut bound = Vec::new();
                for (_, p) in &c.params {
                    match p {
                        Pat::Name(n) => bound.push(n.clone()),
                        p => bound.extend(p.binders()),
                    }
                }
                scope.extend(bound.iter().cloned());
                free_names(&c.body, scope, out);
                scope.truncate(scope.len() - bound.len());
            }
        }
        Expr::Task(e) | Expr::Perform(e) => free_names(e, scope, out),
        Expr::Handle(t, provided) => {
            free_names(t, scope, out);
            provided.iter().flat_map(|(_, ops)| ops).for_each(|(_, e)| free_names(e, scope, out));
        }
        Expr::Int(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Keyword(_) => {}
    }
}

fn renumber_cycle(term: &mut Term, canonical: &dyn Fn(u32) -> u32) {
    match term {
        Term::CycleRef(i) => *i = canonical(*i),
        Term::Fn(_, b) | Term::Field(_, b) | Term::Task(b) | Term::Perform(b) | Term::Site(_, b) => {
            renumber_cycle(b, canonical)
        }
        Term::Handle(t, caps) => {
            renumber_cycle(t, canonical);
            caps.iter_mut().flat_map(|(_, ops)| ops).for_each(|(_, i)| renumber_cycle(i, canonical));
        }
        Term::App(c, args, _) => {
            renumber_cycle(c, canonical);
            args.iter_mut().for_each(|a| renumber_cycle(a, canonical));
        }
        Term::Update(a, _, b) | Term::Let(a, b) | Term::And(a, b) | Term::Or(a, b) => {
            renumber_cycle(a, canonical);
            renumber_cycle(b, canonical);
        }
        Term::If(a, b, c) => {
            renumber_cycle(a, canonical);
            renumber_cycle(b, canonical);
            renumber_cycle(c, canonical);
        }
        Term::Record(fs) | Term::Dict(fs) => fs.iter_mut().for_each(|(_, t)| renumber_cycle(t, canonical)),
        Term::List(items) | Term::Prim(_, items) => items.iter_mut().for_each(|t| renumber_cycle(t, canonical)),
        Term::Match(scrutinee, clauses, default) => {
            renumber_cycle(scrutinee, canonical);
            clauses.iter_mut().for_each(|(_, r)| renumber_cycle(r, canonical));
            if let Some(d) = default {
                renumber_cycle(d, canonical);
            }
        }
        _ => {}
    }
}

/// Hash a component's members, `terms` in member order with cycle
/// references by member position, which come out by canonical index.
/// Each member's hash, in member order, and the members in canonical
/// order.
fn hash_members(
    terms: &mut [Term],
    recursive: bool,
    claim_text: &dyn Fn(usize) -> String,
    names: &[&str],
) -> (Vec<Hash>, Vec<usize>) {
    if !recursive {
        let mut bytes = Vec::new();
        terms[0].encode(&mut bytes, &|i| i);
        bytes.extend_from_slice(claim_text(0).as_bytes());
        return (vec![Hash::of(&bytes)], vec![0]);
    }
    // Order the members by their encoding with every cycle reference
    // written alike, so the order does not depend on names. Members that
    // encode identically are ordered by name, which is a known gap:
    // Unison would treat them as one definition.
    let mut keyed: Vec<(Vec<u8>, &str, usize)> = terms
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut bytes = Vec::new();
            t.encode(&mut bytes, &|_| 0);
            bytes.extend_from_slice(claim_text(i).as_bytes());
            (bytes, names[i], i)
        })
        .collect();
    keyed.sort();
    let cycle_order: Vec<usize> = keyed.iter().map(|k| k.2).collect();
    let canonical =
        |provisional: u32| -> u32 { cycle_order.iter().position(|&p| p == provisional as usize).unwrap() as u32 };
    let mut whole = Vec::new();
    for &p in &cycle_order {
        terms[p].encode(&mut whole, &canonical);
        whole.extend_from_slice(claim_text(p).as_bytes());
    }
    let cycle_hash = Hash::of(&whole);
    let mut hashes = vec![Hash([0; 32]); terms.len()];
    for (position, &p) in cycle_order.iter().enumerate() {
        let mut bytes = cycle_hash.0.to_vec();
        bytes.extend_from_slice(&(position as u32).to_le_bytes());
        hashes[p] = Hash::of(&bytes);
    }
    // Rewrite provisional cycle indices to canonical ones.
    for t in terms.iter_mut() {
        renumber_cycle(t, &canonical);
    }
    (hashes, cycle_order)
}

/// Tarjan's algorithm. Components come out with each one after every
/// component it depends on.
fn strongly_connected(deps: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct State<'a> {
        deps: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    fn visit(s: &mut State, v: usize) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on_stack[v] = true;
        for &w in &s.deps[v].clone() {
            match s.index[w] {
                None => {
                    visit(s, w);
                    s.low[v] = s.low[v].min(s.low[w]);
                }
                Some(iw) if s.on_stack[w] => s.low[v] = s.low[v].min(iw),
                _ => {}
            }
        }
        if Some(s.low[v]) == s.index[v] {
            let mut component = Vec::new();
            loop {
                let w = s.stack.pop().unwrap();
                s.on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            component.sort();
            s.out.push(component);
        }
    }
    let n = deps.len();
    let mut s = State {
        deps,
        index: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: vec![],
        next: 0,
        out: vec![],
    };
    for v in 0..n {
        if s.index[v].is_none() {
            visit(&mut s, v);
        }
    }
    s.out
}

/// The labels of a function defined by clauses, in its parameters'
/// order: the first parameter's, which the clauses that name it must
/// agree on, then the others', which every clause gives.
fn clause_labels(clauses: &[Clause]) -> Result<Vec<String>, ResolveError> {
    let invalid = |m: String| Err(ResolveError::Invalid(m));
    let mut first: Option<String> = None;
    for c in clauses {
        if let Some((None, Pat::Name(n))) = c.params.first() {
            let n = bare(n).to_string();
            match &first {
                Some(f) if *f != n => {
                    return invalid(format!(
                        "the clauses call the first parameter `{f}` and `{n}`: callers may pass it by its name, \
                         so the clauses agree on it"
                    ));
                }
                _ => first = Some(n),
            }
        }
    }
    let labels_of = |c: &Clause| {
        let mut l: Vec<String> = c.params.iter().skip(1).filter_map(|(l, _)| l.clone()).collect();
        l.sort();
        l
    };
    let rest = labels_of(&clauses[0]);
    for c in &clauses[1..] {
        let other = labels_of(c);
        if other != rest {
            return invalid(format!("the clauses take different parameters: `{}` and `{}`", text(&rest), text(&other)));
        }
    }
    Ok(std::iter::once(first.unwrap_or_else(|| "value".into())).chain(rest).collect())
}

/// A function defined by clauses: a `match` of its arguments, the
/// narrowest clause first. Clauses that overlap, neither narrower, are an
/// error, as rules of a macro are; so are clauses that leave a call
/// unmatched.
fn clauses_term(clauses: &[Clause], scope: &mut Vec<String>, names: &Names) -> Result<Term, ResolveError> {
    let invalid = |m: String| Err(ResolveError::Invalid(m));
    let labels = clause_labels(clauses)?;
    let n = labels.len();
    let key = |i: usize| format!("conduct.argument/{i:03}");
    let performs = clauses.iter().all(|c| matches!(c.body, Expr::Task(_)));
    if !performs && clauses.iter().any(|c| matches!(c.body, Expr::Task(_))) {
        return invalid(
            "some clauses are marked `!` and some are not: a function makes a task in every clause or in none".into(),
        );
    }
    let params: Vec<String> = (0..n).map(|i| format!(" argument {i}")).collect();
    let mut arms: Vec<(Pattern, Term, String)> = Vec::new();
    let mut ctors: Vec<Option<String>> = Vec::new();
    for c in clauses {
        // Parameters in the function's order: the first, then by label.
        let mut ordered: Vec<&(Option<String>, Pat)> = c.params.iter().collect();
        ordered[1..].sort_by(|a, b| a.0.cmp(&b.0));
        let mut patterns = Vec::new();
        for (_, p) in ordered {
            patterns.push(match p {
                // A parameter's name binds the argument, even one that
                // names a record type.
                Pat::Name(name) => Pattern::Bind(name.clone()),
                p => {
                    let (pattern, ctor) = names.pattern(p)?;
                    if n == 1 {
                        ctors.push(ctor);
                    }
                    pattern
                }
            });
        }
        if n == 1 && ctors.len() < arms.len() + 1 {
            ctors.push(None);
        }
        let pattern = if n == 1 {
            patterns.pop().unwrap()
        } else {
            Pattern::Fields(patterns.into_iter().enumerate().map(|(i, p)| (key(i), p)).collect())
        };
        let bound = pattern.binders();
        for b in &bound {
            names.local(b)?;
        }
        scope.extend(params.iter().cloned());
        scope.extend(bound.iter().cloned());
        let body = match &c.body {
            Expr::Task(inner) => resolve(inner, scope, names),
            other => resolve(other, scope, names),
        };
        scope.truncate(scope.len() - params.len() - bound.len());
        let head = c.params.iter().map(|(l, p)| match (l, p) {
            (Some(l), Pat::Name(n)) if bare(n) == l => l.clone(),
            (Some(l), p) => format!("{l}: {}", pat_text(p)),
            (None, p) => pat_text(p),
        });
        arms.push((pattern, body?, head.collect::<Vec<_>>().join(" ")));
    }
    // Record types a variant ranks are ordered by its rank, as a `match`
    // over them is.
    let rank: HashMap<String, usize> = {
        let mut cases: Vec<&str> = ctors.iter().flatten().map(String::as_str).collect();
        cases.sort();
        cases.dedup();
        let ranked = (cases.len() > 1)
            .then(|| {
                names.claims.types.keys().find_map(|name| {
                    let order = names.claims.variant(name)?;
                    cases.iter().all(|c| order.iter().any(|o| o == c)).then_some(order)
                })
            })
            .flatten();
        ranked.map(|order| order.into_iter().enumerate().map(|(i, c)| (c, i)).collect()).unwrap_or_default()
    };
    let mut arms: Vec<(Pattern, Term, String, Option<usize>)> = arms
        .into_iter()
        .zip(ctors.into_iter().chain(std::iter::repeat(None)))
        .map(|((p, t, h), c)| (p, t, h, c.and_then(|c| rank.get(&c).copied())))
        .collect();
    let sig = Sig::of_clauses(&arms.iter().map(|a| &a.0).collect::<Vec<_>>());
    let before = |a: &(Pattern, Term, String, Option<usize>), b: &(Pattern, Term, String, Option<usize>)| {
        a.0.narrower(&b.0) || matches!((a.3, b.3), (Some(x), Some(y)) if x < y)
    };
    // The order clauses are written in means nothing: start from one
    // that does not depend on it.
    let encoded = |p: &Pattern, t: &Term| {
        let mut bytes = Vec::new();
        Term::Match(Box::new(Term::Nil), vec![(p.clone(), t.clone())], None).encode(&mut bytes, &|i| i);
        bytes
    };
    arms.sort_by_cached_key(|(p, t, _, _)| encoded(p, t));
    for (i, x) in arms.iter().enumerate() {
        for y in &arms[i + 1..] {
            let (a, ah, b, bh) = (&x.0, &x.2, &y.0, &y.2);
            if !a.overlaps(b, &sig) {
                continue;
            }
            match (before(x, y), before(y, x)) {
                (true, true) => return invalid(format!("the clauses `({ah})` and `({bh})` match the same calls")),
                (false, false) => {
                    return invalid(format!(
                        "some call matches both `({ah})` and `({bh})`, and neither is narrower, so it could take \
                         either: make one narrower than the other, or make them disjoint"
                    ));
                }
                _ => {}
            }
        }
    }
    // Narrower clauses first: each is tried before any it is in.
    let mut ordered = Vec::new();
    while !arms.is_empty() {
        let at = (0..arms.len())
            .find(|&i| {
                !arms.iter().enumerate().any(|(j, b)| j != i && b.0.overlaps(&arms[i].0, &sig) && before(b, &arms[i]))
            })
            .expect("narrower is a partial order");
        let (p, t, _, _) = arms.remove(at);
        ordered.push((p, t));
    }
    if !Pattern::cover(&ordered.iter().map(|(p, _)| p).collect::<Vec<_>>()) {
        return invalid(
            "the clauses do not cover every call: add one for the calls they leave out, or one whose parameters \
             are all names"
                .into(),
        );
    }
    let scrutinee = if n == 1 {
        Term::Local(0)
    } else {
        Term::Record((0..n).map(|i| (key(i), Term::Local((n - 1 - i) as u32))).collect())
    };
    let body = Term::Match(Box::new(scrutinee), ordered, None);
    let body = if performs { Term::Task(Box::new(body)) } else { body };
    Ok(Term::Fn(labels, Box::new(body)))
}

/// A parameter's pattern, as a person wrote it.
fn pat_text(p: &Pat) -> String {
    match p {
        Pat::Keyword(k) => format!(":{k}"),
        Pat::Str(s) => format!("{s:?}"),
        Pat::Int(n) => n.to_string(),
        Pat::Bool(b) => b.to_string(),
        Pat::Any => "_".into(),
        Pat::Name(n) => bare(n).to_string(),
        Pat::Record(fields) => {
            format!("{{{}}}", fields.iter().map(|(k, p)| format!("{k}: {}", pat_text(p))).collect::<Vec<_>>().join(" "))
        }
        Pat::Ctor(name, args) => {
            let args = args.iter().map(|(l, p)| match l {
                Some(l) => format!(" {l}: {}", pat_text(p)),
                None => format!(" {}", pat_text(p)),
            });
            format!("({name}{})", args.collect::<String>())
        }
        Pat::Rest(name, p) => {
            let inner = pat_text(p);
            format!("{} {} ...}}", &inner[..inner.len() - 1], bare(name))
        }
        Pat::List(items, rest) => {
            let mut parts: Vec<String> = items.iter().map(pat_text).collect();
            if let Some(r) = rest {
                parts.push(format!("{} ...", r.as_deref().map(bare).unwrap_or("_")));
            }
            format!("[{}]", parts.join(" "))
        }
    }
}

/// The forms of a source, each with the domain that wraps it, if any:
/// `(domain p form ..)` stands for its forms, whose definitions and
/// claims are named in `p`, and in which `./x` is `p/x`.
fn flatten_domains(forms: &[Sexp]) -> Result<Vec<(Sexp, Option<String>)>, Error> {
    let mut out = Vec::new();
    for form in forms {
        match form {
            Sexp::List(items, span) if items.first().and_then(Sexp::as_symbol) == Some("domain") => {
                let Some(name) = items.get(1) else {
                    return Err(Error::Syntax(syntax::SyntaxError {
                        message: "a domain is `(domain io.gozala.parser (define ..) ..)`".into(),
                        span: *span,
                    }));
                };
                let prefix = syntax::domain_name(name).map_err(Error::Syntax)?;
                if items.len() == 2 {
                    return Err(Error::Syntax(syntax::SyntaxError {
                        message: format!("a domain wraps what it names: `(domain {prefix} (define ..) ..)`"),
                        span: *span,
                    }));
                }
                for inner in &items[2..] {
                    if matches!(inner, Sexp::List(xs, _) if xs.first().and_then(Sexp::as_symbol) == Some("domain")) {
                        return Err(Error::Syntax(syntax::SyntaxError {
                            message: "domains do not nest: write the whole name, `(domain a.b.c ..)`".into(),
                            span: inner.span(),
                        }));
                    }
                    out.push((relative(inner, &prefix), Some(prefix.clone())));
                }
            }
            other => out.push((other.clone(), None)),
        }
    }
    Ok(out)
}

/// `form` with each `./x` written as `prefix/x`.
fn relative(form: &Sexp, prefix: &str) -> Sexp {
    match form {
        Sexp::Symbol(s, span) => match s.strip_prefix("./") {
            Some(rest) => Sexp::Symbol(format!("{prefix}/{rest}"), *span),
            None => form.clone(),
        },
        Sexp::List(items, span) => Sexp::List(items.iter().map(|i| relative(i, prefix)).collect(), *span),
        Sexp::Vector(items, span) => Sexp::Vector(items.iter().map(|i| relative(i, prefix)).collect(), *span),
        Sexp::Map(items, span) => Sexp::Map(items.iter().map(|i| relative(i, prefix)).collect(), *span),
        other => other.clone(),
    }
}

/// Whether the whole name `name` ends in the segments `written` writes,
/// where `.` and `/` both separate segments: `conduct.parser/run` ends in
/// `run`, `parser/run` and itself, not in `arser/run`.
fn ends_with_segments(name: &str, written: &str) -> bool {
    let segments = |n: &str| n.split(['.', '/']).map(str::to_string).collect::<Vec<_>>();
    let (whole, part) = (segments(name), segments(written));
    part.len() <= whole.len() && whole[whole.len() - part.len()..] == part[..]
}

/// The built-in `written` names, if it names one: `byte`,
/// `conduct/byte`, or `syntax/inspect`.
fn builtin_named(written: &str) -> Option<Op> {
    Op::from_written(written)
}

/// The definition of the source being added that `written` names: the
/// one with that whole name, else the one whose name ends in the
/// segments written. Several such are ambiguous.
fn in_batch(written: &str, index: &HashMap<&str, usize>) -> Result<Option<String>, Vec<String>> {
    if index.contains_key(written) {
        return Ok(Some(written.to_string()));
    }
    let mut found: Vec<String> =
        index.keys().filter(|n| ends_with_segments(n, written)).map(|n| n.to_string()).collect();
    found.sort();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(found),
    }
}

/// A component resolved, hashed, and checked if it is new.
struct Attempt {
    terms: Vec<Term>,
    hashes: Vec<Hash>,
    cycle: Vec<Hash>,
    schemes: Option<Vec<Scheme>>,
}

/// Why a try at a component failed: a name that could be several
/// definitions, or anything else.
enum Failure {
    Ambiguous { def: String, name: String, candidates: Vec<String> },
    Error(Error),
}

impl Failure {
    fn into_error(self) -> Error {
        match self {
            Failure::Ambiguous { def, name, candidates } => Error::Resolve {
                def,
                message: format!(
                    "`{name}` could be any of {}: write which",
                    candidates.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
                ),
            },
            Failure::Error(e) => e,
        }
    }
}

/// One way of checking a component, given the names chosen so far.
type Trial<'a> = dyn Fn(&HashMap<String, String>) -> Result<Attempt, Failure> + 'a;

/// Unison's type-directed name resolution: where a name could be several
/// definitions, the one with which the component checks, if exactly one
/// does. Each candidate is tried with the names settled so far, and
/// settles any further ambiguity the same way. None or several fitting is
/// the ambiguity it was.
fn settle(attempt: &Trial, chosen: HashMap<String, String>) -> Result<Attempt, Failure> {
    match attempt(&chosen) {
        Err(Failure::Ambiguous { def, name, candidates }) => {
            let mut fits = Vec::new();
            for candidate in &candidates {
                let mut with = chosen.clone();
                with.insert(name.clone(), candidate.clone());
                if let Ok(found) = settle(attempt, with) {
                    fits.push(found);
                }
            }
            if fits.len() == 1 { Ok(fits.pop().unwrap()) } else { Err(Failure::Ambiguous { def, name, candidates }) }
        }
        other => other,
    }
}

/// A top-level form, with the domain it was written in, if any.
type Placed = (Sexp, Option<String>);

/// What a macro function's definition is named: `route#macro` for the
/// macro `route`.
const MACRO_SUFFIX: &str = "#macro";

/// Whether a claim, with no definition, claims a method: a function whose
/// first parameter is of a type variable, the type each definition of
/// it is for.
fn is_method_claim(c: &Claim) -> bool {
    !c.performs
        && c.with.is_none()
        && c.params.as_ref().is_some_and(|ps| matches!(ps.first(), Some((_, TypeExpr::Var(_)))))
}

/// `(macro name f)`: a macro that is a function, its name and `f`.
fn macro_function(form: &Sexp) -> Option<(&str, &Sexp)> {
    match form {
        Sexp::List(items, _) if items.first().and_then(Sexp::as_symbol) == Some("macro") => match &items[1..] {
            [Sexp::Symbol(name, _), f] => Some((name, f)),
            _ => None,
        },
        _ => None,
    }
}

/// The name a top-level form defines or claims a type for, if it is a
/// `define` or a `claim`.
fn defined_name<'a>(form: &'a Sexp, head: &str) -> Option<&'a str> {
    let Sexp::List(items, _) = form else { return None };
    if items.first().and_then(Sexp::as_symbol) != Some(head) {
        return None;
    }
    match items.get(1)? {
        Sexp::Symbol(n, _) => Some(n),
        Sexp::List(xs, _) => xs.first().and_then(Sexp::as_symbol),
        _ => None,
    }
}

/// Every symbol in `form`, as written.
fn symbols(form: &Sexp, out: &mut Vec<String>) {
    match form {
        Sexp::Symbol(s, _) => out.push(reader::bare(s).to_string()),
        Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
            items.iter().for_each(|i| symbols(i, out));
        }
        _ => {}
    }
}

/// A source split in two, for its macro functions: first its types, each
/// macro function as a definition with the claim it must meet, and the
/// definitions of the source those use, with their claims; then the rest.
fn macro_phases(forms: &[Placed]) -> (Vec<Placed>, Vec<Placed>) {
    let mut first = Vec::new();
    let mut wanted: Vec<String> = Vec::new();
    for (form, prefix) in forms {
        if let Some((name, f)) = macro_function(form) {
            let span = form.span();
            let defined = format!("{name}{MACRO_SUFFIX}");
            let text =
                format!("(claim {defined} : ((\\ form declared: (list declaration)) : (result form error: text)))");
            let claim = reader::read(&text).expect("the claim reads").remove(0);
            let def =
                Sexp::List(vec![Sexp::Symbol("define".into(), span), Sexp::Symbol(defined, span), f.clone()], span);
            symbols(f, &mut wanted);
            first.push((def, prefix.clone()));
            first.push((claim, prefix.clone()));
        }
    }
    // The definitions of the source the functions use, and what those
    // use, by name as written: `run` or `parser/run` names
    // `conduct.parser/run`.
    let mut taken = vec![false; forms.len()];
    loop {
        let mut more = false;
        for (i, (form, prefix)) in forms.iter().enumerate() {
            let Some(name) = defined_name(form, "define") else { continue };
            let whole = match prefix {
                Some(p) if !name.contains('/') => format!("{p}/{name}"),
                _ => name.to_string(),
            };
            if !taken[i] && wanted.iter().any(|w| w == name || ends_with_segments(&whole, w)) {
                taken[i] = true;
                more = true;
                symbols(form, &mut wanted);
            }
        }
        if !more {
            break;
        }
    }
    let used: Vec<&str> = forms
        .iter()
        .enumerate()
        .filter(|(i, _)| taken[*i])
        .filter_map(|(_, (f, _))| defined_name(f, "define"))
        .collect();
    let mut then = Vec::new();
    for (i, (form, prefix)) in forms.iter().enumerate() {
        let is_type = matches!(form, Sexp::List(items, _) if items.first().and_then(Sexp::as_symbol) == Some("type"));
        let claims_used = defined_name(form, "claim").is_some_and(|n| used.contains(&n));
        if macro_function(form).is_some() {
            continue;
        } else if taken[i] || is_type || claims_used {
            first.push((form.clone(), prefix.clone()));
        } else {
            then.push((form.clone(), prefix.clone()));
        }
    }
    (first, then)
}

/// The name a `type` form declares: `x` of `(type x : ..)`, `in-member` of
/// `(type (in-member ?x) : ..)`.
fn declared_name(form: &Sexp) -> Option<&str> {
    defined_name(form, "type")
}

/// What a `type` form declares its name as: the form after `:`.
fn declared_form(form: &Sexp) -> Option<Sexp> {
    let Sexp::List(items, _) = form else { return None };
    match &items[..] {
        [_, _, Sexp::Label(colon, _), declared] if colon.is_empty() => Some(declared.clone()),
        _ => None,
    }
}

fn quasi_error(message: &str) -> Error {
    Error::Resolve { def: "`".into(), message: message.into() }
}
