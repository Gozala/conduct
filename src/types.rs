//! Types and subtype constraint solving, after Parreaux's Simple-sub
//! ("The Simple Essence of Algebraic Subtyping", ICFP 2020).
//!
//! Inference keeps, for every type variable, the types known to flow
//! into it (lower bounds) and out of it (upper bounds). Types form a
//! lattice: when they are shown, a variable's bounds become unions in
//! positive positions and intersections in negative ones.
//!
//! There is no `nil`. What may be missing is an attribute: a record
//! field typed `Opt(t)` may be absent, `{k?: t}`, and reading it needs a
//! default or a `match`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Prim {
    /// A whole number, as Dialog's `integer:`.
    Int,
    /// A whole number that is not negative, as Dialog's `natural:`: a
    /// view of the integers, so every natural is an integer.
    Nat,
    Bool,
    Str,
    /// A parsed JSON value, read with the `json-` built-ins.
    Json,
    /// Any keyword: what a dictionary's keys are.
    Keyword,
    /// `{}`: nothing at all, which is both an empty dictionary and a
    /// record with no attributes.
    Empty,
}

impl Prim {
    pub fn name(self) -> &'static str {
        match self {
            Prim::Int => "integer",
            Prim::Nat => "natural",
            Prim::Bool => "boolean",
            Prim::Str => "text",
            Prim::Json => "json",
            Prim::Keyword => "keyword",
            Prim::Empty => "{}",
        }
    }
}

pub struct VarState {
    pub id: u32,
    pub level: u32,
    pub lower: RefCell<Vec<Ty>>,
    pub upper: RefCell<Vec<Ty>>,
}

pub type Var = Rc<VarState>;

#[derive(Clone)]
pub enum Ty {
    Top,
    Bot,
    Prim(Prim),
    /// Parameters, result, effect (the capabilities a call may use), and
    /// the labels of every parameter but the first, in order.
    Fn(Rc<Vec<Ty>>, Rc<Ty>, Rc<Ty>, Rc<Vec<String>>),
    Record(Rc<BTreeMap<String, Ty>>),
    List(Rc<Ty>),
    Dict(Rc<Ty>),
    View,
    Effect,
    Var(Var),
    /// A record with one field set: what `(assoc r :k v)` returns. It
    /// only flows into things.
    Update(Rc<Ty>, String, Rc<Ty>),
    /// One of a finite set of variants, each a key with a payload or,
    /// for a keyword, without one. A keyword literal is the set of
    /// itself, and a record literal of one field, `{:ok t}`, the variant
    /// `ok` carrying `t`. Sets are ordered by inclusion, payloads
    /// covariantly, so they form a lattice of their own: a union of
    /// variants is one of these, never a union type.
    Tags(Rc<BTreeMap<String, Option<Ty>>>),
    /// What a value fits when, if it has the attribute `k`, the
    /// attribute's value fits `t`: CUE's optional field `{k?: t}`. It
    /// only bounds what flows in, and is how a pattern reads a variant's
    /// payload without requiring every variant to have it.
    IfHas(String, Rc<Ty>),
    /// A set of capabilities, ordered by inclusion like keyword sets. The
    /// empty set is pure.
    Caps(Rc<BTreeSet<String>>),
    /// The type of a record field that may be absent: `{k?: t}`. It only
    /// appears as a field's type.
    Opt(Rc<Ty>),
    /// A claim's type variable, `?a`, while a definition is checked
    /// against the claim: any type the caller chooses, so the definition
    /// can assume nothing of it.
    Rigid(Rc<str>),
    /// Work that, performed, answers its result and needs its
    /// capabilities: `(task {} with: {print})`. Covariant in both.
    Task(Rc<Ty>, Rc<Ty>),
    /// Capabilities and the given names besides: what a task needs once
    /// an environment has provided those names. It only bounds what
    /// flows into it.
    CapsPlus(Rc<Ty>, Rc<BTreeSet<String>>),
    /// One of several types, as a claim writes it: `[integer ?a]`. What
    /// flows in fits one of them; what flows out is any of them.
    Union(Rc<Vec<Ty>>),
    /// All of several types, as a claim writes it: `(& ?a {done: boolean})`
    /// or, for a variable that is a record as well, `{done: boolean ?a ...}`.
    Inter(Rc<Vec<Ty>>),
}

impl Ty {
    pub fn int() -> Ty {
        Ty::Prim(Prim::Int)
    }
    pub fn nat() -> Ty {
        Ty::Prim(Prim::Nat)
    }
    pub fn bool() -> Ty {
        Ty::Prim(Prim::Bool)
    }
    pub fn str() -> Ty {
        Ty::Prim(Prim::Str)
    }
    /// A pure function whose parameters after the first have `labels`.
    pub fn func(params: Vec<Ty>, ret: Ty, labels: &[&str]) -> Ty {
        Ty::Fn(
            Rc::new(params),
            Rc::new(ret),
            Rc::new(Ty::pure()),
            Rc::new(labels.iter().map(|l| l.to_string()).collect()),
        )
    }
    /// A function that may use the capabilities of `effect`.
    pub fn func_eff(params: Vec<Ty>, ret: Ty, effect: Ty, labels: Vec<String>) -> Ty {
        Ty::Fn(Rc::new(params), Rc::new(ret), Rc::new(effect), Rc::new(labels))
    }
    pub fn pure() -> Ty {
        Ty::Caps(Rc::new(BTreeSet::new()))
    }
    pub fn caps(caps: impl IntoIterator<Item = String>) -> Ty {
        Ty::Caps(Rc::new(caps.into_iter().collect()))
    }
    pub fn record(fields: impl IntoIterator<Item = (String, Ty)>) -> Ty {
        Ty::Record(Rc::new(fields.into_iter().collect()))
    }
    pub fn list(t: Ty) -> Ty {
        Ty::List(Rc::new(t))
    }
    /// A task answering `t` and needing `caps`.
    pub fn task(t: Ty, caps: Ty) -> Ty {
        Ty::Task(Rc::new(t), Rc::new(caps))
    }
    /// A record without the field `field`: what a record pattern's rest
    /// is. It is an update that sets the field to nothing.
    pub fn without(record: Ty, field: String) -> Ty {
        Ty::update(record, field, Ty::Opt(Rc::new(Ty::Bot)))
    }

    /// Whether a field set to this is no field at all.
    pub fn absent(&self) -> bool {
        matches!(self, Ty::Opt(t) if matches!(**t, Ty::Bot))
    }

    pub fn dict(t: Ty) -> Ty {
        Ty::Dict(Rc::new(t))
    }
    /// The type of a field that may be absent.
    pub fn opt(t: Ty) -> Ty {
        match t {
            Ty::Opt(_) => t,
            t => Ty::Opt(Rc::new(t)),
        }
    }

    pub fn keys(keys: impl IntoIterator<Item = String>) -> Ty {
        Ty::Tags(Rc::new(keys.into_iter().map(|k| (k, None)).collect()))
    }

    /// The variant `{key: payload}`.
    pub fn variant(key: impl Into<String>, payload: Ty) -> Ty {
        Ty::Tags(Rc::new([(key.into(), Some(payload))].into_iter().collect()))
    }

    /// One of several variants, each with or without a payload.
    pub fn tags(tags: impl IntoIterator<Item = (String, Option<Ty>)>) -> Ty {
        Ty::Tags(Rc::new(tags.into_iter().collect()))
    }

    pub fn if_has(key: impl Into<String>, t: Ty) -> Ty {
        Ty::IfHas(key.into(), Rc::new(t))
    }

    pub fn update(record: Ty, field: String, value: Ty) -> Ty {
        // Setting a field again sets it once.
        let record = match record {
            Ty::Update(inner, k, _) if k == field => (*inner).clone(),
            r => r,
        };
        Ty::Update(Rc::new(record), field, Rc::new(value))
    }

    /// The highest level of any variable in the type.
    pub fn level(&self) -> u32 {
        match self {
            Ty::Var(v) => v.level,
            Ty::Fn(params, ret, eff, _) => {
                params.iter().map(Ty::level).max().unwrap_or(0).max(ret.level()).max(eff.level())
            }
            Ty::Record(fields) => fields.values().map(Ty::level).max().unwrap_or(0),
            Ty::List(t) | Ty::Dict(t) | Ty::Opt(t) => t.level(),
            Ty::Task(t, c) => t.level().max(c.level()),
            Ty::CapsPlus(c, _) => c.level(),
            Ty::Update(r, _, v) => r.level().max(v.level()),
            Ty::Tags(tags) => tags.values().flatten().map(Ty::level).max().unwrap_or(0),
            Ty::IfHas(_, t) => t.level(),
            Ty::Union(ts) | Ty::Inter(ts) => ts.iter().map(Ty::level).max().unwrap_or(0),
            _ => 0,
        }
    }

    /// An identity for the constraint cache. Composite types are shared
    /// through `Rc`, so the address of their allocation names them.
    fn key(&self) -> Key {
        match self {
            Ty::Top => (0, 0),
            Ty::Bot => (1, 0),
            Ty::Prim(p) => (2, *p as usize),
            Ty::View => (3, 0),
            Ty::Effect => (4, 0),
            Ty::Var(v) => (5, v.id as usize),
            Ty::Fn(params, _, _, _) => (6, Rc::as_ptr(params) as *const u8 as usize),
            Ty::Record(fields) => (7, Rc::as_ptr(fields) as *const u8 as usize),
            Ty::List(t) => (8, Rc::as_ptr(t) as *const u8 as usize),
            Ty::Task(t, _) => (17, Rc::as_ptr(t) as *const u8 as usize),
            Ty::CapsPlus(c, _) => (18, Rc::as_ptr(c) as *const u8 as usize),
            Ty::Dict(t) => (9, Rc::as_ptr(t) as *const u8 as usize),
            Ty::Update(r, _, _) => (11, Rc::as_ptr(r) as *const u8 as usize),
            Ty::Tags(k) => (12, Rc::as_ptr(k) as *const u8 as usize),
            Ty::IfHas(_, t) => (15, Rc::as_ptr(t) as *const u8 as usize),
            Ty::Caps(c) => (13, Rc::as_ptr(c) as *const u8 as usize),
            Ty::Opt(t) => (14, Rc::as_ptr(t) as *const u8 as usize),
            Ty::Rigid(name) => (16, Rc::as_ptr(name) as *const u8 as usize),
            Ty::Union(ts) => (19, Rc::as_ptr(ts) as *const u8 as usize),
            Ty::Inter(ts) => (20, Rc::as_ptr(ts) as *const u8 as usize),
        }
    }
}

/// A type whose variables above `level` are generic.
#[derive(Clone)]
pub struct Scheme {
    pub level: u32,
    pub body: Ty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TypeError {}

/// A type's identity in the constraint cache.
type Key = (u8, usize);

/// The constraints already seen while solving one constraint.
#[derive(Default)]
struct Cache {
    pairs: HashSet<(Key, Key)>,
    /// Field reads through a record update, `record <: {name: t}`. The
    /// one-field record is built fresh for each read, so its address
    /// cannot identify the constraint; without this a model that flows
    /// back into its own update would be read through forever.
    reads: HashSet<(Key, String, Key)>,
}

/// Variable allocation and constraint solving.
#[derive(Default)]
pub struct Solver {
    next_id: Cell<u32>,
    /// While one of several types is tried, the bounds added, so that a
    /// try that fails can be undone: each variable, and whether the bound
    /// went to its upper bounds.
    trail: RefCell<Vec<(Var, bool)>>,
    trying: Cell<u32>,
}

impl Solver {
    pub fn fresh(&self, level: u32) -> Var {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        Rc::new(VarState { id, level, lower: RefCell::new(Vec::new()), upper: RefCell::new(Vec::new()) })
    }

    pub fn fresh_ty(&self, level: u32) -> Ty {
        Ty::Var(self.fresh(level))
    }

    /// Require `lhs` to be a subtype of `rhs`.
    pub fn constrain(&self, lhs: &Ty, rhs: &Ty) -> Result<(), TypeError> {
        let mut cache = Cache::default();
        self.go(lhs, rhs, &mut cache)
            .map_err(|e| TypeError(format!("{e}\n  while checking that {} fits {}", Shallow(lhs), Shallow(rhs))))
    }

    fn go(&self, lhs: &Ty, rhs: &Ty, cache: &mut Cache) -> Result<(), TypeError> {
        if !cache.pairs.insert((lhs.key(), rhs.key())) {
            return Ok(());
        }
        match (lhs, rhs) {
            (Ty::Bot, _) | (_, Ty::Top) => Ok(()),
            // Each of several types flows wherever all of them do; what
            // flows into all of several types flows into each.
            (Ty::Union(ls), _) => ls.iter().try_for_each(|l| self.go(l, rhs, cache)),
            (_, Ty::Inter(rs)) => rs.iter().try_for_each(|r| self.go(lhs, r, cache)),
            (Ty::Prim(a), Ty::Prim(b)) if a == b => Ok(()),
            // Every natural is an integer.
            (Ty::Prim(Prim::Nat), Ty::Prim(Prim::Int)) => Ok(()),
            // `{}` is an empty dictionary, and a record that has none of
            // the attributes it may have.
            (Ty::Prim(Prim::Empty), Ty::Dict(_)) => Ok(()),
            (Ty::Prim(Prim::Empty), Ty::Record(fields)) => {
                match fields.iter().find(|(_, t)| !matches!(t, Ty::Opt(_))) {
                    None => Ok(()),
                    Some((name, _)) => Err(TypeError(format!("`{{}}` has no attribute `{name}`"))),
                }
            }
            // Every keyword is a keyword; a variant with a payload is not.
            (Ty::Tags(tags), Ty::Prim(Prim::Keyword)) => match tags.iter().find(|(_, p)| p.is_some()) {
                None => Ok(()),
                Some((k, _)) => Err(TypeError(format!("{} is not a keyword", tag_text(k, true)))),
            },
            (Ty::View, Ty::View) | (Ty::Effect, Ty::Effect) => Ok(()),
            (Ty::Rigid(a), Ty::Rigid(b)) if a == b => Ok(()),
            (Ty::Tags(a), Ty::Tags(b)) => {
                let missing: Vec<&String> = a.keys().filter(|k| !b.contains_key(*k)).collect();
                if !missing.is_empty() {
                    return Err(TypeError(format!(
                        "{} is not one of {}",
                        missing.iter().map(|k| tag_text(k, a[*k].is_some())).collect::<Vec<_>>().join(" | "),
                        tags_text(b)
                    )));
                }
                for (k, pa) in a.iter() {
                    match (pa, &b[k]) {
                        (None, None) => {}
                        (Some(x), Some(y)) => self.go(x, y, cache)?,
                        (Some(_), None) => {
                            return Err(TypeError(format!("the variant `{{{k}: ..}}` is not the keyword `:{k}`")));
                        }
                        (None, Some(_)) => {
                            return Err(TypeError(format!("the keyword `:{k}` is not the variant `{{{k}: ..}}`")));
                        }
                    }
                }
                Ok(())
            }
            // A variant is a record of one field, so it can be read as one.
            (Ty::Tags(a), Ty::Record(fields)) => {
                for (k, payload) in a.iter() {
                    let Some(payload) = payload else {
                        return Err(TypeError(format!("the keyword `:{k}` is not a record")));
                    };
                    for (name, t) in fields.iter() {
                        if name == k {
                            self.go(payload, unopt(t), cache)?;
                        } else if !matches!(t, Ty::Opt(_)) {
                            return Err(TypeError(format!("the variant `{{{k}: ..}}` has no attribute `{name}`")));
                        }
                    }
                }
                Ok(())
            }
            (Ty::Fn(p0, r0, e0, l0), Ty::Fn(p1, r1, e1, l1)) => {
                if p0.len() == p1.len() && l0 != l1 {
                    let text = |l: &[String]| l.iter().map(|l| format!("{l}:")).collect::<Vec<_>>().join(" ");
                    return Err(TypeError(format!("a function labeled `{}` is called with `{}`", text(l0), text(l1))));
                }
                if p0.len() != p1.len() {
                    return Err(TypeError(format!(
                        "a function of {} argument(s) is called with {}",
                        p0.len(),
                        p1.len()
                    )));
                }
                // The result first: where a parameter is one of several
                // types, what the result needs decides which.
                self.go(r0, r1, cache)?;
                for (a, b) in p0.iter().zip(p1.iter()) {
                    self.go(b, a, cache)?;
                }
                self.go(e0, e1, cache)
            }
            (Ty::Caps(a), Ty::Caps(b)) => {
                let extra: Vec<&String> = a.difference(b).collect();
                if extra.is_empty() {
                    return Ok(());
                }
                let named = extra.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
                Err(TypeError(if b.is_empty() {
                    format!("this uses {named}, where only pure code is allowed")
                } else {
                    format!("this uses {named}, where only {} may be used", caps_text(b))
                }))
            }
            (Ty::Record(f0), Ty::Record(f1)) => {
                for (name, t1) in f1.iter() {
                    match (f0.get(name), t1) {
                        (Some(Ty::Opt(t0)), Ty::Opt(u)) => self.go(t0, u, cache)?,
                        (Some(Ty::Opt(_)), _) => {
                            return Err(TypeError(format!(
                                "the record {} may not have `{name}`: read it with a default, or `match` it",
                                Shallow(lhs)
                            )));
                        }
                        (Some(t0), t1) => self.go(t0, unopt(t1), cache)?,
                        // A field asked for only if it is there.
                        (None, Ty::Opt(_)) => {}
                        (None, _) => {
                            return Err(TypeError(format!("the record {} has no attribute `{name}`", Shallow(lhs))));
                        }
                    }
                }
                Ok(())
            }
            (Ty::List(a), Ty::List(b)) | (Ty::Dict(a), Ty::Dict(b)) | (Ty::Opt(a), Ty::Opt(b)) => self.go(a, b, cache),
            // What is there fits what may be.
            (_, Ty::Opt(b)) if !matches!(lhs, Ty::Var(_)) => self.go(lhs, b, cache),
            (Ty::Task(a, c), Ty::Task(b, d)) => {
                self.go(a, b, cache)?;
                self.go(c, d, cache)
            }
            // What an environment provides is not needed of the rest.
            (Ty::Caps(a), Ty::CapsPlus(rest, provided)) => {
                let left: BTreeSet<String> = a.difference(provided).cloned().collect();
                self.go(&Ty::Caps(Rc::new(left)), rest, cache)
            }
            (Ty::Rigid(_), Ty::CapsPlus(rest, _)) => self.go(lhs, rest, cache),
            (Ty::Var(v), _) if rhs.level() <= v.level => {
                self.bound(v, true, rhs.clone());
                let lowers = v.lower.borrow().clone();
                for l in &lowers {
                    self.go(l, rhs, cache)?;
                }
                Ok(())
            }
            (_, Ty::Var(v)) if lhs.level() <= v.level => {
                self.bound(v, false, lhs.clone());
                let uppers = v.upper.borrow().clone();
                for u in &uppers {
                    self.go(lhs, u, cache)?;
                }
                Ok(())
            }
            // All of several types fits where one of them does.
            (Ty::Inter(ls), _) => {
                // Records together are the record of all their fields.
                if let Ty::Record(_) | Ty::Tags(_) = rhs {
                    let mut fields = BTreeMap::new();
                    for l in ls.iter() {
                        match l {
                            Ty::Record(fs) => fields.extend(fs.iter().map(|(k, t)| (k.clone(), t.clone()))),
                            Ty::Tags(tags) if tags.len() == 1 => {
                                if let Some((k, Some(p))) = tags.iter().next() {
                                    fields.insert(k.clone(), p.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                    if fields.len() > 1 {
                        return self.go(&Ty::record(fields), rhs, cache);
                    }
                }
                // A member that says what is asked, before one that only
                // may: a record before a variable.
                let says = |l: &&Ty| match (l, rhs) {
                    (Ty::Record(fs), Ty::IfHas(k, _)) => fs.contains_key(k),
                    (Ty::Tags(tags), Ty::IfHas(k, _)) => tags.contains_key(k),
                    (_, Ty::IfHas(..)) => false,
                    (Ty::Rigid(_) | Ty::Var(_) | Ty::Top, _) => false,
                    _ => fits(l, rhs),
                };
                let mut order: Vec<&Ty> = ls.iter().filter(says).collect();
                order.extend(ls.iter().filter(|l| !says(l) && fits(l, rhs)));
                self.first(order.into_iter().map(|l| (l, rhs)), cache).unwrap_or_else(|| {
                    Err(TypeError(format!("{} is not {}", Shallow(lhs), article(&Shallow(rhs).to_string()))))
                })
            }
            // A type fits one of several where it fits that one.
            (_, Ty::Union(rs)) if !matches!(lhs, Ty::Var(_)) => self
                .first(rs.iter().filter(|r| fits(lhs, r)).map(|r| (lhs, r)), cache)
                .unwrap_or_else(|| Err(TypeError(format!("{} is not one of {}", Shallow(lhs), Shallow(rhs))))),
            // A record with a field set fits another with that field set
            // where the fields fit, and the records do but for that field.
            (Ty::Update(r0, k0, v0), Ty::Update(r1, k1, v1)) if k0 == k1 => {
                self.go(v0, v1, cache)?;
                self.go(r0, &Ty::update((**r1).clone(), k1.clone(), Ty::opt(Ty::Top)), cache)
            }
            (_, Ty::Update(r1, k, v1)) if !matches!(lhs, Ty::Var(_)) => {
                self.go(lhs, &relax(r1, k), cache)?;
                // Taken out, or anything if there, it asks nothing more.
                match &**v1 {
                    Ty::Opt(t) if matches!(**t, Ty::Bot | Ty::Top) => Ok(()),
                    v1 => self.go(lhs, &Ty::record([(k.clone(), v1.clone())]), cache),
                }
            }
            // A value without the attribute fits, whatever else it is.
            (_, Ty::IfHas(k, t)) if !matches!(lhs, Ty::Var(_) | Ty::Update(..)) => match lhs {
                Ty::Tags(tags) => match tags.get(k) {
                    Some(Some(payload)) => self.go(payload, t, cache),
                    _ => Ok(()),
                },
                Ty::Record(fields) => match fields.get(k) {
                    Some(v) => self.go(unopt(v), t, cache),
                    None => Ok(()),
                },
                Ty::Top => self.go(&Ty::Top, t, cache),
                _ => Ok(()),
            },
            (Ty::Update(record, field, value), _) if !matches!(rhs, Ty::Var(_)) => match rhs {
                // The updated field comes from the new value; every other
                // field the reader wants comes from the original record.
                Ty::Record(wanted) => {
                    for (name, t) in wanted.iter() {
                        // A field taken out is there only where it may
                        // be absent.
                        if name == field && value.absent() {
                            if !matches!(t, Ty::Opt(_)) {
                                return Err(TypeError(format!("the record has no `{field}`: a pattern took it out")));
                            }
                        } else if name == field {
                            self.go(value, unopt(t), cache)?;
                        } else if cache.reads.insert((record.key(), name.clone(), t.key())) {
                            self.go(record, &Ty::record([(name.clone(), t.clone())]), cache)?;
                        }
                    }
                    Ok(())
                }
                Ty::IfHas(k, _) if k == field && value.absent() => Ok(()),
                Ty::IfHas(k, t) if k == field => self.go(value, t, cache),
                Ty::IfHas(..) => self.go(record, rhs, cache),
                // A variant with its own field set is that variant again.
                Ty::Tags(tags) => match tags.get(field) {
                    // `{}` with one field set is the variant of that field.
                    Some(Some(payload)) if matches!(**record, Ty::Prim(Prim::Empty)) => self.go(value, payload, cache),
                    Some(Some(payload)) => {
                        self.go(value, payload, cache)?;
                        self.go(record, &Ty::variant(field.clone(), Ty::Top), cache)
                    }
                    _ => Err(TypeError(format!("{} is not one of {}", Shallow(lhs), tags_text(tags)))),
                },
                _ => Err(TypeError(format!("{} is not {}", Shallow(lhs), article(&Shallow(rhs).to_string())))),
            },
            (Ty::Var(v), _) => {
                let rhs = self.extrude(rhs, false, v.level, &mut HashMap::new());
                self.go(lhs, &rhs, cache)
            }
            (_, Ty::Var(v)) => {
                let lhs = self.extrude(lhs, true, v.level, &mut HashMap::new());
                self.go(&lhs, rhs, cache)
            }
            _ => Err(TypeError(format!("{} is not {}", Shallow(lhs), article(&Shallow(rhs).to_string())))),
        }
    }

    fn bound(&self, v: &Var, upper: bool, t: Ty) {
        if upper {
            v.upper.borrow_mut().push(t)
        } else {
            v.lower.borrow_mut().push(t)
        }
        if self.trying.get() > 0 {
            self.trail.borrow_mut().push((v.clone(), upper));
        }
    }

    /// The first of several constraints that holds, each tried in turn
    /// and undone if it does not: `None` if there are none to try, and
    /// the first one's error if none holds.
    fn first<'t>(
        &self,
        tries: impl Iterator<Item = (&'t Ty, &'t Ty)>,
        cache: &mut Cache,
    ) -> Option<Result<(), TypeError>> {
        let mut error = None;
        for (l, r) in tries {
            let mark = self.trail.borrow().len();
            let saved = (cache.pairs.clone(), cache.reads.clone());
            self.trying.set(self.trying.get() + 1);
            let out = self.go(l, r, cache);
            self.trying.set(self.trying.get() - 1);
            match out {
                Ok(()) => {
                    if self.trying.get() == 0 {
                        self.trail.borrow_mut().clear();
                    }
                    return Some(Ok(()));
                }
                Err(e) => {
                    let undone: Vec<(Var, bool)> = self.trail.borrow_mut().drain(mark..).collect();
                    for (v, upper) in undone.into_iter().rev() {
                        if upper {
                            v.upper.borrow_mut().pop()
                        } else {
                            v.lower.borrow_mut().pop()
                        };
                    }
                    (cache.pairs, cache.reads) = saved;
                    error.get_or_insert(Err(e));
                }
            }
        }
        error
    }

    /// Copy the parts of `ty` above `level` into variables at `level`,
    /// linked to the originals, so that a constraint between levels does
    /// not let a generic variable escape.
    fn extrude(&self, ty: &Ty, pol: bool, level: u32, cache: &mut HashMap<(u32, bool), Var>) -> Ty {
        if ty.level() <= level {
            return ty.clone();
        }
        match ty {
            Ty::Fn(params, ret, eff, labels) => Ty::func_eff(
                params.iter().map(|p| self.extrude(p, !pol, level, cache)).collect(),
                self.extrude(ret, pol, level, cache),
                self.extrude(eff, pol, level, cache),
                (**labels).clone(),
            ),
            Ty::Record(fields) => {
                Ty::record(fields.iter().map(|(k, t)| (k.clone(), self.extrude(t, pol, level, cache))))
            }
            Ty::List(t) => Ty::list(self.extrude(t, pol, level, cache)),
            Ty::Task(t, c) => Ty::task(self.extrude(t, pol, level, cache), self.extrude(c, pol, level, cache)),
            Ty::CapsPlus(c, l) => Ty::CapsPlus(Rc::new(self.extrude(c, pol, level, cache)), l.clone()),
            Ty::Dict(t) => Ty::dict(self.extrude(t, pol, level, cache)),
            Ty::Opt(t) => Ty::opt(self.extrude(t, pol, level, cache)),
            Ty::Tags(tags) => {
                Ty::tags(tags.iter().map(|(k, p)| (k.clone(), p.as_ref().map(|p| self.extrude(p, pol, level, cache)))))
            }
            Ty::IfHas(k, t) => Ty::if_has(k.clone(), self.extrude(t, pol, level, cache)),
            Ty::Update(r, k, v) => {
                Ty::update(self.extrude(r, pol, level, cache), k.clone(), self.extrude(v, pol, level, cache))
            }
            Ty::Union(ts) => Ty::Union(Rc::new(ts.iter().map(|t| self.extrude(t, pol, level, cache)).collect())),
            Ty::Inter(ts) => Ty::Inter(Rc::new(ts.iter().map(|t| self.extrude(t, pol, level, cache)).collect())),
            Ty::Var(v) => {
                if let Some(nv) = cache.get(&(v.id, pol)) {
                    return Ty::Var(nv.clone());
                }
                let nv = self.fresh(level);
                cache.insert((v.id, pol), nv.clone());
                if pol {
                    self.bound(v, true, Ty::Var(nv.clone()));
                    let lowers = v.lower.borrow().clone();
                    let copied = lowers.iter().map(|l| self.extrude(l, pol, level, cache)).collect();
                    *nv.lower.borrow_mut() = copied;
                } else {
                    self.bound(v, false, Ty::Var(nv.clone()));
                    let uppers = v.upper.borrow().clone();
                    let copied = uppers.iter().map(|u| self.extrude(u, pol, level, cache)).collect();
                    *nv.upper.borrow_mut() = copied;
                }
                Ty::Var(nv)
            }
            other => other.clone(),
        }
    }

    /// A copy of the scheme's body with fresh variables, at `level`.
    pub fn instantiate(&self, scheme: &Scheme, level: u32) -> Ty {
        let mut map = HashMap::new();
        self.freshen(&scheme.body, scheme.level, level, &mut map)
    }

    fn freshen(&self, ty: &Ty, limit: u32, level: u32, map: &mut HashMap<u32, Var>) -> Ty {
        if ty.level() <= limit {
            return ty.clone();
        }
        match ty {
            Ty::Var(v) => {
                if let Some(nv) = map.get(&v.id) {
                    return Ty::Var(nv.clone());
                }
                let nv = self.fresh(level);
                map.insert(v.id, nv.clone());
                let lowers = v.lower.borrow().clone();
                let uppers = v.upper.borrow().clone();
                let l = lowers.iter().map(|t| self.freshen(t, limit, level, map)).collect();
                let u = uppers.iter().map(|t| self.freshen(t, limit, level, map)).collect();
                *nv.lower.borrow_mut() = l;
                *nv.upper.borrow_mut() = u;
                Ty::Var(nv)
            }
            Ty::Fn(params, ret, eff, labels) => Ty::func_eff(
                params.iter().map(|p| self.freshen(p, limit, level, map)).collect(),
                self.freshen(ret, limit, level, map),
                self.freshen(eff, limit, level, map),
                (**labels).clone(),
            ),
            Ty::Record(fields) => {
                Ty::record(fields.iter().map(|(k, t)| (k.clone(), self.freshen(t, limit, level, map))))
            }
            Ty::List(t) => Ty::list(self.freshen(t, limit, level, map)),
            Ty::Task(t, c) => Ty::task(self.freshen(t, limit, level, map), self.freshen(c, limit, level, map)),
            Ty::CapsPlus(c, l) => Ty::CapsPlus(Rc::new(self.freshen(c, limit, level, map)), l.clone()),
            Ty::Dict(t) => Ty::dict(self.freshen(t, limit, level, map)),
            Ty::Opt(t) => Ty::opt(self.freshen(t, limit, level, map)),
            Ty::Tags(tags) => {
                Ty::tags(tags.iter().map(|(k, p)| (k.clone(), p.as_ref().map(|p| self.freshen(p, limit, level, map)))))
            }
            Ty::IfHas(k, t) => Ty::if_has(k.clone(), self.freshen(t, limit, level, map)),
            Ty::Update(r, k, v) => {
                Ty::update(self.freshen(r, limit, level, map), k.clone(), self.freshen(v, limit, level, map))
            }
            Ty::Union(ts) => Ty::Union(Rc::new(ts.iter().map(|t| self.freshen(t, limit, level, map)).collect())),
            Ty::Inter(ts) => Ty::Inter(Rc::new(ts.iter().map(|t| self.freshen(t, limit, level, map)).collect())),
            other => other.clone(),
        }
    }
}

/// What a record is but for the field `k`, as far as it can be said:
/// what a record with `k` set asks of the rest.
fn relax(r: &Ty, k: &str) -> Ty {
    match r {
        Ty::Record(fields) => Ty::record(fields.iter().filter(|(f, _)| *f != k).map(|(f, t)| (f.clone(), t.clone()))),
        Ty::Tags(tags) if tags.len() == 1 && tags.contains_key(k) => Ty::Top,
        r => r.clone(),
    }
}

/// Whether `l` may fit `r` by their outermost shapes: how a type is
/// matched to one of a union's members, or one of an intersection's
/// members to what it flows into.
fn fits(l: &Ty, r: &Ty) -> bool {
    match (l, r) {
        (Ty::Bot, _) | (_, Ty::Top) | (Ty::Var(_), _) | (_, Ty::Var(_)) | (_, Ty::IfHas(..)) => true,
        (Ty::Union(ls), _) => ls.iter().all(|l| fits(l, r)),
        (_, Ty::Union(rs)) => rs.iter().any(|r| fits(l, r)),
        (Ty::Inter(ls), _) => ls.iter().any(|l| fits(l, r)),
        (_, Ty::Inter(rs)) => rs.iter().all(|r| fits(l, r)),
        (Ty::Prim(a), Ty::Prim(b)) => a == b || (*a == Prim::Nat && *b == Prim::Int),
        (Ty::Prim(Prim::Empty), Ty::Dict(_) | Ty::Record(_)) => true,
        (Ty::Tags(tags), Ty::Prim(Prim::Keyword)) => tags.values().all(Option::is_none),
        (Ty::Tags(a), Ty::Tags(b)) => a.keys().all(|k| b.contains_key(k)),
        (Ty::Tags(_), Ty::Record(_)) => true,
        (Ty::Record(a), Ty::Record(b)) => b.iter().all(|(k, t)| matches!(t, Ty::Opt(_)) || a.contains_key(k)),
        (Ty::Rigid(a), Ty::Rigid(b)) => a == b,
        (Ty::Fn(a, ..), Ty::Fn(b, ..)) => a.len() == b.len(),
        (Ty::List(_), Ty::List(_))
        | (Ty::Dict(_), Ty::Dict(_))
        | (Ty::Task(..), Ty::Task(..))
        | (Ty::View, Ty::View)
        | (Ty::Effect, Ty::Effect)
        | (Ty::Caps(_), Ty::Caps(_) | Ty::CapsPlus(..))
        | (Ty::Rigid(_), Ty::CapsPlus(..)) => true,
        (Ty::Update(..), Ty::Record(_) | Ty::Tags(_) | Ty::Update(..)) => true,
        (_, Ty::Update(r, ..)) => fits(l, r),
        _ => false,
    }
}

/// A one-level rendering of a type, for error messages.
pub struct Shallow<'a>(pub &'a Ty);

impl fmt::Display for Shallow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Ty::Top => write!(f, "any"),
            Ty::Bot => write!(f, "nothing"),
            Ty::Prim(p) => write!(f, "{}", p.name()),
            Ty::View => write!(f, "view"),
            Ty::Rigid(name) => write!(f, "?{name}, any type the caller chooses"),
            Ty::Effect => write!(f, "effect"),
            Ty::Var(_) => write!(f, "a value whose type is still being inferred"),
            Ty::Fn(params, _, _, _) => write!(f, "a function of {} argument(s)", params.len()),
            Ty::Caps(c) => write!(f, "{}", caps_text(c)),
            Ty::Record(fields) => {
                write!(f, "{{")?;
                for (i, name) in fields.keys().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{name}:")?;
                }
                write!(f, "}}")
            }
            Ty::List(_) => write!(f, "a list"),
            Ty::Task(..) => write!(f, "task"),
            Ty::CapsPlus(c, l) => write!(f, "{} and {}", Shallow(c), caps_text(l)),
            Ty::Dict(_) => write!(f, "a dictionary"),
            Ty::Opt(t) => write!(f, "{} if present", Shallow(t)),
            Ty::Update(r, k, _) => write!(f, "{} with `{k}` set", Shallow(r)),
            Ty::Tags(k) => write!(f, "{}", tags_text(k)),
            Ty::IfHas(k, _) => write!(f, "{{{k}?: ..}}"),
            Ty::Union(ts) => {
                let all: Vec<String> = ts.iter().map(|t| Shallow(t).to_string()).collect();
                write!(f, "[{}]", all.join(" "))
            }
            Ty::Inter(ts) => {
                // A variable still being inferred bounds nothing a reader
                // can act on: `(& ?a integer)` is said as `integer`.
                let known: Vec<String> =
                    ts.iter().filter(|t| !matches!(t, Ty::Var(_))).map(|t| Shallow(t).to_string()).collect();
                match known.len() {
                    0 => write!(f, "a value whose type is still being inferred"),
                    1 => write!(f, "{}", known[0]),
                    _ => write!(f, "(& {})", known.join(" ")),
                }
            }
        }
    }
}

/// A type's name as a sentence says it: `an integer`, `a text`, and
/// as it is where it is not a word.
fn article(name: &str) -> String {
    match name.chars().next() {
        _ if name.starts_with("a ") || name.starts_with("an ") => name.to_string(),
        Some(c) if "aeiou".contains(c) => format!("an {name}"),
        Some(c) if c.is_ascii_alphabetic() => format!("a {name}"),
        _ => name.to_string(),
    }
}

fn caps_text(caps: &BTreeSet<String>) -> String {
    format!("{{{}}}", caps.iter().cloned().collect::<Vec<_>>().join(" "))
}

fn tag_text(k: &str, payload: bool) -> String {
    if payload { format!("{{{k}: ..}}") } else { format!(":{k}") }
}

fn tags_text(tags: &BTreeMap<String, Option<Ty>>) -> String {
    let all: Vec<String> = tags.iter().map(|(k, p)| tag_text(k, p.is_some())).collect();
    if all.len() == 1 { all.into_iter().next().unwrap() } else { format!("[{}]", all.join(" ")) }
}

/// A type as shown to a person: variables' bounds turned into unions
/// and intersections.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Top,
    Bot,
    Prim(Prim),
    View,
    Effect,
    Var(u32),
    Fn(Vec<Type>, Box<Type>, Box<Type>, Vec<String>),
    Record(BTreeMap<String, Type>),
    List(Box<Type>),
    Dict(Box<Type>),
    Union(Vec<Type>),
    Inter(Vec<Type>),
    Rec(u32, Box<Type>),
    /// A field that may be absent.
    Opt(Box<Type>),
    Update(Box<Type>, String, Box<Type>),
    Tags(BTreeMap<String, Option<Type>>),
    Caps(BTreeSet<String>),
    /// A claim's type variable, shown as written.
    Named(String),
    Task(Box<Type>, Box<Type>),
}

/// Turn an inferred type into a displayable one and simplify it.
pub fn coalesce(ty: &Ty) -> Type {
    let mut recursive: HashMap<(u32, bool), u32> = HashMap::new();
    let mut next = 0x8000_0000u32;
    let raw = coalesce_go(ty, true, &mut Vec::new(), &mut recursive, &mut next);
    simplify(raw)
}

fn coalesce_go(
    ty: &Ty,
    pol: bool,
    in_process: &mut Vec<(u32, bool)>,
    recursive: &mut HashMap<(u32, bool), u32>,
    next: &mut u32,
) -> Type {
    match ty {
        Ty::Top => Type::Top,
        Ty::Bot => Type::Bot,
        Ty::Prim(p) => Type::Prim(*p),
        Ty::View => Type::View,
        Ty::Rigid(name) => Type::Named(name.to_string()),
        Ty::Effect => Type::Effect,
        Ty::Tags(tags) => Type::Tags(
            tags.iter()
                .map(|(k, p)| (k.clone(), p.as_ref().map(|p| coalesce_go(p, pol, in_process, recursive, next))))
                .collect(),
        ),
        // Asking for `k` only if it is there is a record with `k?`, and
        // asks nothing if anything may be there.
        Ty::IfHas(k, t) => match coalesce_go(t, pol, in_process, recursive, next) {
            Type::Top => Type::Top,
            t => Type::Record([(k.clone(), Type::Opt(Box::new(t)))].into_iter().collect()),
        },
        Ty::Caps(c) => Type::Caps((**c).clone()),
        Ty::Fn(params, ret, eff, labels) => Type::Fn(
            params.iter().map(|p| coalesce_go(p, !pol, in_process, recursive, next)).collect(),
            Box::new(coalesce_go(ret, pol, in_process, recursive, next)),
            Box::new(coalesce_go(eff, pol, in_process, recursive, next)),
            (**labels).clone(),
        ),
        Ty::Record(fields) => Type::Record(
            fields.iter().map(|(k, t)| (k.clone(), coalesce_go(t, pol, in_process, recursive, next))).collect(),
        ),
        Ty::List(t) => Type::List(Box::new(coalesce_go(t, pol, in_process, recursive, next))),
        Ty::Task(t, c) => Type::Task(
            Box::new(coalesce_go(t, pol, in_process, recursive, next)),
            Box::new(coalesce_go(c, pol, in_process, recursive, next)),
        ),
        Ty::CapsPlus(c, _) => coalesce_go(c, pol, in_process, recursive, next),
        Ty::Dict(t) => Type::Dict(Box::new(coalesce_go(t, pol, in_process, recursive, next))),
        Ty::Opt(t) => Type::Opt(Box::new(coalesce_go(t, pol, in_process, recursive, next))),
        Ty::Update(r, k, v) => Type::Update(
            Box::new(coalesce_go(r, pol, in_process, recursive, next)),
            k.clone(),
            Box::new(coalesce_go(v, pol, in_process, recursive, next)),
        ),
        Ty::Union(ts) => Type::Union(ts.iter().map(|t| coalesce_go(t, pol, in_process, recursive, next)).collect()),
        Ty::Inter(ts) => Type::Inter(ts.iter().map(|t| coalesce_go(t, pol, in_process, recursive, next)).collect()),
        Ty::Var(v) => {
            let key = (v.id, pol);
            if in_process.contains(&key) {
                let id = *recursive.entry(key).or_insert_with(|| {
                    *next += 1;
                    *next
                });
                return Type::Var(id);
            }
            in_process.push(key);
            let mut parts = vec![Type::Var(v.id)];
            if pol {
                let lowers = v.lower.borrow().clone();
                parts.extend(lowers.iter().map(|l| coalesce_go(l, pol, in_process, recursive, next)));
            } else {
                let uppers = v.upper.borrow().clone();
                parts.extend(uppers.iter().map(|u| coalesce_go(u, pol, in_process, recursive, next)));
            }
            in_process.pop();
            let body = if pol { Type::Union(parts) } else { Type::Inter(parts) };
            match recursive.get(&key) {
                Some(id) => Type::Rec(*id, Box::new(body)),
                None => body,
            }
        }
    }
}

fn simplify(ty: Type) -> Type {
    // Variables that occur in only one polarity carry no information:
    // a variable only ever flowed into, or only ever flowed out of, is
    // the same as nothing (in a union) or anything (in an intersection).
    let mut pos = HashSet::new();
    let mut neg = HashSet::new();
    occurrences(&ty, true, &mut pos, &mut neg);
    let keep: HashSet<u32> = pos.intersection(&neg).copied().collect();
    let ty = prune(ty, true, &keep);
    // A recursive variable no constructor guards says nothing, and would
    // keep the variables beside it from merging.
    let mut ty = normalize(vacuous(normalize(ty), true));
    // Merge or drop variables by what they co-occur with, until nothing
    // changes.
    for _ in 0..64 {
        let next = normalize(cooccurrence(ty.clone()));
        if next == ty {
            break;
        }
        ty = next;
    }
    rename(vacuous(ty, true))
}

/// Where a value goes in, a record that asks only for fields that may
/// be absent and may be anything asks nothing. (Such a bound comes from
/// a pattern that reads a field only where the value has it.)
fn vacuous(ty: Type, pol: bool) -> Type {
    match ty {
        Type::Record(fields)
            if !pol && !fields.is_empty() && fields.values().all(|t| *t == Type::Opt(Box::new(Type::Top))) =>
        {
            Type::Top
        }
        Type::Fn(params, ret, eff, labels) => Type::Fn(
            params.into_iter().map(|p| vacuous(p, !pol)).collect(),
            Box::new(vacuous(*ret, pol)),
            Box::new(vacuous(*eff, pol)),
            labels,
        ),
        Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, vacuous(t, pol))).collect()),
        Type::List(t) => Type::List(Box::new(vacuous(*t, pol))),
        Type::Task(t, c) => Type::Task(Box::new(vacuous(*t, pol)), Box::new(vacuous(*c, pol))),
        Type::Dict(t) => Type::Dict(Box::new(vacuous(*t, pol))),
        Type::Opt(t) => Type::Opt(Box::new(vacuous(*t, pol))),
        Type::Union(ts) => normalize(Type::Union(ts.into_iter().map(|t| vacuous(t, pol)).collect())),
        Type::Inter(ts) => normalize(Type::Inter(ts.into_iter().map(|t| vacuous(t, pol)).collect())),
        Type::Rec(v, body) => {
            // `rec a. a & T` is `T`: an occurrence of the variable that no
            // constructor guards says nothing. Bare, it is anything going
            // in and nothing coming out.
            let body = vacuous(*body, pol);
            let unguarded = |t: &Type| *t == Type::Var(v);
            let body = match body {
                Type::Var(x) if x == v => return if pol { Type::Bot } else { Type::Top },
                Type::Union(ts) if ts.iter().any(unguarded) => {
                    normalize(Type::Union(ts.into_iter().filter(|t| !unguarded(t)).collect()))
                }
                Type::Inter(ts) if ts.iter().any(unguarded) => {
                    normalize(Type::Inter(ts.into_iter().filter(|t| !unguarded(t)).collect()))
                }
                other => other,
            };
            if mentions(&body, v) { Type::Rec(v, Box::new(body)) } else { body }
        }
        other => other,
    }
}

/// Simple-sub's co-occurrence simplification. A variable that, in every
/// union it appears in and in every intersection it appears in, appears
/// alongside the same other variable is that variable; one that always
/// appears alongside the same concrete type adds nothing to it.
fn cooccurrence(ty: Type) -> Type {
    // For each variable and polarity, the members it always appears with.
    let mut co: HashMap<(u32, bool), Vec<Type>> = HashMap::new();
    let mut bound = HashSet::new();
    fn visit(ty: &Type, pol: bool, co: &mut HashMap<(u32, bool), Vec<Type>>, bound: &mut HashSet<u32>) {
        let group: Option<&Vec<Type>> = match ty {
            Type::Union(ts) if pol => Some(ts),
            Type::Inter(ts) if !pol => Some(ts),
            _ => None,
        };
        let singleton;
        let members: &[Type] = match (group, ty) {
            (Some(ts), _) => ts,
            (None, Type::Var(_)) => {
                singleton = [ty.clone()];
                &singleton
            }
            _ => &[],
        };
        for m in members {
            if let Type::Var(v) = m {
                let others: Vec<Type> = members.iter().filter(|o| *o != m).map(as_record).collect();
                co.entry((*v, pol)).and_modify(|prev| prev.retain(|t| others.contains(t))).or_insert(others);
            }
        }
        match ty {
            Type::Fn(params, ret, eff, _) => {
                params.iter().for_each(|p| visit(p, !pol, co, bound));
                visit(ret, pol, co, bound);
                visit(eff, pol, co, bound);
            }
            Type::Record(fields) => fields.values().for_each(|t| visit(t, pol, co, bound)),
            Type::Tags(tags) => tags.values().flatten().for_each(|t| visit(t, pol, co, bound)),
            Type::List(t) | Type::Dict(t) | Type::Opt(t) => visit(t, pol, co, bound),
            Type::Task(t, c) => {
                visit(t, pol, co, bound);
                visit(c, pol, co, bound);
            }
            Type::Update(r, _, v) => {
                visit(r, pol, co, bound);
                visit(v, pol, co, bound);
            }
            // The variables directly in a group were counted with it
            // above; visiting them alone would count them again.
            Type::Union(ts) | Type::Inter(ts) => {
                ts.iter().filter(|t| !matches!(t, Type::Var(_))).for_each(|t| visit(t, pol, co, bound))
            }
            Type::Rec(v, body) => {
                bound.insert(*v);
                visit(body, pol, co, bound);
            }
            _ => {}
        }
    }
    visit(&ty, true, &mut co, &mut bound);
    let mut vars: Vec<u32> = co.keys().map(|k| k.0).collect();
    vars.sort();
    vars.dedup();
    for v in vars {
        if bound.contains(&v) {
            continue;
        }
        let (Some(pos), Some(neg)) = (co.get(&(v, true)), co.get(&(v, false))) else { continue };
        for t in pos {
            if !neg.contains(t) {
                continue;
            }
            match t {
                Type::Var(w) if *w != v && !bound.contains(w) => {
                    return substitute(ty, v, &Type::Var(*w));
                }
                Type::Var(_) => {}
                // Always beside the same concrete type in both
                // polarities: the variable can only ever be that type.
                _ => return drop_var(ty, v, true),
            }
        }
    }
    // Two variables that every negative occurrence has together are one:
    // what flows in flows into both, so instantiating both with what
    // flows in is as general, where they only flow out apart.
    let mut vars: Vec<u32> = co.keys().filter(|k| !k.1).map(|k| k.0).collect();
    vars.sort();
    for v in vars {
        if bound.contains(&v) {
            continue;
        }
        let Some(neg) = co.get(&(v, false)) else { continue };
        for t in neg {
            if let Type::Var(w) = t
                && *w != v
                && !bound.contains(w)
                && co.get(&(*w, false)).is_some_and(|other| other.contains(&Type::Var(v)))
            {
                return substitute(ty, v, &Type::Var(*w));
            }
        }
    }
    ty
}

fn substitute(ty: Type, v: u32, with: &Type) -> Type {
    map_type(ty, &|t| if *t == Type::Var(v) { Some(with.clone()) } else { None })
}

fn drop_var(ty: Type, v: u32, pol: bool) -> Type {
    match ty {
        Type::Var(x) if x == v => {
            if pol {
                Type::Bot
            } else {
                Type::Top
            }
        }
        Type::Fn(params, ret, eff, labels) => Type::Fn(
            params.into_iter().map(|p| drop_var(p, v, !pol)).collect(),
            Box::new(drop_var(*ret, v, pol)),
            Box::new(drop_var(*eff, v, pol)),
            labels,
        ),
        Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, drop_var(t, v, pol))).collect()),
        Type::List(t) => Type::List(Box::new(drop_var(*t, v, pol))),
        Type::Task(t, c) => Type::Task(Box::new(drop_var(*t, v, pol)), Box::new(drop_var(*c, v, pol))),
        Type::Dict(t) => Type::Dict(Box::new(drop_var(*t, v, pol))),
        Type::Opt(t) => Type::Opt(Box::new(drop_var(*t, v, pol))),
        Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, p)| (k, p.map(|p| drop_var(p, v, pol)))).collect()),
        Type::Update(r, k, x) => Type::Update(Box::new(drop_var(*r, v, pol)), k, Box::new(drop_var(*x, v, pol))),
        Type::Union(ts) => Type::Union(ts.into_iter().map(|t| drop_var(t, v, pol)).collect()),
        Type::Inter(ts) => Type::Inter(ts.into_iter().map(|t| drop_var(t, v, pol)).collect()),
        Type::Rec(x, body) => Type::Rec(x, Box::new(drop_var(*body, v, pol))),
        other => other,
    }
}

impl Type {
    /// The type with every attribute's key renamed by `f`: global names
    /// shown by the local names a codebase claims them under.
    pub fn rename_keys(self, f: &dyn Fn(&str) -> String) -> Type {
        let go = |t: Type| t.rename_keys(f);
        let boxed = |t: Box<Type>| Box::new(t.rename_keys(f));
        match self {
            Type::Fn(params, ret, eff, labels) => {
                Type::Fn(params.into_iter().map(go).collect(), boxed(ret), boxed(eff), labels)
            }
            Type::Record(fields) => Type::Record(
                fields.into_iter().map(|(k, t)| (f(&k).trim_start_matches(CONSTRUCTOR).to_string(), go(t))).collect(),
            ),
            Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, t)| (f(&k), t.map(go))).collect()),
            Type::List(t) => Type::List(boxed(t)),
            Type::Task(t, c) => Type::Task(boxed(t), boxed(c)),
            Type::Dict(t) => Type::Dict(boxed(t)),
            Type::Opt(t) => Type::Opt(boxed(t)),
            Type::Rec(v, t) => Type::Rec(v, boxed(t)),
            Type::Update(r, k, v) => {
                Type::Update(boxed(r), f(&k).trim_start_matches(CONSTRUCTOR).to_string(), boxed(v))
            }
            Type::Union(ts) => Type::Union(ts.into_iter().map(go).collect()),
            Type::Inter(ts) => Type::Inter(ts.into_iter().map(go).collect()),
            other => other,
        }
    }
}

fn map_type(ty: Type, f: &dyn Fn(&Type) -> Option<Type>) -> Type {
    if let Some(t) = f(&ty) {
        return t;
    }
    match ty {
        Type::Fn(params, ret, eff, labels) => Type::Fn(
            params.into_iter().map(|p| map_type(p, f)).collect(),
            Box::new(map_type(*ret, f)),
            Box::new(map_type(*eff, f)),
            labels,
        ),
        Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, map_type(t, f))).collect()),
        Type::List(t) => Type::List(Box::new(map_type(*t, f))),
        Type::Task(t, c) => Type::Task(Box::new(map_type(*t, f)), Box::new(map_type(*c, f))),
        Type::Dict(t) => Type::Dict(Box::new(map_type(*t, f))),
        Type::Opt(t) => Type::Opt(Box::new(map_type(*t, f))),
        Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, p)| (k, p.map(|p| map_type(p, f)))).collect()),
        Type::Update(r, k, v) => Type::Update(Box::new(map_type(*r, f)), k, Box::new(map_type(*v, f))),
        Type::Union(ts) => Type::Union(ts.into_iter().map(|t| map_type(t, f)).collect()),
        Type::Inter(ts) => Type::Inter(ts.into_iter().map(|t| map_type(t, f)).collect()),
        Type::Rec(x, body) => Type::Rec(x, Box::new(map_type(*body, f))),
        other => other,
    }
}

fn occurrences(ty: &Type, pol: bool, pos: &mut HashSet<u32>, neg: &mut HashSet<u32>) {
    match ty {
        Type::Var(v) => {
            if pol {
                pos.insert(*v);
            } else {
                neg.insert(*v);
            }
        }
        Type::Fn(params, ret, eff, _) => {
            params.iter().for_each(|p| occurrences(p, !pol, pos, neg));
            occurrences(ret, pol, pos, neg);
            occurrences(eff, pol, pos, neg);
        }
        Type::Record(fields) => fields.values().for_each(|t| occurrences(t, pol, pos, neg)),
        Type::Tags(tags) => tags.values().flatten().for_each(|t| occurrences(t, pol, pos, neg)),
        Type::List(t) | Type::Dict(t) | Type::Opt(t) => occurrences(t, pol, pos, neg),
        Type::Task(t, c) => {
            occurrences(t, pol, pos, neg);
            occurrences(c, pol, pos, neg);
        }
        Type::Update(r, _, v) => {
            occurrences(r, pol, pos, neg);
            occurrences(v, pol, pos, neg);
        }
        Type::Union(ts) | Type::Inter(ts) => ts.iter().for_each(|t| occurrences(t, pol, pos, neg)),
        Type::Rec(v, body) => {
            // A recursive binder is referenced from inside its body.
            pos.insert(*v);
            neg.insert(*v);
            occurrences(body, pol, pos, neg);
        }
        _ => {}
    }
}

fn prune(ty: Type, pol: bool, keep: &HashSet<u32>) -> Type {
    match ty {
        Type::Var(v) if !keep.contains(&v) => {
            if pol {
                Type::Bot
            } else {
                Type::Top
            }
        }
        Type::Fn(params, ret, eff, labels) => Type::Fn(
            params.into_iter().map(|p| prune(p, !pol, keep)).collect(),
            Box::new(prune(*ret, pol, keep)),
            Box::new(prune(*eff, pol, keep)),
            labels,
        ),
        Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, prune(t, pol, keep))).collect()),
        Type::List(t) => Type::List(Box::new(prune(*t, pol, keep))),
        Type::Task(t, c) => Type::Task(Box::new(prune(*t, pol, keep)), Box::new(prune(*c, pol, keep))),
        Type::Dict(t) => Type::Dict(Box::new(prune(*t, pol, keep))),
        Type::Opt(t) => Type::Opt(Box::new(prune(*t, pol, keep))),
        Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, p)| (k, p.map(|p| prune(p, pol, keep)))).collect()),
        Type::Update(r, k, v) => Type::Update(Box::new(prune(*r, pol, keep)), k, Box::new(prune(*v, pol, keep))),
        Type::Union(ts) => Type::Union(ts.into_iter().map(|t| prune(t, pol, keep)).collect()),
        Type::Inter(ts) => Type::Inter(ts.into_iter().map(|t| prune(t, pol, keep)).collect()),
        Type::Rec(v, body) => Type::Rec(v, Box::new(prune(*body, pol, keep))),
        other => other,
    }
}

fn normalize(ty: Type) -> Type {
    match ty {
        Type::Fn(params, ret, eff, labels) => Type::Fn(
            params.into_iter().map(normalize).collect(),
            Box::new(normalize(*ret)),
            Box::new(normalize(*eff)),
            labels,
        ),
        Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, normalize(t))).collect()),
        Type::List(t) => Type::List(Box::new(normalize(*t))),
        Type::Task(t, c) => Type::Task(Box::new(normalize(*t)), Box::new(normalize(*c))),
        Type::Dict(t) => Type::Dict(Box::new(normalize(*t))),
        Type::Rec(v, body) => {
            let body = normalize(*body);
            if mentions(&body, v) { Type::Rec(v, Box::new(body)) } else { body }
        }
        Type::Opt(t) => Type::Opt(Box::new(normalize(*t))),
        Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, p)| (k, p.map(normalize))).collect()),
        Type::Update(r, k, v) => update(normalize(*r), k, normalize(*v)),
        Type::Union(ts) => {
            let mut out: Vec<Type> = Vec::new();
            for t in ts.into_iter().map(normalize) {
                match t {
                    Type::Union(inner) => out.extend(inner),
                    Type::Bot => {}
                    t => out.push(t),
                }
            }
            if out.contains(&Type::Top) {
                return Type::Top;
            }
            // `{}` adds nothing to a dictionary or a record beside it.
            if out.iter().any(|t| matches!(t, Type::Dict(_) | Type::Record(_))) {
                out.retain(|t| *t != Type::Prim(Prim::Empty));
            }
            // Nor a natural to an integer.
            if out.contains(&Type::Prim(Prim::Int)) {
                out.retain(|t| *t != Type::Prim(Prim::Nat));
            }
            merge_tags(&mut out, true);
            records_beside_records(&mut out);
            merge_records(&mut out, true);
            merge_containers(&mut out, true);
            merge_caps(&mut out, true);
            out.sort_by(display_order);
            out.dedup();
            match out.len() {
                0 => Type::Bot,
                1 => out.pop().unwrap(),
                _ => Type::Union(out),
            }
        }
        Type::Inter(ts) => {
            let mut out: Vec<Type> = Vec::new();
            for t in ts.into_iter().map(normalize) {
                match t {
                    Type::Inter(inner) => out.extend(inner),
                    Type::Top => {}
                    t => out.push(t),
                }
            }
            if out.contains(&Type::Bot) {
                return Type::Bot;
            }
            // A natural that is an integer as well is a natural.
            if out.contains(&Type::Prim(Prim::Nat)) {
                out.retain(|t| *t != Type::Prim(Prim::Int));
            }
            optional_reads_into_variants(&mut out);
            records_beside_records(&mut out);
            merge_records(&mut out, false);
            merge_containers(&mut out, false);
            merge_tags(&mut out, false);
            merge_caps(&mut out, false);
            if out.contains(&Type::Bot) {
                return Type::Bot;
            }
            out.sort_by(display_order);
            out.dedup();
            match out.len() {
                0 => Type::Top,
                1 => out.pop().unwrap(),
                _ => Type::Inter(out),
            }
        }
        other => other,
    }
}

/// Variables first, then concrete types.
fn display_order(a: &Type, b: &Type) -> std::cmp::Ordering {
    fn rank(t: &Type) -> u8 {
        match t {
            Type::Var(_) => 0,
            _ => 1,
        }
    }
    rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
}

/// Variant sets: a union of sets is their union, an intersection their
/// intersection, which may be empty. Payloads of the same key combine
/// the same way.
fn merge_tags(parts: &mut Vec<Type>, union: bool) {
    let sets: Vec<BTreeMap<String, Option<Type>>> =
        parts.iter().filter_map(|t| if let Type::Tags(k) = t { Some(k.clone()) } else { None }).collect();
    if sets.len() < 2 {
        return;
    }
    parts.retain(|t| !matches!(t, Type::Tags(_)));
    let combine = |a: Option<Type>, b: Option<Type>| match (a, b) {
        (Some(a), Some(b)) => {
            Some(if union { normalize(Type::Union(vec![a, b])) } else { normalize(Type::Inter(vec![a, b])) })
        }
        (a, b) => a.or(b),
    };
    let mut sets = sets.into_iter();
    let first = sets.next().unwrap();
    let merged = sets.fold(first, |mut acc, s| {
        if union {
            for (k, p) in s {
                let old = acc.remove(&k).flatten();
                acc.insert(k, combine(old, p));
            }
            acc
        } else {
            let mut out = BTreeMap::new();
            for (k, p) in s {
                if let Some(old) = acc.remove(&k) {
                    out.insert(k, combine(old, p));
                }
            }
            out
        }
    });
    parts.push(if merged.is_empty() { Type::Bot } else { Type::Tags(merged) });
}

/// In an intersection with variants, a record of only optional fields
/// asks of each variant that has the field that its payload fit: it
/// folds into the payloads.
fn optional_reads_into_variants(parts: &mut Vec<Type>) {
    let Some(at) = parts.iter().position(|t| matches!(t, Type::Tags(tags) if tags.values().any(|p| p.is_some())))
    else {
        return;
    };
    let mut i = 0;
    while i < parts.len() {
        let fields = match &parts[i] {
            Type::Record(fields) if fields.values().all(|t| matches!(t, Type::Opt(_))) => fields.clone(),
            _ => {
                i += 1;
                continue;
            }
        };
        if let Type::Tags(tags) = &mut parts[at] {
            for (k, t) in fields {
                if let (Some(Some(p)), Type::Opt(t)) = (tags.get_mut(&k), t) {
                    *p = normalize(Type::Inter(vec![p.clone(), *t]));
                }
            }
        }
        parts.remove(i);
    }
}

/// A variant beside a record is shown as the record of one field it is,
/// so the two merge as records do.
fn records_beside_records(parts: &mut [Type]) {
    if parts.iter().any(|t| matches!(t, Type::Record(_))) {
        for t in parts.iter_mut() {
            *t = as_record(t);
        }
    }
}

/// A single variant with a payload as the record of one field it is.
fn as_record(t: &Type) -> Type {
    match t {
        Type::Tags(tags) if tags.len() == 1 => match tags.iter().next() {
            Some((k, Some(p))) => Type::Record([(k.clone(), p.clone())].into_iter().collect()),
            _ => t.clone(),
        },
        _ => t.clone(),
    }
}

/// Capability sets merge like keyword sets, except that an empty
/// intersection is the pure set, not nothing.
fn merge_caps(parts: &mut Vec<Type>, union: bool) {
    // In a union, the pure set adds nothing beside anything else.
    if union && parts.len() > 1 {
        parts.retain(|t| !matches!(t, Type::Caps(c) if c.is_empty()));
    }
    let sets: Vec<BTreeSet<String>> =
        parts.iter().filter_map(|t| if let Type::Caps(c) = t { Some(c.clone()) } else { None }).collect();
    if sets.len() < 2 {
        return;
    }
    parts.retain(|t| !matches!(t, Type::Caps(_)));
    let mut sets = sets.into_iter();
    let first = sets.next().unwrap();
    let merged = sets.fold(first, |acc, s| {
        if union { acc.union(&s).cloned().collect() } else { acc.intersection(&s).cloned().collect() }
    });
    parts.push(Type::Caps(merged));
}

/// Lists and dictionaries are covariant: a union of lists is a list of
/// the union, and likewise for intersections.
fn merge_containers(parts: &mut Vec<Type>, union: bool) {
    let combine = |ts: Vec<Type>| if union { normalize(Type::Union(ts)) } else { normalize(Type::Inter(ts)) };
    let lists: Vec<Type> =
        parts.iter().filter_map(|t| if let Type::List(e) = t { Some((**e).clone()) } else { None }).collect();
    if lists.len() > 1 {
        parts.retain(|t| !matches!(t, Type::List(_)));
        parts.push(Type::List(Box::new(combine(lists))));
    }
    let dicts: Vec<Type> =
        parts.iter().filter_map(|t| if let Type::Dict(e) = t { Some((**e).clone()) } else { None }).collect();
    if dicts.len() > 1 {
        parts.retain(|t| !matches!(t, Type::Dict(_)));
        parts.push(Type::Dict(Box::new(combine(dicts))));
    }
    // Functions of the same parameters: what each takes, and what each
    // answers, combined the other way round for the parameters.
    let fns: Vec<Type> = parts.iter().filter(|t| matches!(t, Type::Fn(..))).cloned().collect();
    if fns.len() > 1 {
        let Type::Fn(first, _, _, labels) = &fns[0] else { unreachable!() };
        let same = fns.iter().all(|f| matches!(f, Type::Fn(p, _, _, l) if p.len() == first.len() && l == labels));
        if same {
            let labels = labels.clone();
            let flip = |ts: Vec<Type>| if union { normalize(Type::Inter(ts)) } else { normalize(Type::Union(ts)) };
            let mut params: Vec<Vec<Type>> = vec![Vec::new(); first.len()];
            let (mut rets, mut effs) = (Vec::new(), Vec::new());
            for f in fns {
                let Type::Fn(p, r, e, _) = f else { unreachable!() };
                p.into_iter().enumerate().for_each(|(i, p)| params[i].push(p));
                rets.push(*r);
                effs.push(*e);
            }
            parts.retain(|t| !matches!(t, Type::Fn(..)));
            parts.push(Type::Fn(
                params.into_iter().map(flip).collect(),
                Box::new(combine(rets)),
                Box::new(combine(effs)),
                labels,
            ));
        }
    }
    let tasks: Vec<(Type, Type)> = parts
        .iter()
        .filter_map(|t| if let Type::Task(r, c) = t { Some(((**r).clone(), (**c).clone())) } else { None })
        .collect();
    if tasks.len() > 1 {
        parts.retain(|t| !matches!(t, Type::Task(..)));
        let (results, caps): (Vec<Type>, Vec<Type>) = tasks.into_iter().unzip();
        parts.push(Type::Task(Box::new(combine(results)), Box::new(combine(caps))));
    }
}

/// In an intersection, records combine their fields; in a union, two
/// records keep only the fields they share.
fn merge_records(parts: &mut Vec<Type>, union: bool) {
    let mut merged: Option<BTreeMap<String, Type>> = None;
    parts.retain(|t| {
        let Type::Record(fields) = t else { return true };
        merged = Some(match merged.take() {
            None => fields.clone(),
            Some(mut acc) => {
                if union {
                    // A field one record lacks may be absent.
                    for (k, v) in fields {
                        if !acc.contains_key(k) {
                            acc.insert(k.clone(), Type::Opt(Box::new(split_opt(v.clone()).0)));
                        }
                    }
                    for (k, v) in acc.iter_mut() {
                        let (a, a_opt) = split_opt(v.clone());
                        let (b, b_opt) = match fields.get(k) {
                            Some(t) => split_opt(t.clone()),
                            None => (Type::Bot, true),
                        };
                        let joined = normalize(Type::Union(vec![a, b]));
                        *v = if a_opt || b_opt { Type::Opt(Box::new(joined)) } else { joined };
                    }
                } else {
                    for (k, v) in fields {
                        let combined = match acc.remove(k) {
                            Some(old) => {
                                // Required by either, it is required.
                                let (a, a_opt) = split_opt(old);
                                let (b, b_opt) = split_opt(v.clone());
                                let met = normalize(Type::Inter(vec![a, b]));
                                if a_opt && b_opt { Type::Opt(Box::new(met)) } else { met }
                            }
                            None => v.clone(),
                        };
                        acc.insert(k.clone(), combined);
                    }
                }
                acc
            }
        });
        false
    });
    if let Some(fields) = merged {
        parts.push(Type::Record(fields));
    }
}

/// A record with one field set, shown as the record when the original
/// is one.
fn update(record: Type, field: String, value: Type) -> Type {
    let absent = value == Type::Opt(Box::new(Type::Bot));
    match record {
        // `{}` is the record with no attributes.
        Type::Prim(Prim::Empty) => update(Type::Record(BTreeMap::new()), field, value),
        Type::Record(mut fields) if absent => {
            fields.remove(&field);
            Type::Record(fields)
        }
        Type::Record(mut fields) => {
            fields.insert(field, value);
            Type::Record(fields)
        }
        // Setting a field again sets it once.
        Type::Update(inner, k, _) if k == field => update(*inner, field, value),
        // A variant is the record of one field it is.
        Type::Tags(ref tags) if tags.len() == 1 && tags.values().all(|p| p.is_some()) => {
            update(as_record(&record), field, value)
        }
        // Setting a field of either record sets it in whichever it is.
        Type::Union(members) if members.iter().all(|m| matches!(m, Type::Record(_))) => {
            normalize(Type::Union(members.into_iter().map(|m| update(m, field.clone(), value.clone())).collect()))
        }
        other => Type::Update(Box::new(other), field, Box::new(value)),
    }
}

fn mentions(ty: &Type, v: u32) -> bool {
    match ty {
        Type::Var(x) => *x == v,
        Type::Fn(params, ret, eff, _) => params.iter().any(|p| mentions(p, v)) || mentions(ret, v) || mentions(eff, v),
        Type::Record(fields) => fields.values().any(|t| mentions(t, v)),
        Type::List(t) | Type::Dict(t) | Type::Opt(t) | Type::Rec(_, t) => mentions(t, v),
        Type::Task(t, c) => mentions(t, v) || mentions(c, v),
        Type::Tags(tags) => tags.values().flatten().any(|t| mentions(t, v)),
        Type::Update(r, _, x) => mentions(r, v) || mentions(x, v),
        Type::Union(ts) | Type::Inter(ts) => ts.iter().any(|t| mentions(t, v)),
        _ => false,
    }
}

fn rename(ty: Type) -> Type {
    fn go(ty: Type, map: &mut HashMap<u32, u32>) -> Type {
        let name = |v: u32, map: &mut HashMap<u32, u32>| {
            let n = map.len() as u32;
            *map.entry(v).or_insert(n)
        };
        match ty {
            Type::Var(v) => Type::Var(name(v, map)),
            Type::Rec(v, body) => {
                let v = name(v, map);
                Type::Rec(v, Box::new(go(*body, map)))
            }
            Type::Fn(params, ret, eff, labels) => {
                let params = params.into_iter().map(|p| go(p, map)).collect();
                let ret = go(*ret, map);
                Type::Fn(params, Box::new(ret), Box::new(go(*eff, map)), labels)
            }
            Type::Record(fields) => Type::Record(fields.into_iter().map(|(k, t)| (k, go(t, map))).collect()),
            Type::List(t) => Type::List(Box::new(go(*t, map))),
            Type::Task(t, c) => Type::Task(Box::new(go(*t, map)), Box::new(go(*c, map))),
            Type::Dict(t) => Type::Dict(Box::new(go(*t, map))),
            Type::Opt(t) => Type::Opt(Box::new(go(*t, map))),
            Type::Tags(tags) => Type::Tags(tags.into_iter().map(|(k, p)| (k, p.map(|p| go(p, map)))).collect()),
            Type::Update(r, k, v) => {
                let r = go(*r, map);
                Type::Update(Box::new(r), k, Box::new(go(*v, map)))
            }
            Type::Union(ts) => Type::Union(ts.into_iter().map(|t| go(t, map)).collect()),
            Type::Inter(ts) => Type::Inter(ts.into_iter().map(|t| go(t, map)).collect()),
            other => other,
        }
    }
    go(ty, &mut HashMap::new())
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_type(self, f, 0)
    }
}

/// Precedence: 0 for a function arrow, 1 for a union, 2 for an
/// intersection, 3 for an atom.
/// A type as a `type` or `claim` writes it, so that what is shown can be
/// written back: `[:a :b]`, `(list ?a)`, `((\\ integer with: text) : text)`.
/// A record that is a variable as well is written as an open record,
/// `{todo-id: text ?a ...}`: the record `?a`, which has `todo-id`.
fn write_type(ty: &Type, f: &mut fmt::Formatter<'_>, prec: u8) -> fmt::Result {
    let _ = prec;
    let var = |v: u32| -> String {
        let letter = (b'a' + (v % 26) as u8) as char;
        if v < 26 { format!("?{letter}") } else { format!("?{letter}{}", v / 26) }
    };
    let fields = |f: &mut fmt::Formatter<'_>, fields: &BTreeMap<String, Type>| -> fmt::Result {
        for (i, (k, t)) in fields.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            match t {
                Type::Opt(_) => write!(f, "{k}?: ")?,
                _ => write!(f, "{k}: ")?,
            }
            write_type(split_ref(t), f, 0)?;
        }
        Ok(())
    };
    match ty {
        Type::Top => write!(f, "any"),
        Type::Bot => write!(f, "nothing"),
        Type::Prim(p) => write!(f, "{}", p.name()),
        Type::View => write!(f, "view"),
        Type::Effect => write!(f, "effect"),
        Type::Caps(c) => write!(f, "{}", caps_text(c)),
        Type::Tags(tags) if tags.len() > 1 => {
            write!(f, "[")?;
            write_tags(tags, f)?;
            write!(f, "]")
        }
        Type::Tags(tags) => write_tags(tags, f),
        Type::Var(v) => write!(f, "{}", var(*v)),
        Type::Named(name) => write!(f, "?{name}"),
        Type::Fn(params, ret, eff, labels) => {
            write!(f, "((\\")?;
            for (i, p) in params.iter().enumerate() {
                write!(f, " ")?;
                if i > 0
                    && let Some(label) = labels.get(i - 1)
                {
                    write!(f, "{label}: ")?;
                }
                write_type(p, f, 0)?;
            }
            write!(f, ")")?;
            let pure = match &**eff {
                Type::Bot => true,
                Type::Caps(c) => c.is_empty(),
                _ => false,
            };
            if !pure {
                write!(f, " with: ")?;
                write_type(eff, f, 0)?;
            }
            write!(f, " : ")?;
            write_type(ret, f, 0)?;
            write!(f, ")")
        }
        Type::Record(fs) => {
            write!(f, "{{")?;
            fields(f, fs)?;
            write!(f, "}}")
        }
        Type::List(t) => {
            write!(f, "(list ")?;
            write_type(t, f, 0)?;
            write!(f, ")")
        }
        Type::Dict(t) => {
            write!(f, "(dict ")?;
            write_type(t, f, 0)?;
            write!(f, ")")
        }
        // As a claim writes it.
        Type::Task(t, c) => {
            write!(f, "(task ")?;
            write_type(t, f, 0)?;
            let pure = matches!(&**c, Type::Caps(c) if c.is_empty()) || matches!(**c, Type::Bot);
            if !pure {
                write!(f, " with: ")?;
                write_type(c, f, 0)?;
            }
            write!(f, ")")
        }
        Type::Union(ts) => {
            write!(f, "[")?;
            for (i, t) in ts.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                match t {
                    // A variant set is already a union.
                    Type::Tags(tags) => write_tags(tags, f)?,
                    _ => write_type(t, f, 0)?,
                }
            }
            write!(f, "]")
        }
        // A variable that is a record as well: the record, open.
        Type::Inter(ts)
            if ts.len() == 2
                && let Some(Type::Var(v)) = ts.iter().find(|t| matches!(t, Type::Var(_)))
                && let Some(Type::Record(fs)) =
                    ts.iter().map(as_record).find(|t| matches!(t, Type::Record(_))).as_ref() =>
        {
            write!(f, "{{")?;
            fields(f, fs)?;
            if !fs.is_empty() {
                write!(f, " ")?;
            }
            write!(f, "{} ...}}", var(*v))
        }
        Type::Inter(ts) => {
            write!(f, "(&")?;
            for t in ts {
                write!(f, " ")?;
                write_type(t, f, 0)?;
            }
            write!(f, ")")
        }
        Type::Rec(v, body) => {
            write!(f, "(rec {} ", var(*v))?;
            write_type(body, f, 0)?;
            write!(f, ")")
        }
        Type::Update(..) => {
            // Updates of an update are one: `(?a with x: integer, y: boolean)`.
            let mut steps = Vec::new();
            let mut base = ty;
            while let Type::Update(r, k, v) = base {
                steps.push((k, v));
                base = r;
            }
            steps.reverse();
            write!(f, "(")?;
            write_type(base, f, 0)?;
            let absent = Type::Opt(Box::new(Type::Bot));
            let (gone, set): (Vec<_>, Vec<_>) = steps.into_iter().partition(|(_, v)| ***v == absent);
            if !set.is_empty() {
                write!(f, " with ")?;
                for (i, (k, v)) in set.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: ")?;
                    write_type(v, f, 0)?;
                }
            }
            if !gone.is_empty() {
                write!(f, " without {}", gone.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "))?;
            }
            write!(f, ")")
        }
        Type::Opt(t) => write_type(t, f, prec),
    }
}

/// A field's displayed type, without whether it may be absent.
fn split_ref(t: &Type) -> &Type {
    match t {
        Type::Opt(inner) => inner,
        t => t,
    }
}

/// Variants as a union: a keyword as itself, a variant as the record of
/// one field it is.
/// A variant's key that starts with this is shown as its constructor:
/// `(ok Int)`, not `{ok: Int}`. A codebase renames the keys of record
/// types it claims with one attribute so (see [`Type::rename_keys`]).
pub const CONSTRUCTOR: char = '\u{1}';

fn write_tags(tags: &BTreeMap<String, Option<Type>>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, (k, p)) in tags.iter().enumerate() {
        if i > 0 {
            write!(f, " ")?;
        }
        match (p, k.strip_prefix(CONSTRUCTOR)) {
            (None, _) => write!(f, ":{}", k.trim_start_matches(CONSTRUCTOR))?,
            (Some(p), Some(name)) => {
                write!(f, "({name} ")?;
                write_type(p, f, 0)?;
                write!(f, ")")?;
            }
            (Some(p), None) => {
                write!(f, "{{{k}: ")?;
                write_type(p, f, 0)?;
                write!(f, "}}")?;
            }
        }
    }
    Ok(())
}

/// A field's type, whether or not it may be absent.
fn unopt(t: &Ty) -> &Ty {
    match t {
        Ty::Opt(inner) => inner,
        t => t,
    }
}

/// A field's displayed type, and whether it may be absent.
fn split_opt(t: Type) -> (Type, bool) {
    match t {
        Type::Opt(inner) => (*inner, true),
        t => (t, false),
    }
}
