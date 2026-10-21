//! Methods: a claim with no definition, which each type defines for
//! itself, and which a call finds by the type of what it is called on.
//!
//! `(claim (compare ?a vs: ?a) : ordering)` in the domain `ord` claims
//! the method `ord/compare`. A variant defines it in the domain of its
//! attribute: the variant `{demo.thing/thing ..}` with
//! `demo.thing/compare`, as Roc's nominal types define their methods in
//! their own block. A call `(ord/compare x vs: y)` is a call of the
//! definition for the type of `x`, chosen when the code is checked.
//!
//! Where that type is not known, because it is what a caller gives, the
//! definition asks its callers for the method instead: its type carries
//! a requirement, and it takes the method as a parameter that each call
//! passes, found where the call knows the type (dictionary passing, as
//! Haskell compiles type classes). Nothing is copied for each type.

use crate::term::{Hash, Op, Term};
use crate::types::{Prim, Scheme, Ty};
use std::collections::{HashMap, HashSet};

/// A method's claim: the type every definition of it must fit, with the
/// type it is called on as its one requirement, and its parameters'
/// labels, the first's included.
#[derive(Clone)]
pub struct Method {
    pub scheme: Scheme,
    pub labels: Vec<String>,
    /// The claim as written, which a claim of it in another source must
    /// repeat.
    pub text: String,
}

/// What the type a method is called on is, for finding the method.
pub enum Receiver {
    /// Known: the domains to look for the method in, most specific
    /// first, and how to say what the type is.
    Known(Vec<String>, String),
    /// Not known here: what flows in comes from these variables, which a
    /// caller knows. Two calls on values from the same variables need
    /// the same method.
    Unknown(Vec<Ty>),
    /// A type no method is found for, said as "a record", say.
    Invalid(String),
}

/// What the values that flow into `t` are, by the types that flow in.
pub fn receiver(t: &Ty) -> Receiver {
    let mut seen = HashSet::new();
    let mut known = Vec::new();
    let mut sources: Vec<Ty> = Vec::new();
    gather(t, &mut seen, &mut known, &mut sources);
    let mut variants: Vec<String> = Vec::new();
    let mut prims: Vec<Prim> = Vec::new();
    let mut lists = false;
    for k in &known {
        match k {
            Ty::Tags(tags) => {
                for (key, payload) in tags.iter() {
                    if payload.is_none() {
                        return Receiver::Invalid(format!("the keyword `:{key}`, and a keyword has no methods"));
                    }
                    if !variants.contains(key) {
                        variants.push(key.clone());
                    }
                }
            }
            // A value a pattern found to be this variant.
            Ty::Update(_, key, _, true) => {
                if !variants.contains(key) {
                    variants.push(key.clone());
                }
            }
            Ty::Prim(Prim::Empty) => return Receiver::Invalid("`{}`, which has no methods".into()),
            Ty::Prim(Prim::Keyword) => return Receiver::Invalid("a keyword, which has no methods".into()),
            Ty::Prim(p) => {
                if !prims.contains(p) {
                    prims.push(*p);
                }
            }
            Ty::List(_) => lists = true,
            Ty::Bot => {}
            Ty::Record(_) | Ty::Update(..) => {
                return Receiver::Invalid(
                    "a record, and only a variant (a record of one attribute) or a built-in type has methods".into(),
                );
            }
            Ty::Fn(..) => return Receiver::Invalid("a function, which has no methods".into()),
            other => return Receiver::Invalid(format!("{}, which has no methods", crate::types::Shallow(other))),
        }
    }
    if variants.is_empty() && prims.is_empty() && !lists {
        if sources.is_empty() {
            sources.push(t.clone());
        }
        return Receiver::Unknown(sources);
    }
    if !sources.is_empty() {
        return Receiver::Invalid(
            "a value whose type is known in some places it comes from and not in others: say which type it is, or \
             call it where it is one"
                .into(),
        );
    }
    let kinds = usize::from(!variants.is_empty()) + usize::from(!prims.is_empty()) + usize::from(lists);
    if kinds > 1 || variants.len() > 1 {
        let mut shown: Vec<String> = variants.iter().map(|k| format!("`({} ..)`", local(k))).collect();
        shown.sort();
        shown.extend(prims.iter().map(|p| format!("`{}`", p.name())));
        if lists {
            shown.push("a list".into());
        }
        return Receiver::Invalid(format!(
            "one of {}, which each define it apart: `match` first, and call it on one",
            shown.join(", ")
        ));
    }
    if let [key] = variants.as_slice() {
        let (domain, _) = key.rsplit_once('/').unwrap_or(("", key));
        return Receiver::Known(vec![domain.to_string()], format!("`({} ..)`", local(key)));
    }
    if lists {
        return Receiver::Known(vec![builtin_domain("list")], "a list".into());
    }
    // Numbers are views of each other: a byte is a natural, a natural an
    // integer. The widest that flows in decides, and its method is
    // looked for there and then in what it is a view of.
    let numeric = [Prim::Byte, Prim::Nat, Prim::Int];
    if prims.iter().all(|p| numeric.contains(p)) {
        let widest = numeric.iter().rposition(|n| prims.contains(n)).unwrap();
        let domains = numeric[widest..].iter().map(|p| builtin_domain(p.name())).collect();
        return Receiver::Known(domains, format!("`{}`", numeric[widest].name()));
    }
    match prims.as_slice() {
        [p] => Receiver::Known(vec![builtin_domain(p.name())], format!("`{}`", p.name())),
        _ => Receiver::Invalid(format!(
            "one of {}, which each define it apart",
            prims.iter().map(|p| format!("`{}`", p.name())).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The domain a built-in type's methods are defined in: `conduct.natural`.
fn builtin_domain(name: &str) -> String {
    format!("{}.{name}", crate::codebase::BUILTIN_DOMAIN)
}

/// An attribute's name without its domain, as it is usually written.
fn local(global: &str) -> &str {
    global.rsplit('/').next().unwrap_or(global)
}

/// The types that flow into `t`, and the variables nothing flows into.
pub fn sources(t: &Ty) -> (Vec<Ty>, Vec<Ty>) {
    let (mut known, mut vars) = (Vec::new(), Vec::new());
    gather(t, &mut HashSet::new(), &mut known, &mut vars);
    (known, vars)
}

/// The types that flow into `t`, through the variables that flow into
/// it, and the variables that nothing flows into.
fn gather(t: &Ty, seen: &mut HashSet<u32>, known: &mut Vec<Ty>, sources: &mut Vec<Ty>) {
    match t {
        Ty::Var(v) => {
            if !seen.insert(v.id) {
                return;
            }
            let lowers = v.lower.borrow().clone();
            if lowers.is_empty() {
                sources.push(t.clone());
            }
            for l in &lowers {
                gather(l, seen, known, sources);
            }
        }
        Ty::Union(ts) => ts.iter().for_each(|t| gather(t, seen, known, sources)),
        other => known.push(other.clone()),
    }
}

/// Whether what flows into `t` must be of a built-in type, by what it
/// flows into: a variable read as an integer is a number of some kind,
/// whatever callers give.
pub fn built_in_above(t: &Ty) -> bool {
    fn above(t: &Ty, seen: &mut HashSet<u32>) -> bool {
        match t {
            Ty::Prim(_) => true,
            Ty::Inter(ts) => ts.iter().any(|t| above(t, seen)),
            Ty::Var(v) => seen.insert(v.id) && v.upper.borrow().iter().any(|u| above(u, seen)),
            _ => false,
        }
    }
    above(t, &mut HashSet::new())
}

/// The claim variables that flow into `t`, by name: what a requirement
/// is on once a definition is checked against its claim.
pub fn rigid_names(t: &Ty) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut known = Vec::new();
    gather(t, &mut seen, &mut known, &mut Vec::new());
    let mut names: Vec<String> =
        known.iter().filter_map(|k| if let Ty::Rigid(n) = k { Some(n.to_string()) } else { None }).collect();
    names.sort();
    names.dedup();
    names
}

/// The variables that what callers give flows into: a definition's
/// parameters and the parts of them it reads, by the bounds they are
/// read through. A method called on values of one of these is the
/// callers' to say.
pub fn inputs(members: &[Ty]) -> HashSet<u32> {
    fn reach(t: &Ty, out: &mut HashSet<u32>) {
        match t {
            Ty::Var(v) => {
                if out.insert(v.id) {
                    for u in v.upper.borrow().iter() {
                        reach(u, out);
                    }
                }
            }
            Ty::Fn(params, ret, _, _) => {
                params.iter().for_each(|p| reach(p, out));
                reach(ret, out);
            }
            Ty::Record(fields) => fields.values().for_each(|t| reach(t, out)),
            Ty::List(t) | Ty::Dict(t) | Ty::Opt(t) | Ty::IfHas(_, t) => reach(t, out),
            Ty::Tags(tags) => tags.values().flatten().for_each(|t| reach(t, out)),
            Ty::Update(r, _, v, _) | Ty::Task(r, v) => {
                reach(r, out);
                reach(v, out);
            }
            Ty::Union(ts) | Ty::Inter(ts) => ts.iter().for_each(|t| reach(t, out)),
            _ => {}
        }
    }
    /// The variables a claim's parameter types hold: what flows in from
    /// them is the callers' too.
    fn claimed(t: &Ty, out: &mut HashSet<u32>) {
        match t {
            Ty::Var(v) => {
                out.insert(v.id);
            }
            Ty::Record(fields) => fields.values().for_each(|t| claimed(t, out)),
            Ty::List(t) | Ty::Dict(t) | Ty::Opt(t) => claimed(t, out),
            Ty::Tags(tags) => tags.values().flatten().for_each(|t| claimed(t, out)),
            Ty::Union(ts) | Ty::Inter(ts) => ts.iter().for_each(|t| claimed(t, out)),
            _ => {}
        }
    }
    let mut out = HashSet::new();
    for m in members {
        if let Ty::Var(v) = m {
            for l in v.lower.borrow().iter() {
                if let Ty::Fn(params, ..) = l {
                    params.iter().for_each(|p| reach(p, &mut out));
                }
            }
            for u in v.upper.borrow().iter() {
                if let Ty::Fn(params, ..) = u {
                    params.iter().for_each(|p| claimed(p, &mut out));
                }
            }
        }
    }
    out
}

/// The variables a [`Receiver::Unknown`]'s values come from, by which
/// calls on the same values are known to need the same method.
pub fn key(sources: &[Ty]) -> Vec<u32> {
    let mut ids: Vec<u32> = sources.iter().filter_map(|t| if let Ty::Var(v) = t { Some(v.id) } else { None }).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// What a call of a method was found to call.
#[derive(Clone)]
pub enum Impl {
    /// A definition of it, with the methods that definition needs in
    /// turn, its own parameters' labels, and the labels it takes those
    /// methods by.
    Found { target: Hash, needs: Vec<Impl>, labels: Vec<String>, hidden: Vec<String> },
    /// A member of the cycle being checked, by its index there: it takes
    /// the methods the cycle is passed, as each member does.
    Member(u32),
    /// The method the definition is passed, by its position among them.
    Passed(usize),
    /// None: nothing reaches the call, as nothing is in an empty list,
    /// so any function of the method's parameters, which is never
    /// called, will do. Its parameters' labels.
    Nothing(Vec<String>),
    /// A method with a default, for a type that does not define it: a
    /// function the shape of the type makes, calling the method found
    /// for each of its parts. The method, its parameters' labels, the
    /// shape, and the parts.
    Derived { method: String, labels: Vec<String>, shape: Shape, parts: Vec<Impl> },
}

/// The methods every type has, which a type may define for itself and
/// which otherwise follow its shape: `show` and `same?`.
pub const DEFAULTED: &[&str] = &["conduct/show", "conduct/same?"];

/// How a method with a default goes through a type that does not define
/// it, with a part for each place it calls the method again.
#[derive(Clone, Debug)]
pub enum Shape {
    /// As the built-in does: `show` as `inspect` writes it, text as it
    /// is; `same?` as `=`.
    Plain,
    /// A record's attributes, by global name and local name, and whether
    /// each may be absent, in the order shown: a part for each.
    Record(Vec<(String, String, bool)>),
    /// A list: a part for its items.
    List,
    /// Variants, by global name and local name, each with a part for
    /// the whole value when its domain defines the method, or else for
    /// what it holds; any other value is [`Shape::Plain`].
    Variants(Vec<(String, String, bool)>),
}

/// A function's own parameters' labels, and the labels it takes the
/// methods it needs by.
pub type Labels = (Vec<String>, Vec<String>);

/// Rewrites a checked definition so that what its [`Term::Site`]s
/// recorded is passed: each method called is the definition found for
/// it, and a definition that is passed methods takes them as parameters
/// after its own.
pub struct Elaborator<'a> {
    /// What each site needs, in the order of its needs.
    pub found: &'a HashMap<u32, Vec<Impl>>,
    /// The labels the definitions being checked take their methods by.
    pub hidden: &'a [String],
    /// Each cycle member's own parameters' labels, by its index.
    pub cycle_labels: &'a [Vec<String>],
    /// A stored definition's own parameters' labels, and the labels it
    /// takes the methods it needs by.
    pub definition_labels: &'a dyn Fn(&Hash) -> Labels,
}

impl Elaborator<'_> {
    /// A member of the definitions being checked, which is a function if
    /// it is passed methods.
    pub fn member(&self, term: &Term) -> Term {
        match term {
            Term::Fn(labels, body) if !self.hidden.is_empty() => {
                let labels = labels.iter().chain(self.hidden).cloned().collect();
                Term::Fn(labels, Box::new(self.walk(body, 0)))
            }
            other => self.walk(other, 0),
        }
    }

    /// The method passed at `j`, under `depth` binders of the body.
    fn passed(&self, j: usize, depth: u32) -> Term {
        Term::Local(depth + (self.hidden.len() - 1 - j) as u32)
    }

    fn walk(&self, term: &Term, depth: u32) -> Term {
        let k = self.hidden.len() as u32;
        let w = |t: &Term| Box::new(self.walk(t, depth));
        let all = |ts: &[Term]| ts.iter().map(|t| self.walk(t, depth)).collect::<Vec<_>>();
        let entries =
            |fs: &[(String, Term)]| fs.iter().map(|(n, t)| (n.clone(), self.walk(t, depth))).collect::<Vec<_>>();
        match term {
            // A parameter of the definition: the methods it is passed come
            // after its own parameters, so the last of its own is no
            // longer local 0.
            Term::Local(i) if *i >= depth => Term::Local(i + k),
            Term::Fn(labels, body) => Term::Fn(labels.clone(), Box::new(self.walk(body, depth + labels.len() as u32))),
            Term::App(callee, args, labels) => {
                let args = all(args);
                match &**callee {
                    Term::Site(id, inner) => self.call(*id, inner, args, labels.clone(), depth),
                    other => Term::App(w(other), args, labels.clone()),
                }
            }
            Term::Site(id, inner) => self.value(*id, inner, depth),
            Term::Let(v, body) => Term::Let(w(v), Box::new(self.walk(body, depth + 1))),
            Term::If(a, b, c) => Term::If(w(a), w(b), w(c)),
            Term::And(a, b) => Term::And(w(a), w(b)),
            Term::Or(a, b) => Term::Or(w(a), w(b)),
            Term::Record(fs) => Term::Record(entries(fs)),
            Term::Dict(fs) => Term::Dict(entries(fs)),
            Term::Field(n, t) => Term::Field(n.clone(), w(t)),
            Term::Update(r, n, v) => Term::Update(w(r), n.clone(), w(v)),
            Term::List(items) => Term::List(all(items)),
            Term::Prim(op, args) => Term::Prim(*op, all(args)),
            Term::Host(name, args) => Term::Host(name.clone(), all(args)),
            Term::Task(t) => Term::Task(w(t)),
            Term::Perform(t) => Term::Perform(w(t)),
            Term::Handle(t, caps) => {
                Term::Handle(w(t), caps.iter().map(|(c, ops)| (c.clone(), entries(ops))).collect())
            }
            Term::Match(x, clauses, default) => Term::Match(
                w(x),
                clauses.iter().map(|(p, r)| (p.clone(), self.walk(r, depth + p.binders().len() as u32))).collect(),
                default.as_ref().map(|d| w(d)),
            ),
            Term::Dispatch(_) => unreachable!("every method is called at a site"),
            other => other.clone(),
        }
    }

    /// A call at a site, with its arguments already rewritten.
    fn call(&self, id: u32, inner: &Term, mut args: Vec<Term>, mut labels: Vec<String>, depth: u32) -> Term {
        match inner {
            Term::Dispatch(_) => self.call_impl(&self.found[&id][0], args, labels, depth),
            Term::Ref(h) => {
                let (_, hidden) = (self.definition_labels)(h);
                args.extend(self.found[&id].iter().map(|i| self.value_impl(i, depth)));
                labels.extend(hidden);
                Term::App(Box::new(Term::Ref(*h)), args, labels)
            }
            Term::CycleRef(i) => self.call_impl(&Impl::Member(*i), args, labels, depth),
            other => unreachable!("a site is a method, a definition, or a cycle member, not {other:?}"),
        }
    }

    fn call_impl(&self, imp: &Impl, mut args: Vec<Term>, mut labels: Vec<String>, depth: u32) -> Term {
        let callee = match imp {
            Impl::Passed(j) => self.passed(*j, depth),
            Impl::Nothing(_) | Impl::Derived { .. } => self.value_impl(imp, depth),
            Impl::Found { target, needs, hidden, .. } => {
                args.extend(needs.iter().map(|n| self.value_impl(n, depth)));
                labels.extend(hidden.iter().cloned());
                Term::Ref(*target)
            }
            Impl::Member(i) => {
                args.extend((0..self.hidden.len()).map(|j| self.passed(j, depth)));
                labels.extend(self.hidden.iter().cloned());
                Term::CycleRef(*i)
            }
        };
        Term::App(Box::new(callee), args, labels)
    }

    /// A site's value, where it is not called: a function that takes the
    /// method's own parameters and passes the methods it needs.
    fn value(&self, id: u32, inner: &Term, depth: u32) -> Term {
        match inner {
            Term::Dispatch(_) => self.value_impl(&self.found[&id][0], depth),
            Term::Ref(h) => {
                let (labels, hidden) = (self.definition_labels)(h);
                let needs = self.found.get(&id).cloned().unwrap_or_default();
                self.value_impl(&Impl::Found { target: *h, needs, labels, hidden }, depth)
            }
            Term::CycleRef(i) => self.value_impl(&Impl::Member(*i), depth),
            other => unreachable!("a site is a method, a definition, or a cycle member, not {other:?}"),
        }
    }

    fn value_impl(&self, imp: &Impl, depth: u32) -> Term {
        let (callee, labels, passing): (Term, &[String], Vec<&Impl>) = match imp {
            Impl::Passed(j) => return self.passed(*j, depth),
            Impl::Nothing(labels) => return Term::Fn(labels.clone(), Box::new(Term::Nil)),
            Impl::Derived { method, labels, shape, parts } => return self.derived(method, labels, shape, parts, depth),
            Impl::Found { target, needs, .. } if needs.is_empty() => return Term::Ref(*target),
            Impl::Member(i) if self.hidden.is_empty() => return Term::CycleRef(*i),
            Impl::Found { target, needs, labels, .. } => (Term::Ref(*target), labels, needs.iter().collect()),
            Impl::Member(i) => (Term::CycleRef(*i), &self.cycle_labels[*i as usize], Vec::new()),
        };
        let n = labels.len() as u32;
        let mut args: Vec<Term> = (0..n).map(|p| Term::Local(n - 1 - p)).collect();
        let mut call_labels: Vec<String> = labels[1.min(labels.len())..].to_vec();
        match imp {
            Impl::Found { hidden, .. } => {
                args.extend(passing.iter().map(|i| self.value_impl(i, depth + n)));
                call_labels.extend(hidden.iter().cloned());
            }
            _ => {
                args.extend((0..self.hidden.len()).map(|j| self.passed(j, depth + n)));
                call_labels.extend(self.hidden.iter().cloned());
            }
        }
        Term::Fn(labels.to_vec(), Box::new(Term::App(Box::new(callee), args, call_labels)))
    }

    /// A function of the method's parameters that goes through `shape`,
    /// calling the method found for each part.
    fn derived(&self, method: &str, labels: &[String], shape: &Shape, parts: &[Impl], depth: u32) -> Term {
        let n = labels.len() as u32;
        let parts: Vec<Term> = parts.iter().map(|p| self.value_impl(p, depth + n)).collect();
        let body = if method.ends_with("/show") { show_body(shape, &parts) } else { same_body(shape, &parts) };
        Term::Fn(labels.to_vec(), Box::new(body))
    }
}

fn join(pieces: Vec<Term>, with: &str) -> Term {
    Term::Prim(Op::Join, vec![Term::List(pieces), Term::Str(with.into())])
}

fn has(value: Term, global: &str) -> Term {
    Term::Prim(Op::HasField, vec![value, Term::Str(global.into())])
}

fn field(global: &str, value: Term) -> Term {
    Term::Field(global.into(), Box::new(value))
}

/// `show` through `shape`, of local 0.
fn show_body(shape: &Shape, parts: &[Term]) -> Term {
    let x = || Term::Local(0);
    let call = |part: &Term, arg: Term| Term::App(Box::new(part.clone()), vec![arg], vec![]);
    match shape {
        Shape::Plain => Term::Prim(Op::Show, vec![x()]),
        Shape::List => join(
            vec![
                Term::Str("[".into()),
                join_list(Term::Prim(Op::Map, vec![parts[0].clone(), x()]), " "),
                Term::Str("]".into()),
            ],
            "",
        ),
        Shape::Record(fields) => {
            let entries = fields.iter().zip(parts).map(|((global, local, optional), part)| {
                let entry = join(vec![Term::Str(format!("{local}: ")), call(part, field(global, x()))], "");
                if *optional {
                    Term::If(
                        Box::new(has(x(), global)),
                        Box::new(Term::List(vec![entry])),
                        Box::new(Term::List(vec![])),
                    )
                } else {
                    Term::List(vec![entry])
                }
            });
            let all = entries.reduce(|a, b| Term::Prim(Op::Concat, vec![a, b])).unwrap_or(Term::List(vec![]));
            join(vec![Term::Str("{".into()), join_list(all, ", "), Term::Str("}".into())], "")
        }
        Shape::Variants(variants) => variants.iter().zip(parts).rev().fold(
            Term::Prim(Op::Show, vec![x()]),
            |otherwise, ((global, local, whole), part)| {
                let shown = if *whole {
                    call(part, x())
                } else {
                    join(
                        vec![Term::Str(format!("({local} ")), call(part, field(global, x())), Term::Str(")".into())],
                        "",
                    )
                };
                Term::If(Box::new(has(x(), global)), Box::new(shown), Box::new(otherwise))
            },
        ),
    }
}

fn join_list(list: Term, with: &str) -> Term {
    Term::Prim(Op::Join, vec![list, Term::Str(with.into())])
}

/// `same?` through `shape`, of locals 1 and 0.
fn same_body(shape: &Shape, parts: &[Term]) -> Term {
    let (a, b) = (|| Term::Local(1), || Term::Local(0));
    let call = |part: &Term, x: Term, y: Term| Term::App(Box::new(part.clone()), vec![x, y], vec!["as".into()]);
    let and = |x: Term, y: Term| Term::And(Box::new(x), Box::new(y));
    match shape {
        Shape::Plain => Term::Prim(Op::Eq, vec![a(), b()]),
        Shape::List => Term::Prim(Op::Pairwise, vec![parts[0].clone(), a(), b()]),
        Shape::Record(fields) => fields
            .iter()
            .zip(parts)
            .map(|((global, _, optional), part)| {
                let both = call(part, field(global, a()), field(global, b()));
                if *optional {
                    Term::If(
                        Box::new(has(a(), global)),
                        Box::new(and(has(b(), global), both)),
                        Box::new(Term::Prim(Op::Not, vec![has(b(), global)])),
                    )
                } else {
                    both
                }
            })
            .reduce(and)
            .unwrap_or(Term::Bool(true)),
        Shape::Variants(variants) => variants.iter().zip(parts).rev().fold(
            Term::Prim(Op::Eq, vec![a(), b()]),
            |otherwise, ((global, _, whole), part)| {
                let both =
                    if *whole { call(part, a(), b()) } else { call(part, field(global, a()), field(global, b())) };
                Term::If(Box::new(has(a(), global)), Box::new(and(has(b(), global), both)), Box::new(otherwise))
            },
        ),
    }
}

/// Mark the places in `term` checking records something at: each method
/// called, each definition that asks for methods, and, in a cycle, each
/// member, which is passed the methods the cycle is.
pub fn mark_sites(term: Term, next: &mut u32, needs: &dyn Fn(&Hash) -> bool, cycle: bool) -> Term {
    let site = |t: Term, next: &mut u32| {
        let id = *next;
        *next += 1;
        Term::Site(id, Box::new(t))
    };
    let m = |t: Term, next: &mut u32| mark_sites(t, next, needs, cycle);
    let all = |ts: Vec<Term>, next: &mut u32| ts.into_iter().map(|t| mark_sites(t, next, needs, cycle)).collect();
    let entries = |fs: Vec<(String, Term)>, next: &mut u32| {
        fs.into_iter().map(|(n, t)| (n, mark_sites(t, next, needs, cycle))).collect()
    };
    match term {
        Term::Dispatch(_) => site(term, next),
        Term::Ref(h) if needs(&h) => site(term, next),
        Term::CycleRef(_) if cycle => site(term, next),
        Term::Fn(labels, body) => Term::Fn(labels, Box::new(m(*body, next))),
        Term::App(callee, args, labels) => {
            let callee = m(*callee, next);
            Term::App(Box::new(callee), all(args, next), labels)
        }
        Term::Let(v, b) => {
            let v = m(*v, next);
            Term::Let(Box::new(v), Box::new(m(*b, next)))
        }
        Term::If(a, b, c) => {
            let a = m(*a, next);
            let b = m(*b, next);
            Term::If(Box::new(a), Box::new(b), Box::new(m(*c, next)))
        }
        Term::And(a, b) => {
            let a = m(*a, next);
            Term::And(Box::new(a), Box::new(m(*b, next)))
        }
        Term::Or(a, b) => {
            let a = m(*a, next);
            Term::Or(Box::new(a), Box::new(m(*b, next)))
        }
        Term::Record(fs) => Term::Record(entries(fs, next)),
        Term::Dict(fs) => Term::Dict(entries(fs, next)),
        Term::Field(n, t) => Term::Field(n, Box::new(m(*t, next))),
        Term::Update(r, n, v) => {
            let r = m(*r, next);
            Term::Update(Box::new(r), n, Box::new(m(*v, next)))
        }
        Term::List(items) => Term::List(all(items, next)),
        Term::Prim(op, args) => Term::Prim(op, all(args, next)),
        Term::Host(name, args) => Term::Host(name, all(args, next)),
        Term::Task(t) => Term::Task(Box::new(m(*t, next))),
        Term::Perform(t) => Term::Perform(Box::new(m(*t, next))),
        Term::Handle(t, caps) => {
            let t = m(*t, next);
            Term::Handle(Box::new(t), caps.into_iter().map(|(c, ops)| (c, entries(ops, next))).collect())
        }
        Term::Match(x, clauses, default) => {
            let x = m(*x, next);
            let clauses = clauses.into_iter().map(|(p, r)| (p, m(r, next))).collect();
            Term::Match(Box::new(x), clauses, default.map(|d| Box::new(m(*d, next))))
        }
        other => other,
    }
}

/// Whether checking `term` must record sites: it calls a method, or a
/// definition that asks for methods.
pub fn has_sites(term: &Term, needs: &dyn Fn(&Hash) -> bool) -> bool {
    let mut found = false;
    term.walk(&mut |t| {
        found |= match t {
            Term::Dispatch(_) => true,
            Term::Ref(h) => needs(h),
            _ => false,
        }
    });
    found
}
