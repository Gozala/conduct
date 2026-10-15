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
use crate::reader::{self, Sexp, bare, pin_of};
use crate::syntax::{self, Arg, Claim, Clause, Def, Expr, Item, Param, Pat, TypeExpr};
use crate::term::{Hash, Op, Pattern, Sig, Term};
use crate::types::{Scheme, Solver, Ty, TypeError, coalesce};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

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
    /// The last mark an expansion gave its template's symbols.
    marks: u64,
    /// The domain of the source being added, if it names one.
    domain: Option<String>,
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
        self.lookup(name).map(|d| self.show(&d.scheme.body))
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
        let scheme = Scheme { level: 0, body: ty };
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
        let saved = (self.claims.clone(), self.names.clone(), self.macros.clone());
        let out = self.scoped(&forms).and_then(|rest| self.add_forms(rest));
        self.domain = None;
        if out.is_err() {
            (self.claims, self.names, self.macros) = saved;
        }
        out
    }

    /// Read the `domain` form a source may start with, which names the
    /// domain its definitions go in, and answer the source's other forms.
    fn scoped<'f>(&mut self, forms: &'f [Sexp]) -> Result<&'f [Sexp], Error> {
        let heads = |form: &Sexp| match form {
            Sexp::List(items, _) => items.first().and_then(Sexp::as_symbol) == Some("domain"),
            _ => false,
        };
        let leading = forms.iter().take_while(|f| heads(f)).count();
        let misplaced = forms[leading..].iter().find(|f| heads(f)).or(if leading > 1 { forms.get(1) } else { None });
        if let Some(form) = misplaced {
            return Err(Error::Syntax(syntax::SyntaxError {
                message: "a source names one domain, first: `(domain io.gozala.parser)`".into(),
                span: form.span(),
            }));
        }
        if let Some(form) = forms[..leading].first() {
            let Item::Domain(name) = syntax::item(form).map_err(Error::Syntax)? else {
                unreachable!("only `domain` leads")
            };
            self.domain = Some(name);
        }
        Ok(&forms[leading..])
    }

    /// The whole name of a definition of the source being added: `./x`
    /// and a name with no domain go in the source's domain.
    fn full_name(&self, name: &str) -> String {
        let local = name.strip_prefix("./").unwrap_or(name);
        match &self.domain {
            Some(domain) if !local.contains('/') => format!("{domain}/{local}"),
            _ => local.to_string(),
        }
    }

    /// The whole names `written` may stand for among the definitions of
    /// the source being added: in its domain, or as written.
    fn spellings(&self, written: &str) -> Vec<String> {
        if let Some(local) = written.strip_prefix("./") {
            return self.domain.iter().map(|d| format!("{d}/{local}")).collect();
        }
        if written.contains('/') {
            return vec![written.to_string()];
        }
        let mut out: Vec<String> = self.domain.iter().map(|d| format!("{d}/{written}")).collect();
        out.push(written.to_string());
        out
    }

    /// The whole name in the codebase that `written` refers to. `./x` is
    /// `x` in the source's domain, and `io.gozala.parser/x` is itself. A
    /// name with no domain is the definition of that name with no
    /// domain, if there is one; else the one definition of that name in
    /// any domain. When the name is in several domains, or in one and
    /// also built in, it is ambiguous, and the error lists each whole
    /// name to write instead.
    fn resolve_name(&self, written: &str, defined: &dyn Fn(&str) -> bool) -> Result<Option<String>, Vec<String>> {
        if let Some(local) = written.strip_prefix("./") {
            return Ok(self.domain.as_ref().map(|d| format!("{d}/{local}")).filter(|n| defined(n)));
        }
        if written.contains('/') || defined(written) {
            return Ok(Some(written.to_string()).filter(|n| defined(n)));
        }
        let suffix = format!("/{written}");
        let mut candidates: Vec<String> = self.names.keys().filter(|n| n.ends_with(&suffix)).cloned().collect();
        let builtin = Op::from_name(written).is_some();
        match (candidates.len(), builtin) {
            (0, _) => Ok(None),
            (1, false) => Ok(candidates.pop()),
            _ => {
                if builtin {
                    candidates.push(format!("{BUILTIN_DOMAIN}/{written}"));
                }
                Err(candidates)
            }
        }
    }

    fn add_forms(&mut self, forms: &[Sexp]) -> Result<Vec<Added>, Error> {
        let mut rules: BTreeMap<String, Vec<Rule>> = BTreeMap::new();
        let mut rest = Vec::new();
        for form in forms {
            if let Sexp::List(items, _) = form
                && items.first().and_then(Sexp::as_symbol) == Some("macro")
            {
                let Item::Macro(m) = syntax::item(form).map_err(Error::Syntax)? else { unreachable!() };
                let rule = Rule::new(&m.name, &m.pattern, &m.template)
                    .map_err(|message| Error::Resolve { def: format!("({} ..)", m.name), message })?;
                rules.entry(m.name).or_default().push(rule);
            } else {
                rest.push(form);
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
        let mut items = Vec::new();
        for form in rest {
            let form = self.expand_top(form, 0)?;
            items.push(syntax::item(&form).map_err(Error::Syntax)?);
        }
        let added = self.add_items(items)?;
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
                Some(name) if self.macros.contains_key(name) => {
                    let out = self.expand_call(name, &items[1..], *span, depth)?;
                    self.expand(&out, depth + 1)?
                }
                _ => Sexp::List(all(self, items)?, *span),
            },
            Sexp::Vector(items, span) => Sexp::Vector(all(self, items)?, *span),
            Sexp::Map(items, span) => Sexp::Map(all(self, items)?, *span),
            other => other.clone(),
        })
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
                Item::Domain(_) => unreachable!("a source's domain is read first"),
            }
        }
        for d in &mut defs {
            d.name = self.full_name(&d.name);
        }
        for c in &mut claims {
            c.name = self.full_name(&c.name);
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
        for c in claims {
            let error = |message: String| Err(Error::Resolve { def: c.name.clone(), message });
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
        let saved = self.claims.clone();
        let out = self.claim(&types, &defs).and_then(|()| self.add_checked(defs, &signatures));
        if out.is_err() {
            self.claims = saved;
        }
        out
    }

    /// Record type claims, after checking each agrees with what is
    /// already declared and every type they name is declared.
    fn claim(&mut self, claims: &[Claim], defs: &[Def]) -> Result<(), Error> {
        for c in claims {
            let error = |message: String| Err(Error::Resolve { def: c.name.clone(), message });
            if self.names.contains_key(&c.name) || defs.iter().any(|d| d.name == c.name) {
                return error(format!("`{}` names a definition, so it cannot also name a type", c.name));
            }
            match (&c.params, &c.ty) {
                (None, TypeExpr::Attribute(global, value, _)) => self.claim_attribute(&c.name, global, value)?,
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
                                self.claim_global(&c.name, global, value)?;
                            }
                        }
                    }
                    self.claims.types.insert(c.name.clone(), TypeDef { params: vars, body: body.clone(), text });
                }
            }
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

    fn claim_attribute(&mut self, name: &str, global: &str, value: &TypeExpr) -> Result<(), Error> {
        let error = |message: String| Err(Error::Resolve { def: name.to_string(), message });
        if self.claims.types.contains_key(name) {
            return error(format!("`{name}` is declared as a type"));
        }
        if let Some(before) = self.claims.attributes.get(name)
            && before != global
        {
            return error(format!("`{name}` is already declared as the attribute `{before}`"));
        }
        self.claim_global(name, global, value)?;
        self.claims.attributes.insert(name.to_string(), global.to_string());
        Ok(())
    }

    /// The type of a global name's values, which every claim of it must
    /// agree on.
    fn claim_global(&mut self, name: &str, global: &str, value: &TypeExpr) -> Result<(), Error> {
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
        // A variable an attribute does not tie to anything else asks
        // nothing of its values.
        let mut env = Env::new(Mode::Flexible(&self.solver, 1));
        let ty = match value {
            TypeExpr::Var(_) => Ty::Top,
            _ => {
                let mut free = BTreeSet::new();
                claims::vars(value, &mut free);
                if free.is_empty() {
                    self.claims.ty(value, &mut env).map_err(|message| Error::Resolve { def: name.into(), message })?
                } else {
                    Ty::Top
                }
            }
        };
        self.claims.attribute_types.insert(global.to_string(), (written, ty));
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
                    .filter_map(|n| self.spellings(n).iter().find_map(|s| index.get(s.as_str()).copied()))
                    .collect();
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
                    // A definition of this source, in its namespace or as
                    // written, comes before what the codebase holds.
                    let in_batch = self.spellings(written).into_iter().find(|s| index.contains_key(s.as_str()));
                    let name = match in_batch {
                        Some(full) => full,
                        None => match self.resolve_name(written, &|n| self.names.contains_key(n)) {
                            Ok(Some(full)) => full,
                            Ok(None) => return None,
                            Err(candidates) => {
                                ambiguous.replace(Some((written.to_string(), candidates)));
                                return None;
                            }
                        },
                    };
                    let name = name.as_str();
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
                    let term = staged
                        .iter()
                        .find(|s| s.hash == *h)
                        .map(|s| &s.term)
                        .or_else(|| self.defs.get(h).map(|d| &d.term));
                    let labels = match term {
                        Some(Term::Fn(labels, _)) => Some(labels.clone()),
                        _ => None,
                    };
                    Some(Target::Hash(*h, labels))
                };
                let names = Names { lookup: &lookup, claims: &self.claims, macros: &self.macros };
                let resolved_term = resolve(&d.body, &mut Vec::new(), &names);
                // A name that could be several things is an error even
                // where resolution went on to read it as a built-in.
                let resolved_term = match (resolved_term, ambiguous.borrow().clone()) {
                    (Ok(_), Some((name, _))) => Err(ResolveError::Unbound(name)),
                    (other, _) => other,
                };
                let term = resolved_term.map_err(|e| match e {
                    ResolveError::Unbound(name) if ambiguous.borrow().as_ref().is_some_and(|(n, _)| *n == name) => {
                        let (_, candidates) = ambiguous.take().unwrap();
                        Error::Resolve {
                            def: d.name.clone(),
                            message: format!(
                                "`{name}` could be any of {}: write which",
                                candidates.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
                            ),
                        }
                    }
                    ResolveError::Unbound(name) => Error::Unbound { name, def: d.name.clone() },
                    ResolveError::Invalid(message) => Error::Resolve { def: d.name.clone(), message },
                })?;
                terms.push(term);
            }

            let hashes: Vec<Hash>;
            let cycle_order: Vec<usize>;
            // A signature is part of what a definition is: callers see
            // its type.
            let claim_text = |i: usize| signatures.get(&members[i].name).map(|c| c.text()).unwrap_or_default();
            if !recursive {
                let mut bytes = Vec::new();
                terms[0].encode(&mut bytes, &|i| i);
                bytes.extend_from_slice(claim_text(0).as_bytes());
                hashes = vec![Hash::of(&bytes)];
                cycle_order = vec![0];
            } else {
                // Order the members by their encoding with every cycle
                // reference written alike, so the order does not depend
                // on names. Members that encode identically are ordered
                // by name, which is a known gap: Unison would treat them
                // as one definition.
                let mut keyed: Vec<(Vec<u8>, &str, usize)> = terms
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let mut bytes = Vec::new();
                        t.encode(&mut bytes, &|_| 0);
                        bytes.extend_from_slice(claim_text(i).as_bytes());
                        (bytes, members[i].name.as_str(), i)
                    })
                    .collect();
                keyed.sort();
                cycle_order = keyed.iter().map(|k| k.2).collect();
                let canonical = |provisional: u32| -> u32 {
                    cycle_order.iter().position(|&p| p == provisional as usize).unwrap() as u32
                };
                let mut whole = Vec::new();
                for &p in &cycle_order {
                    terms[p].encode(&mut whole, &canonical);
                    whole.extend_from_slice(claim_text(p).as_bytes());
                }
                let cycle_hash = Hash::of(&whole);
                let mut by_provisional = vec![Hash([0; 32]); members.len()];
                for (position, &p) in cycle_order.iter().enumerate() {
                    let mut bytes = cycle_hash.0.to_vec();
                    bytes.extend_from_slice(&(position as u32).to_le_bytes());
                    by_provisional[p] = Hash::of(&bytes);
                }
                hashes = by_provisional;
                // Rewrite provisional cycle indices to canonical ones.
                for t in terms.iter_mut() {
                    renumber_cycle(t, &canonical);
                }
            }
            let cycle: Vec<Hash> = if recursive { cycle_order.iter().map(|&p| hashes[p]).collect() } else { vec![] };

            let fresh = !self.defs.contains_key(&hashes[0]) && !staged.iter().any(|d| d.hash == hashes[0]);
            if fresh {
                let order: &[usize] = if recursive { &cycle_order } else { &[] };
                let schemes = self.check(&members, &terms, order, &staged, signatures)?;
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

    /// Type check one component and return each member's scheme.
    fn check(
        &self,
        members: &[&Def],
        terms: &[Term],
        cycle_order: &[usize],
        staged: &[Definition],
        signatures: &HashMap<String, Claim>,
    ) -> Result<Vec<Scheme>, Error> {
        struct Layered<'a> {
            defs: &'a HashMap<Hash, Definition>,
            staged: &'a [Definition],
            attributes: &'a BTreeMap<String, (String, Ty)>,
            hosts: &'a BTreeMap<String, BTreeMap<String, Hash>>,
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
        }
        let globals =
            Layered { defs: &self.defs, staged, attributes: &self.claims.attribute_types, hosts: &self.hosts };
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
        let infer =
            Infer { solver: s, globals: &globals, cycle: cycle_types, effects: std::cell::RefCell::new(vec![None]) };
        let mut schemes = Vec::new();
        for (i, (d, term)) in members.iter().zip(terms).enumerate() {
            let t = infer
                .term(term, &mut Vec::<Binding>::new(), 1)
                .map_err(|error| Error::Type { def: d.name.clone(), error })?;
            s.constrain(&t, &member_vars[i]).map_err(|error| Error::Type { def: d.name.clone(), error })?;
            schemes.push(Scheme { level: 0, body: member_vars[i].clone() });
        }
        // A signature is checked against what was inferred, and is what
        // callers then see.
        for (i, (d, term)) in members.iter().zip(terms).enumerate() {
            if let Some(claim) = signatures.get(&d.name) {
                let fail = |message: String| Error::Type { def: d.name.clone(), error: TypeError(message) };
                let claimed = self.signature(claim, term, &schemes[i]).map_err(fail)?;
                schemes[i] = Scheme { level: 0, body: claimed };
            }
        }
        Ok(schemes)
    }

    /// The type a signature claims for `term`, once checked against the
    /// scheme inferred for it.
    fn signature(&self, claim: &Claim, term: &Term, inferred: &Scheme) -> Result<Ty, String> {
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
        // The claim's type, and the variable for capabilities it leaves
        // to inference, if it does.
        let read = |env: &mut Env| -> Result<(Ty, Option<Ty>), String> {
            let params = match &claim.params {
                Some(params) => Some(self.claims.params(params, env)?),
                None => None,
            };
            let ret = self.claims.ty(&claim.ty, env)?;
            // A claim may write the whole type, as it is shown: a task, or
            // a function that answers one.
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
        };
        // Checked with its variables as types the caller chooses...
        let mut rigid_env = Env::new(Mode::Rigid(s));
        let (rigid, effect) = read(&mut rigid_env)?;
        s.constrain(&s.instantiate(inferred, 1), &rigid)
            .map_err(|e| format!("the definition does not fit its claim: {e}"))?;
        // ...and shown to callers with them free. Capabilities it left
        // out are what the body was found to use: named ones, and those
        // of the functions it was given, which are its own variables.
        let mut open_env = Env::new(Mode::Flexible(s, 1));
        let (open, open_effect) = read(&mut open_env)?;
        if let (Some(found), Some(shown)) = (effect, open_effect) {
            for bound in lower_bounds(&found) {
                let bound = match bound {
                    Ty::Rigid(name) => open_env.vars[&*name].clone(),
                    other => other,
                };
                s.constrain(&bound, &shown).map_err(|e| e.to_string())?;
            }
        }
        Ok(open)
    }
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
}

/// Everything resolution looks names up in.
struct Names<'a> {
    lookup: &'a dyn Fn(&str) -> Option<Target>,
    claims: &'a Claims,
    macros: &'a BTreeMap<String, macros::Macro>,
}

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
        Op::Natural => Some((&["or"], &[0, 1])),
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
                    None if Op::from_name(name).is_some() => {
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
                    return read(name, attribute, resolved);
                }
                if let Some(op) = Op::from_name(name.strip_prefix("conduct/").unwrap_or(name)) {
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
fn builtin(op: Op, args: Vec<(Option<String>, Term)>) -> Result<Term, ResolveError> {
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
        Term::Fn(_, b) | Term::Field(_, b) | Term::Task(b) | Term::Perform(b) => renumber_cycle(b, canonical),
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
