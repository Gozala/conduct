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
use crate::syntax::{self, Def, Expr};
use crate::term::{Hash, Op, Term};
use crate::types::{Scheme, Solver, Ty, TypeError, coalesce};
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
    /// The definition's type as shown to a person.
    pub fn type_string(&self) -> String {
        coalesce(&self.scheme.body).to_string()
    }
}

#[derive(Default)]
pub struct Codebase {
    pub solver: Solver,
    pub defs: HashMap<Hash, Definition>,
    pub names: BTreeMap<String, Hash>,
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

impl Globals for HashMap<Hash, Definition> {
    fn scheme(&self, hash: &Hash) -> Option<Scheme> {
        self.get(hash).map(|d| d.scheme.clone())
    }
}

impl Codebase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lookup(&self, name: &str) -> Option<&Definition> {
        self.names.get(name).and_then(|h| self.defs.get(h))
    }

    /// Parse, resolve, hash, and type check `source`, then point its
    /// names at the resulting hashes. Nothing changes on an error.
    pub fn add_source(&mut self, source: &str) -> Result<Vec<Added>, Error> {
        let forms = reader::read(source).map_err(Error::Read)?;
        let defs = syntax::defs(&forms).map_err(Error::Syntax)?;
        self.add_defs(defs)
    }

    pub fn add_defs(&mut self, defs: Vec<Def>) -> Result<Vec<Added>, Error> {
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
                        return Some(Target::Cycle(i as u32));
                    }
                    if let Some(h) = resolved.get(name) {
                        return Some(Target::Hash(*h));
                    }
                    self.names.get(name).map(|h| Target::Hash(*h))
                };
                let term = resolve(&d.body, &mut Vec::new(), &lookup).map_err(|e| match e {
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
        }
        impl Globals for Layered<'_> {
            fn scheme(&self, hash: &Hash) -> Option<Scheme> {
                self.defs.get(hash).or_else(|| self.staged.iter().find(|d| d.hash == *hash)).map(|d| d.scheme.clone())
            }
        }
        let globals = Layered { defs: &self.defs, staged };
        let s = &self.solver;
        // Cycle members are known to each other monomorphically while
        // their bodies are checked, by their canonical index.
        let member_vars: Vec<Ty> = (0..members.len()).map(|_| s.fresh_ty(1)).collect();
        // `terms` are in provisional order, and the cycle references in
        // them are canonical: canonical position i is provisional member
        // `cycle_order[i]`.
        let cycle_types: Vec<Ty> = cycle_order.iter().map(|&p| member_vars[p].clone()).collect();
        let infer = Infer { solver: s, globals: &globals, cycle: cycle_types };
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

enum ResolveError {
    Unbound(String),
    Invalid(String),
}

enum Target {
    Hash(Hash),
    Cycle(u32),
}

/// Replace names with de Bruijn indices and hashes. Returns the first
/// name that resolves to nothing.
fn resolve(
    expr: &Expr,
    scope: &mut Vec<String>,
    lookup: &dyn Fn(&str) -> Option<Target>,
) -> Result<Term, ResolveError> {
    let b = |e: &Expr, scope: &mut Vec<String>| resolve(e, scope, lookup).map(Box::new);
    Ok(match expr {
        Expr::Int(n) => Term::Int(*n),
        Expr::Str(s) => Term::Str(s.clone()),
        Expr::Bool(v) => Term::Bool(*v),
        Expr::Nil => Term::Nil,
        Expr::Var(name, _) => {
            if let Some(pos) = scope.iter().rposition(|n| n == name) {
                Term::Local((scope.len() - 1 - pos) as u32)
            } else {
                match lookup(name) {
                    Some(Target::Hash(h)) => Term::Ref(h),
                    Some(Target::Cycle(i)) => Term::CycleRef(i),
                    None if Op::from_name(name).is_some() => {
                        return Err(ResolveError::Invalid(format!(
                            "the built-in `{name}` is not a value yet; call it, or wrap it in `fn`"
                        )));
                    }
                    None => return Err(ResolveError::Unbound(name.clone())),
                }
            }
        }
        Expr::Fn(params, body) => {
            scope.extend(params.iter().cloned());
            let body = resolve(body, scope, lookup);
            scope.truncate(scope.len() - params.len());
            Term::Fn(params.len() as u32, Box::new(body?))
        }
        Expr::App(callee, args, _) => {
            let args: Vec<Term> = args.iter().map(|a| resolve(a, scope, lookup)).collect::<Result<_, _>>()?;
            // A name no local or definition binds may name a built-in:
            // locals and definitions shadow built-ins.
            if let Expr::Var(name, _) = &**callee
                && !scope.contains(name)
                && lookup(name).is_none()
                && let Some(op) = Op::from_name(name)
            {
                return builtin(op, args);
            }
            Term::App(b(callee, scope)?, args)
        }
        Expr::Let(name, value, body) => {
            let value = b(value, scope)?;
            scope.push(name.clone());
            let body = resolve(body, scope, lookup);
            scope.pop();
            Term::Let(value, Box::new(body?))
        }
        Expr::If(c, t, e) => Term::If(b(c, scope)?, b(t, scope)?, b(e, scope)?),
        Expr::IfSome(name, value, then, otherwise) => {
            let value = b(value, scope)?;
            scope.push(name.clone());
            let then = resolve(then, scope, lookup);
            scope.pop();
            Term::IfSome(value, Box::new(then?), b(otherwise, scope)?)
        }
        Expr::Coalesce(x, y) => Term::Coalesce(b(x, scope)?, b(y, scope)?),
        Expr::And(x, y) => Term::And(b(x, scope)?, b(y, scope)?),
        Expr::Or(x, y) => Term::Or(b(x, scope)?, b(y, scope)?),
        Expr::Record(fields) => {
            let mut out: Vec<(String, Term)> = fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, scope, lookup)?)))
                .collect::<Result<_, ResolveError>>()?;
            out.sort_by(|a, b| a.0.cmp(&b.0));
            Term::Record(out)
        }
        Expr::Field(name, record) => Term::Field(name.clone(), b(record, scope)?),
        Expr::Update(record, name, value) => Term::Update(b(record, scope)?, name.clone(), b(value, scope)?),
        Expr::Keyword(k) => Term::Keyword(k.clone()),
        Expr::Match(scrutinee, clauses, default) => Term::Match(
            b(scrutinee, scope)?,
            clauses
                .iter()
                .map(|(p, r)| Ok((resolve(p, scope, lookup)?, resolve(r, scope, lookup)?)))
                .collect::<Result<_, ResolveError>>()?,
            match default {
                Some(d) => Some(b(d, scope)?),
                None => None,
            },
        ),
        Expr::Dict(entries) => Term::Dict(
            entries
                .iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, scope, lookup)?)))
                .collect::<Result<_, ResolveError>>()?,
        ),
        Expr::List(items) => Term::List(items.iter().map(|i| resolve(i, scope, lookup)).collect::<Result<_, _>>()?),
    })
}

/// A built-in applied to arguments, after checking how many it takes.
/// `h` fills in its optional attributes, children, and events.
fn builtin(op: Op, mut args: Vec<Term>) -> Result<Term, ResolveError> {
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
            if !scope.contains(name) && !out.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::Fn(params, body) => {
            scope.extend(params.iter().cloned());
            free_names(body, scope, out);
            scope.truncate(scope.len() - params.len());
        }
        Expr::App(c, args, _) => {
            free_names(c, scope, out);
            args.iter().for_each(|a| free_names(a, scope, out));
        }
        Expr::Let(name, value, body) => {
            free_names(value, scope, out);
            scope.push(name.clone());
            free_names(body, scope, out);
            scope.pop();
        }
        Expr::IfSome(name, value, then, otherwise) => {
            free_names(value, scope, out);
            scope.push(name.clone());
            free_names(then, scope, out);
            scope.pop();
            free_names(otherwise, scope, out);
        }
        Expr::If(a, b, c) => {
            free_names(a, scope, out);
            free_names(b, scope, out);
            free_names(c, scope, out);
        }
        Expr::Coalesce(a, b) | Expr::And(a, b) | Expr::Or(a, b) => {
            free_names(a, scope, out);
            free_names(b, scope, out);
        }
        Expr::Record(fields) | Expr::Dict(fields) => fields.iter().for_each(|(_, e)| free_names(e, scope, out)),
        Expr::Field(_, e) => free_names(e, scope, out),
        Expr::Update(r, _, v) => {
            free_names(r, scope, out);
            free_names(v, scope, out);
        }
        Expr::List(items) => items.iter().for_each(|e| free_names(e, scope, out)),
        Expr::Match(scrutinee, clauses, default) => {
            free_names(scrutinee, scope, out);
            clauses.iter().for_each(|(_, r)| free_names(r, scope, out));
            if let Some(d) = default {
                free_names(d, scope, out);
            }
        }
        Expr::Int(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Nil | Expr::Keyword(_) => {}
    }
}

fn renumber_cycle(term: &mut Term, canonical: &dyn Fn(u32) -> u32) {
    match term {
        Term::CycleRef(i) => *i = canonical(*i),
        Term::Fn(_, b) | Term::Field(_, b) => renumber_cycle(b, canonical),
        Term::App(c, args) => {
            renumber_cycle(c, canonical);
            args.iter_mut().for_each(|a| renumber_cycle(a, canonical));
        }
        Term::Update(a, _, b) | Term::Let(a, b) | Term::Coalesce(a, b) | Term::And(a, b) | Term::Or(a, b) => {
            renumber_cycle(a, canonical);
            renumber_cycle(b, canonical);
        }
        Term::If(a, b, c) | Term::IfSome(a, b, c) => {
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
