//! The codebase: definitions stored by hash, and names that point at
//! them.
//!
//! Adding source resolves its names, groups mutually recursive
//! definitions into cycles, hashes each definition (a cycle is hashed
//! as a unit, as Unison does), and type checks only what the codebase
//! does not already hold. A definition is checked once, ever: adding the
//! same code again, under any names, finds its hash already present.

use crate::infer::{Binding, Globals, Infer};
use crate::reader;
use crate::syntax::{self, Arg, Attribute, Def, Expr, Item, Param, Pattern, TypeExpr};
use crate::term::{Hash, Op, Term};
use crate::types::{Scheme, Solver, Ty, Type, TypeError, coalesce};
use std::collections::{BTreeMap, HashMap};
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

/// What every codebase starts with: the attributes the language's own
/// variants use.
pub const CORE: &str = include_str!("core.cdt");

#[derive(Default)]
pub struct Codebase {
    pub solver: Solver,
    pub defs: HashMap<Hash, Definition>,
    pub names: BTreeMap<String, Hash>,
    /// Claimed attributes: local name to global name.
    pub attributes: BTreeMap<String, String>,
    /// Each global name's claimed value type, as written and as a type.
    pub attribute_types: BTreeMap<String, (String, Ty)>,
}

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
    /// A codebase holding the core attributes.
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
            .rename_keys(&|k| self.local_attribute(k).map(str::to_string).unwrap_or_else(|| k.to_string()))
            .to_string()
    }

    /// The local name an attribute's global name is claimed under, if
    /// any.
    pub fn local_attribute(&self, global: &str) -> Option<&str> {
        self.attributes.iter().find(|(_, g)| *g == global).map(|(l, _)| l.as_str())
    }

    /// Declare a function the host provides, under `name`, which must end
    /// with `!`: a call takes `params`, answers `ret`, and uses the
    /// capability `capability`. Its hash covers the name it is imported
    /// by and its type.
    pub fn add_host(&mut self, name: &str, capability: &str, params: Vec<Ty>, ret: Ty) -> Result<Hash, Error> {
        let arity = params.len() as u32;
        let args = (0..arity).rev().map(Term::Local).collect();
        // A host function takes at most one argument, by position; the
        // labels are only there to make the term well formed.
        let labels: Vec<String> =
            (0..arity).map(|i| if i == 0 { "value".to_string() } else { format!("arg{i}") }).collect();
        let term = Term::Fn(labels.clone(), Box::new(Term::Host(name.to_string(), args)));
        let ty = Ty::func_eff(params, ret, Ty::caps([capability.to_string()]), labels[1.min(labels.len())..].to_vec());
        let mut bytes = Vec::new();
        term.encode(&mut bytes, &|i| i);
        bytes.extend_from_slice(coalesce(&ty).to_string().as_bytes());
        let hash = Hash::of(&bytes);
        let scheme = Scheme { level: 0, body: ty };
        check_name(name, &scheme)?;
        self.defs.insert(hash, Definition { hash, term, cycle: vec![], scheme });
        self.names.insert(name.to_string(), hash);
        Ok(hash)
    }

    pub fn lookup(&self, name: &str) -> Option<&Definition> {
        self.names.get(name).and_then(|h| self.defs.get(h))
    }

    /// Parse, resolve, hash, and type check `source`, then point its
    /// names at the resulting hashes. Nothing changes on an error.
    pub fn add_source(&mut self, source: &str) -> Result<Vec<Added>, Error> {
        let forms = reader::read(source).map_err(Error::Read)?;
        let items = syntax::items(&forms).map_err(Error::Syntax)?;
        let mut defs = Vec::new();
        let mut attributes = Vec::new();
        for item in items {
            match item {
                Item::Def(d) => defs.push(d),
                Item::Attribute(a) => attributes.push(a),
            }
        }
        // Claims come first: they declare what definitions use, and
        // nothing about them depends on order.
        let saved = (self.attributes.clone(), self.attribute_types.clone());
        let out = self.claim_attributes(&attributes, &defs).and_then(|()| self.add_defs(defs));
        if out.is_err() {
            (self.attributes, self.attribute_types) = saved;
        }
        out
    }

    /// Record attribute claims, after checking each agrees with what is
    /// already claimed.
    fn claim_attributes(&mut self, claims: &[Attribute], defs: &[Def]) -> Result<(), Error> {
        for a in claims {
            let error = |message: String| Err(Error::Resolve { def: a.name.clone(), message });
            let Some((domain, last)) = a.global.rsplit_once('/') else { unreachable!() };
            if domain.is_empty() || last.is_empty() {
                return error(format!("`{}` is not a global name: write `domain/name`", a.global));
            }
            if self.names.contains_key(&a.name) || defs.iter().any(|d| d.name == a.name) {
                return error(format!("`{}` names a definition, so it cannot also name an attribute", a.name));
            }
            if let Some(global) = self.attributes.get(&a.name)
                && *global != a.global
            {
                return error(format!("`{}` is already claimed as the attribute `{global}`", a.name));
            }
            let written = a.ty.to_string();
            let ty = attribute_type(&a.ty).map_err(|message| Error::Resolve { def: a.name.clone(), message })?;
            if let Some((before, _)) = self.attribute_types.get(&a.global)
                && *before != written
            {
                return error(format!("`{}` is already claimed with values `{before}`, not `{written}`", a.global));
            }
            self.attributes.insert(a.name.clone(), a.global.clone());
            self.attribute_types.insert(a.global.clone(), (written, ty));
        }
        Ok(())
    }

    pub fn add_defs(&mut self, defs: Vec<Def>) -> Result<Vec<Added>, Error> {
        for d in &defs {
            if self.attributes.contains_key(&d.name) {
                return Err(Error::Resolve {
                    def: d.name.clone(),
                    message: format!("`{}` is claimed as an attribute, so it cannot also name a definition", d.name),
                });
            }
        }
        // A later definition of a name in the same source replaces an
        // earlier one.
        let mut batch: Vec<Def> = Vec::new();
        for d in defs {
            batch.retain(|b| b.name != d.name);
            batch.push(d);
        }
        let index: HashMap<&str, usize> = batch.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();

        // Which definitions of the batch each one mentions.
        let deps: Vec<Vec<usize>> = batch
            .iter()
            .map(|d| {
                let mut names = Vec::new();
                free_names(&d.body, &mut Vec::new(), &mut names);
                let mut out: Vec<usize> = names.iter().filter_map(|n| index.get(n.as_str()).copied()).collect();
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
            for d in &members {
                let lookup = |name: &str| -> Option<Target> {
                    if let Some(i) = names_in_cycle.iter().position(|n| *n == name)
                        && recursive
                    {
                        let labels = match &members[i].body {
                            Expr::Fn(params, _) => Some(canonical(params).into_iter().map(|p| p.label).collect()),
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
                let names = Names { lookup: &lookup, attributes: &self.attributes, globals: &self.attribute_types };
                let term = resolve(&d.body, &mut Vec::new(), &names).map_err(|e| match e {
                    ResolveError::Unbound(name) => Error::Unbound { name, def: d.name.clone() },
                    ResolveError::Invalid(message) => Error::Resolve { def: d.name.clone(), message },
                })?;
                terms.push(term);
            }

            let hashes: Vec<Hash>;
            let cycle_order: Vec<usize>;
            if !recursive {
                let mut bytes = Vec::new();
                terms[0].encode(&mut bytes, &|i| i);
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
                let schemes = self.check(&members, &terms, order, &staged)?;
                for ((term, scheme), hash) in terms.into_iter().zip(schemes).zip(hashes.iter()) {
                    staged.push(Definition { hash: *hash, term, cycle: cycle.clone(), scheme });
                }
            }
            for (d, h) in members.iter().zip(hashes.iter()) {
                let def = staged.iter().find(|s| s.hash == *h).or_else(|| self.defs.get(h));
                if let Some(def) = def {
                    check_name(&d.name, &def.scheme)?;
                }
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
    ) -> Result<Vec<Scheme>, Error> {
        struct Layered<'a> {
            defs: &'a HashMap<Hash, Definition>,
            staged: &'a [Definition],
            attributes: &'a BTreeMap<String, (String, Ty)>,
        }
        impl Globals for Layered<'_> {
            fn scheme(&self, hash: &Hash) -> Option<Scheme> {
                self.defs.get(hash).or_else(|| self.staged.iter().find(|d| d.hash == *hash)).map(|d| d.scheme.clone())
            }
            fn attribute(&self, global: &str) -> Option<Ty> {
                self.attributes.get(global).map(|(_, t)| t.clone())
            }
        }
        let globals = Layered { defs: &self.defs, staged, attributes: &self.attribute_types };
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
            effects: std::cell::RefCell::new(vec![Ty::pure()]),
        };
        let mut schemes = Vec::new();
        for (i, (d, term)) in members.iter().zip(terms).enumerate() {
            let t = infer
                .term(term, &mut Vec::<Binding>::new(), 1)
                .map_err(|error| Error::Type { def: d.name.clone(), error })?;
            s.constrain(&t, &member_vars[i]).map_err(|error| Error::Type { def: d.name.clone(), error })?;
            schemes.push(Scheme { level: 0, body: member_vars[i].clone() });
        }
        Ok(schemes)
    }
}

/// Roc's rule: a name ends with `!` exactly when what it names uses
/// capabilities. A function whose effect depends on its arguments' (like
/// `map`'s) may be named either way.
fn check_name(name: &str, scheme: &Scheme) -> Result<(), Error> {
    let marked = name.ends_with('!');
    let effect = match coalesce(&scheme.body) {
        Type::Fn(_, _, effect, _) => *effect,
        _ => Type::Caps(Default::default()),
    };
    let mut caps = std::collections::BTreeSet::new();
    let mut polymorphic = false;
    let mut visit = |t: &Type| match t {
        Type::Caps(c) => caps.extend(c.iter().cloned()),
        Type::Var(_) => polymorphic = true,
        _ => {}
    };
    match &effect {
        Type::Union(ts) => ts.iter().for_each(&mut visit),
        t => visit(t),
    }
    let error = |message: String| Err(Error::Resolve { def: name.to_string(), message });
    if !caps.is_empty() && !marked {
        let used = caps.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
        return error(format!("it uses {used}, so its name must end with `!`"));
    }
    if caps.is_empty() && !polymorphic && marked {
        return error("it uses no capabilities, so its name must not end with `!`".into());
    }
    Ok(())
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
    attributes: &'a BTreeMap<String, String>,
    globals: &'a BTreeMap<String, (String, Ty)>,
}

impl Names<'_> {
    /// The global name of an attribute, written by its local name or its
    /// global one.
    fn attribute(&self, name: &str) -> Option<String> {
        if let Some(global) = self.attributes.get(name) {
            return Some(global.clone());
        }
        self.globals.contains_key(name).then(|| name.to_string())
    }

    fn key(&self, name: &str) -> Result<String, ResolveError> {
        self.attribute(name).ok_or_else(|| {
            ResolveError::Invalid(if name.contains('/') {
                format!("no claim declares the attribute `{name}`")
            } else {
                format!("`{name}:` is not a claimed attribute: claim it, `(claim {name} : (domain/{name} type))`")
            })
        })
    }

    /// A pattern with its record keys resolved to global names.
    fn pattern(&self, p: &Pattern) -> Result<Pattern, ResolveError> {
        Ok(match p {
            Pattern::Fields(fields) => {
                let mut out: Vec<(String, Pattern)> = Vec::new();
                for (k, p) in fields {
                    let global = self.key(k)?;
                    if out.iter().any(|(g, _)| *g == global) {
                        return Err(ResolveError::Invalid(format!("the attribute `{global}` appears twice")));
                    }
                    out.push((global, self.pattern(p)?));
                }
                out.sort_by(|a, b| a.0.cmp(&b.0));
                Pattern::Fields(out)
            }
            other => other.clone(),
        })
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
                match lookup(name) {
                    Some(Target::Hash(h, _)) => Term::Ref(h),
                    Some(Target::Cycle(i, _)) => Term::CycleRef(i),
                    None if Op::from_name(name).is_some() => {
                        return Err(ResolveError::Invalid(format!(
                            "the built-in `{name}` is not a value yet; call it, or wrap it in `fn`"
                        )));
                    }
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
            scope.extend(params.iter().map(|p| p.local.clone()));
            let body = resolve(body, scope, names);
            scope.truncate(scope.len() - params.len());
            Term::Fn(params.into_iter().map(|p| p.label).collect(), Box::new(body?))
        }
        Expr::App(callee, args, _) => app(callee, args, scope, names)?,
        Expr::Let(name, value, body) => {
            let value = b(value, scope)?;
            scope.push(name.clone());
            let body = resolve(body, scope, names);
            scope.pop();
            Term::Let(value, Box::new(body?))
        }
        Expr::If(c, t, e) => Term::If(b(c, scope)?, b(t, scope)?, b(e, scope)?),
        Expr::And(x, y) => Term::And(b(x, scope)?, b(y, scope)?),
        Expr::Or(x, y) => Term::Or(b(x, scope)?, b(y, scope)?),
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
            let mut arms: Vec<(Pattern, Term)> = Vec::new();
            for (p, r) in clauses {
                let p = names.pattern(p)?;
                if arms.iter().any(|(q, _)| *q == p) {
                    return Err(ResolveError::Invalid("a `match` has the same pattern twice".into()));
                }
                let bound = p.binders();
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
    })
}

/// A call: of a definition, whose labels are known; of an attribute, a
/// read; of a built-in; or of a function value, whose first argument is
/// passed without its label and whose others go in label order.
fn app(callee: &Expr, args: &[Arg], scope: &mut Vec<String>, names: &Names) -> Result<Term, ResolveError> {
    let resolved: Vec<(Option<String>, Term)> = args
        .iter()
        .map(|a| Ok((a.label.clone(), resolve(&a.value, scope, names)?)))
        .collect::<Result<_, ResolveError>>()?;
    let global = match callee {
        Expr::Var(name, _) if !scope.contains(name) => Some(name.as_str()),
        _ => None,
    };
    if let Some(name) = global {
        match (names.lookup)(name) {
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
                if let Some(attribute) = names.attribute(name) {
                    return read(name, attribute, resolved);
                }
                if let Some(op) = Op::from_name(name) {
                    return builtin(op, resolved);
                }
                return Err(ResolveError::Unbound(name.to_string()));
            }
        }
    }
    value_call(resolve(callee, scope, names)?, resolved)
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

/// The value type an attribute claims.
fn attribute_type(t: &TypeExpr) -> Result<Ty, String> {
    Ok(match t {
        TypeExpr::Name(name, _) => match name.as_str() {
            "text" => Ty::str(),
            "integer" => Ty::int(),
            "boolean" => Ty::bool(),
            "json" => Ty::Prim(crate::types::Prim::Json),
            other => return Err(format!("`{other}` is not a type an attribute takes yet")),
        },
        // A variable an attribute does not tie to anything else asks
        // nothing of its values.
        TypeExpr::Var(_) => Ty::Top,
        TypeExpr::App(head, args, _) => match (head.as_str(), args.as_slice()) {
            ("list", [t]) => Ty::list(attribute_type(t)?),
            ("dict", [t]) => Ty::dict(attribute_type(t)?),
            _ => return Err(format!("`({head} ..)` is not a type an attribute takes yet")),
        },
        TypeExpr::Keywords(ks) => Ty::keys(ks.iter().cloned()),
        TypeExpr::Empty => Ty::record([]),
    })
}

/// The names an expression mentions that no binder inside it binds.
fn free_names(expr: &Expr, scope: &mut Vec<String>, out: &mut Vec<String>) {
    match expr {
        Expr::Var(name, _) => {
            if !scope.contains(name) && !out.contains(name) {
                out.push(name.clone());
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
        Expr::And(a, b) | Expr::Or(a, b) => {
            free_names(a, scope, out);
            free_names(b, scope, out);
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
        Expr::Int(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Keyword(_) => {}
    }
}

fn renumber_cycle(term: &mut Term, canonical: &dyn Fn(u32) -> u32) {
    match term {
        Term::CycleRef(i) => *i = canonical(*i),
        Term::Fn(_, b) | Term::Field(_, b) => renumber_cycle(b, canonical),
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
