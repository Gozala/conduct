//! Typing rules: the type of every term, with no annotations.

use crate::term::{Hash, Op, Pattern, Term};
use crate::types::{Scheme, Solver, Ty, TypeError};
use std::rc::Rc;

pub enum Binding {
    Mono(Ty),
    Poly(Scheme),
}

/// What a term can refer to outside itself.
pub trait Globals {
    /// The type scheme of another definition.
    fn scheme(&self, hash: &Hash) -> Option<Scheme>;
    /// The value type declared for an attribute, by its global name.
    fn attribute(&self, global: &str) -> Option<Ty>;
    /// The host functions that need a capability: each one's name and
    /// type.
    fn operations(&self, capability: &str) -> Vec<(String, Scheme)>;
    /// A method's claim, by its whole name: the type every definition of
    /// it fits, with the type it is called on as its one requirement.
    fn method(&self, name: &str) -> Option<Scheme>;
}

/// A place in a definition where checking found what calls need: a
/// method called, a definition that asks for methods, or a member of the
/// definition's own cycle.
pub struct Site {
    pub id: u32,
    pub kind: SiteKind,
    /// Each method needed there, with the type it is called on.
    pub needs: Vec<(String, Ty)>,
    /// The level of the code the site is in: above the definition's own,
    /// it is in a function a `let` made generic.
    pub level: u32,
}

pub enum SiteKind {
    Method,
    Definition(Hash),
    Cycle(u32),
}

pub struct Infer<'a> {
    pub solver: &'a Solver,
    pub globals: &'a dyn Globals,
    /// The types of the members of the cycle being checked.
    pub cycle: Vec<Ty>,
    /// What each body being checked may do, innermost last: `None` for a
    /// function's body or a top-level value, which are pure, and the
    /// capabilities of a task's body, which may perform.
    pub effects: std::cell::RefCell<Vec<Option<Ty>>>,
    /// What each [`Term::Site`] was found to need, in the order met.
    pub sites: std::cell::RefCell<Vec<Site>>,
}

type Result<T> = std::result::Result<T, TypeError>;

impl Infer<'_> {
    pub fn term(&self, term: &Term, env: &mut Vec<Binding>, level: u32) -> Result<Ty> {
        let s = self.solver;
        Ok(match term {
            // A number that is not negative is a natural, which flows
            // wherever an integer does.
            Term::Int(n) if (0..256).contains(n) => Ty::byte(),
            Term::Int(n) if *n >= 0 => Ty::nat(),
            Term::Int(_) => Ty::int(),
            Term::Str(_) => Ty::str(),
            Term::Bool(_) => Ty::bool(),
            // Only what an exhaustive `match` cannot reach.
            Term::Nil => Ty::Bot,
            Term::Local(i) => match &env[env.len() - 1 - *i as usize] {
                Binding::Mono(t) => t.clone(),
                Binding::Poly(scheme) => s.instantiate(scheme, level),
            },
            Term::Ref(h) => {
                let scheme =
                    self.globals.scheme(h).ok_or_else(|| TypeError(format!("unknown definition {}", h.short())))?;
                if !scheme.requires.is_empty() {
                    return Err(TypeError(format!("{} asks for methods, met where none were looked for", h.short())));
                }
                s.instantiate(&scheme, level)
            }
            Term::CycleRef(i) => self.cycle[*i as usize].clone(),
            Term::Site(id, inner) => {
                let (ty, kind, needs) = match &**inner {
                    Term::Dispatch(name) => {
                        let scheme =
                            self.globals.method(name).ok_or_else(|| TypeError(format!("`{name}` is not a method")))?;
                        let (ty, receivers) = s.instantiate_with(&scheme, level);
                        (ty, SiteKind::Method, vec![(name.clone(), receivers[0].clone())])
                    }
                    Term::Ref(h) => {
                        let scheme = self
                            .globals
                            .scheme(h)
                            .ok_or_else(|| TypeError(format!("unknown definition {}", h.short())))?;
                        let (ty, receivers) = s.instantiate_with(&scheme, level);
                        let needs = scheme.requires.iter().map(|r| r.method.clone()).zip(receivers).collect();
                        (ty, SiteKind::Definition(*h), needs)
                    }
                    Term::CycleRef(i) => (self.cycle[*i as usize].clone(), SiteKind::Cycle(*i), Vec::new()),
                    other => return self.term(other, env, level),
                };
                self.sites.borrow_mut().push(Site { id: *id, kind, needs, level });
                ty
            }
            Term::Dispatch(name) => {
                return Err(TypeError(format!("`{name}` is a method, met where none was looked for")));
            }
            Term::Fn(labels, body) => {
                let arity = labels.len();
                let params: Vec<Ty> = (0..arity).map(|_| s.fresh_ty(level)).collect();
                env.extend(params.iter().cloned().map(Binding::Mono));
                // Calls are pure: a function's body performs nothing, even
                // inside a task's.
                self.effects.borrow_mut().push(None);
                let ret = self.term(body, env, level);
                self.effects.borrow_mut().pop();
                env.truncate(env.len() - arity);
                Ty::func_eff(params, ret?, Ty::pure(), labels[1.min(arity)..].to_vec())
            }
            Term::App(callee, args, labels) => {
                let f = self.term(callee, env, level)?;
                let args = args.iter().map(|a| self.term(a, env, level)).collect::<Result<Vec<_>>>()?;
                let ret = s.fresh_ty(level);
                s.constrain(&f, &Ty::func_eff(args, ret.clone(), Ty::pure(), labels.clone()))?;
                ret
            }
            Term::Task(body) => {
                let caps = s.fresh_ty(level);
                self.effects.borrow_mut().push(Some(caps.clone()));
                let result = self.term(body, env, level);
                self.effects.borrow_mut().pop();
                Ty::task(result?, caps)
            }
            Term::Perform(task) => {
                let t = self.term(task, env, level)?;
                let (result, caps) = (s.fresh_ty(level), s.fresh_ty(level));
                s.constrain(&t, &Ty::task(result.clone(), caps.clone()))
                    .map_err(|e| TypeError(format!("`!` performs a task: {e}")))?;
                let body = self.effects.borrow().last().cloned().flatten();
                match body {
                    Some(body) => s.constrain(&caps, &body)?,
                    None => {
                        return Err(TypeError(
                            "`!` performs, and only a task's body may: mark the definition, `(define (name! ..) ..)`, \
                             or the function, `(fn! [..] ..)`"
                                .into(),
                        ));
                    }
                }
                result
            }
            Term::Handle(task, provided) => {
                let t = self.term(task, env, level)?;
                let (result, caps) = (s.fresh_ty(level), s.fresh_ty(level));
                s.constrain(&t, &Ty::task(result.clone(), caps.clone()))
                    .map_err(|e| TypeError(format!("`perform` takes a task: {e}")))?;
                let mut names = std::collections::BTreeSet::new();
                for (cap, ops) in provided {
                    let wanted = self.globals.operations(cap);
                    if wanted.is_empty() {
                        return Err(TypeError(format!("no host function uses the capability `{cap}`")));
                    }
                    for (op, _) in &wanted {
                        if !ops.iter().any(|(o, _)| o == op) {
                            return Err(TypeError(format!("an environment that provides `{cap}` implements `{op}`")));
                        }
                    }
                    for (op, implementation) in ops {
                        let Some((_, scheme)) = wanted.iter().find(|(o, _)| o == op) else {
                            return Err(TypeError(format!("`{cap}` has no operation `{op}`")));
                        };
                        // An implementation answers what performing the
                        // operation would.
                        // An operation of no parameters is a task, and is
                        // implemented by its answer.
                        let expected = match s.instantiate(scheme, level) {
                            Ty::Fn(params, ret, _, labels) => {
                                let answer = match &*ret {
                                    Ty::Task(answer, _) => (**answer).clone(),
                                    other => other.clone(),
                                };
                                Ty::func_eff((*params).clone(), answer, Ty::pure(), (*labels).clone())
                            }
                            Ty::Task(answer, _) => (*answer).clone(),
                            other => other,
                        };
                        // An implementation is pure: it answers, and the
                        // task goes on.
                        self.effects.borrow_mut().push(None);
                        let i = self.term(implementation, env, level);
                        self.effects.borrow_mut().pop();
                        let i = i?;
                        s.constrain(&i, &expected)
                            .map_err(|e| TypeError(format!("the implementation of `{op}`: {e}")))?;
                    }
                    names.insert(cap.clone());
                }
                let rest = s.fresh_ty(level);
                s.constrain(&caps, &Ty::CapsPlus(std::rc::Rc::new(rest.clone()), std::rc::Rc::new(names)))?;
                Ty::task(result, rest)
            }
            Term::Let(value, body) => {
                let t = self.term(value, env, level + 1)?;
                env.push(Binding::Poly(Scheme::new(level, t)));
                let out = self.term(body, env, level);
                env.pop();
                out?
            }
            Term::If(c, t, e) => {
                let c = self.term(c, env, level)?;
                s.constrain(&c, &Ty::bool())
                    .map_err(|e| TypeError(format!("the test of an `if` must be a Bool: {e}")))?;
                self.join([self.term(t, env, level)?, self.term(e, env, level)?], level)?
            }
            Term::And(a, b) | Term::Or(a, b) => {
                for part in [a, b] {
                    let t = self.term(part, env, level)?;
                    s.constrain(&t, &Ty::bool()).map_err(|e| TypeError(format!("`and` and `or` take Bools: {e}")))?;
                }
                Ty::bool()
            }
            // A record of one field is a variant, which reads as a record
            // but joins with other variants without losing its field.
            Term::Record(fields) if fields.len() == 1 => {
                Ty::variant(fields[0].0.clone(), self.attribute(&fields[0].0, &fields[0].1, env, level)?)
            }
            Term::Record(fields) => Ty::record(
                fields
                    .iter()
                    .map(|(k, t)| Ok((k.clone(), self.attribute(k, t, env, level)?)))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Term::Field(name, record) => {
                let r = self.term(record, env, level)?;
                let out = s.fresh_ty(level);
                s.constrain(&r, &Ty::record([(name.clone(), out.clone())]))?;
                out
            }
            Term::Update(record, name, value) => {
                let r = self.term(record, env, level)?;
                // Only a record has attributes to set.
                s.constrain(&r, &Ty::record([]))
                    .map_err(|e| TypeError(format!("`assoc` sets an attribute of a record: {e}")))?;
                let v = self.attribute(name, value, env, level)?;
                Ty::update(r, name.clone(), v)
            }
            Term::Dict(entries) if entries.is_empty() => Ty::Prim(crate::types::Prim::Empty),
            Term::Dict(entries) => {
                let values = entries.iter().map(|(_, t)| self.term(t, env, level)).collect::<Result<Vec<_>>>()?;
                Ty::dict(self.join(values, level)?)
            }
            Term::List(items) => {
                let items = items.iter().map(|t| self.term(t, env, level)).collect::<Result<Vec<_>>>()?;
                Ty::list(self.join(items, level)?)
            }
            Term::Keyword(k) => Ty::keys([k.clone()]),
            // A host definition's type is declared, never inferred.
            Term::Host(name, _) => return Err(TypeError(format!("`{name}` is provided by the host"))),
            Term::Match(scrutinee, clauses, default) => {
                let t = self.term(scrutinee, env, level)?;
                if default.is_none() {
                    // No `_` clause: the value must be one the patterns
                    // cover, so the match is exhaustive.
                    let patterns: Vec<&Pattern> = clauses.iter().map(|(p, _)| p).collect();
                    if let Some(shape) = cover_type(&patterns) {
                        s.constrain(&t, &shape).map_err(|e| {
                            TypeError(format!("a `match` without a `_` clause must cover every value it is given: {e}"))
                        })?;
                    }
                }
                // The local the value was read from, and the fields it
                // was read through, outermost last.
                let mut path = Vec::new();
                let mut at = &**scrutinee;
                while let Term::Field(name, inner) = at {
                    path.push(name.clone());
                    at = inner;
                }
                path.reverse();
                let local = if let Term::Local(i) = at { Some(env.len() - 1 - *i as usize) } else { None };
                let mut results = Vec::new();
                for (pattern, result) in clauses {
                    // Each pattern narrows what it tested: in its branch,
                    // the local has the matched shape.
                    let (narrowed, binders) = match local {
                        Some(slot) => {
                            let base = match &env[slot] {
                                Binding::Mono(t) => t.clone(),
                                Binding::Poly(scheme) => s.instantiate(scheme, level),
                            };
                            let (shape, binders) = self.narrow(base, &path, pattern, level)?;
                            let bound = s.fresh_ty(level);
                            s.constrain(&shape, &bound)?;
                            (Some((slot, std::mem::replace(&mut env[slot], Binding::Mono(bound)))), binders)
                        }
                        None => (None, self.narrow(t.clone(), &[], pattern, level)?.1),
                    };
                    // The pattern's names, the last innermost.
                    let n = binders.len();
                    env.extend(binders.into_iter().map(Binding::Mono));
                    let r = self.term(result, env, level);
                    env.truncate(env.len() - n);
                    if let Some((slot, before)) = narrowed {
                        env[slot] = before;
                    }
                    results.push(r?);
                }
                if let Some(d) = default {
                    results.push(self.term(d, env, level)?);
                }
                self.join(results, level)?
            }
            Term::Prim(op, args) => {
                let args = args.iter().map(|a| self.term(a, env, level)).collect::<Result<Vec<_>>>()?;
                let (params, ret, effect) = signature(s, *op, level);
                for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                    s.constrain(arg, param)
                        .map_err(|e| TypeError(format!("argument {} of `{}`: {e}", i + 1, op.name())))?;
                }
                let _ = effect;
                ret
            }
        })
    }

    /// `t` as it is known to be where `pattern` matched it, read through
    /// the fields of `path`: each field read is narrowed in place, as a
    /// record update narrows one field.
    fn narrow(&self, t: Ty, path: &[String], pattern: &Pattern, level: u32) -> Result<(Ty, Vec<Ty>)> {
        let s = self.solver;
        if let Some((name, rest)) = path.split_first() {
            let field = s.fresh_ty(level);
            s.constrain(&t, &Ty::record([(name.clone(), field.clone())]))?;
            let (inner, binders) = self.narrow(field, rest, pattern, level)?;
            return Ok((Ty::matched(t, name.clone(), inner), binders));
        }
        Ok(match pattern {
            Pattern::Keyword(k) => (Ty::keys([k.clone()]), vec![]),
            Pattern::Str(_) => (Ty::str(), vec![]),
            Pattern::Int(_) => (Ty::int(), vec![]),
            Pattern::Bool(_) => (Ty::bool(), vec![]),
            Pattern::Any => (t, vec![]),
            Pattern::Bind(_) => (t.clone(), vec![t]),
            // The whole value, as the pattern it is bound beside narrows
            // it.
            Pattern::Rest(_, p) => {
                let (narrowed, mut binders) = self.narrow(t, &[], p, level)?;
                let named: Vec<String> = match p.core() {
                    Pattern::Fields(fields) => fields.iter().map(|(k, _)| k.clone()).collect(),
                    _ => vec![],
                };
                let rest = named.into_iter().fold(narrowed.clone(), Ty::without);
                binders.insert(0, rest);
                (narrowed, binders)
            }
            Pattern::List(items, rest) => {
                // Every item is one type, so a list's patterns narrow
                // what is bound, not the list.
                let item = s.fresh_ty(level);
                s.constrain(&t, &Ty::list(item.clone()))?;
                let mut binders = Vec::new();
                for p in items {
                    binders.extend(self.narrow(item.clone(), &[], p, level)?.1);
                }
                if let Some(r) = rest {
                    binders.extend(self.narrow(Ty::list(item), &[], r, level)?.1);
                }
                (t, binders)
            }
            Pattern::Fields(fields) => {
                // A field is read only where the value has it, so a
                // variant pattern asks nothing of the other variants.
                let mut out = t.clone();
                let mut binders = Vec::new();
                for (name, p) in fields {
                    let field = s.fresh_ty(level);
                    s.constrain(&t, &Ty::if_has(name.clone(), field.clone()))?;
                    let (inner, bound) = self.narrow(field, &[], p, level)?;
                    out = Ty::matched(out, name.clone(), inner);
                    binders.extend(bound);
                }
                (out, binders)
            }
        })
    }

    /// The type of `value`, set as the attribute `global`: it must fit
    /// the attribute's declared type.
    fn attribute(&self, global: &str, value: &Term, env: &mut Vec<Binding>, level: u32) -> Result<Ty> {
        let t = self.term(value, env, level)?;
        if let Some(claimed) = self.globals.attribute(global) {
            self.solver
                .constrain(&t, &claimed)
                .map_err(|e| TypeError(format!("the attribute `{global}` holds what its type says: {e}")))?;
        }
        Ok(t)
    }

    /// A variable that every one of `types` flows into.
    fn join(&self, types: impl IntoIterator<Item = Ty>, level: u32) -> Result<Ty> {
        let out = self.solver.fresh_ty(level);
        for t in types {
            self.solver.constrain(&t, &out)?;
        }
        Ok(out)
    }
}

/// The parameter types and result type of a built-in, with fresh
/// variables for its generic parts.
pub fn signature(s: &Solver, op: Op, level: u32) -> (Vec<Ty>, Ty, Ty) {
    let (params, ret) = pure_signature(s, op, level);
    (params, ret, Ty::pure())
}

fn pure_signature(s: &Solver, op: Op, level: u32) -> (Vec<Ty>, Ty) {
    let var = || s.fresh_ty(level);
    let int2 = || vec![Ty::int(), Ty::int()];
    match op {
        // A sum or product of naturals is a natural, and of integers an
        // integer: each argument flows into one variable bounded by
        // integer, and the result is that or a natural, since a sum of
        // bytes need not be a byte. A quotient is no larger than what it
        // divides. A difference may be negative, and a remainder has the
        // sign of the dividend.
        Op::Add | Op::Mul => {
            let a = var();
            let bounded = Ty::Inter(Rc::new(vec![a.clone(), Ty::int()]));
            (vec![bounded.clone(), bounded], Ty::Union(Rc::new(vec![a, Ty::nat()])))
        }
        Op::Quot => {
            let a = var();
            let bounded = Ty::Inter(Rc::new(vec![a.clone(), Ty::int()]));
            (vec![bounded.clone(), bounded], a)
        }
        Op::Sub => (int2(), Ty::int()),
        Op::Rem => {
            let a = var();
            (vec![Ty::Inter(Rc::new(vec![a.clone(), Ty::int()])), Ty::int()], a)
        }
        Op::Lt | Op::Gt | Op::Le | Op::Ge => (int2(), Ty::bool()),
        Op::Eq | Op::NotEq => (vec![Ty::Top, Ty::Top], Ty::bool()),
        Op::Not => (vec![Ty::bool()], Ty::bool()),
        Op::Inspect | Op::Show => (vec![Ty::Top], Ty::str()),
        Op::Pairwise => (vec![Ty::Top, Ty::list(Ty::Top), Ty::list(Ty::Top)], Ty::bool()),
        Op::Count => (vec![Ty::list(Ty::Top)], Ty::nat()),
        Op::BitAnd | Op::BitOr | Op::BitXor | Op::ShiftLeft | Op::ShiftRight => (vec![Ty::nat(), Ty::nat()], Ty::nat()),
        Op::CountOnes => (vec![Ty::nat()], Ty::nat()),
        Op::Hash => (vec![Ty::Top], Ty::nat()),
        Op::SetAt | Op::InsertAt => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::nat(), a.clone()], Ty::list(a))
        }
        Op::Natural => (vec![Ty::int(), Ty::nat()], Ty::nat()),
        Op::Compare => (vec![Ty::Top, Ty::Top], Ty::int()),
        Op::ByteCount => (vec![Ty::str()], Ty::nat()),
        // What may be missing answers its default, of any type.
        Op::ByteAt => {
            let d = var();
            (vec![Ty::str(), Ty::nat(), d.clone()], Ty::Union(Rc::new(vec![Ty::byte(), d])))
        }
        Op::CodePointAt => {
            let d = var();
            (vec![Ty::str(), Ty::nat(), d.clone()], Ty::Union(Rc::new(vec![Ty::nat(), d])))
        }
        // Text or a list, and what comes back is the same.
        Op::Slice => {
            let a = var();
            let either = Ty::Union(Rc::new(vec![Ty::str(), Ty::list(Ty::Top)]));
            (vec![Ty::Inter(Rc::new(vec![a.clone(), either])), Ty::nat(), Ty::nat()], a)
        }
        Op::Utf8 => (vec![Ty::str()], Ty::list(Ty::byte())),
        Op::FromUtf8 => {
            let d = var();
            (vec![Ty::list(Ty::byte()), d.clone()], Ty::Union(Rc::new(vec![Ty::str(), d])))
        }
        Op::Byte => {
            let d = var();
            (vec![Ty::int(), d.clone()], Ty::Union(Rc::new(vec![Ty::byte(), d])))
        }
        Op::Occurs => (vec![Ty::str(), Ty::str(), Ty::nat()], Ty::bool()),
        Op::IndexOf => {
            let d = var();
            (vec![Ty::str(), Ty::str(), Ty::nat(), d.clone()], Ty::Union(Rc::new(vec![Ty::nat(), d])))
        }
        Op::FromCodePoint => (vec![Ty::nat()], Ty::str()),
        Op::Join => (vec![Ty::list(Ty::str()), Ty::str()], Ty::str()),
        // The default comes back for an index past the end.
        Op::Item => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::nat(), a.clone()], a)
        }
        Op::RemoveAt => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::nat()], Ty::list(a))
        }
        // The default comes back for an empty list.
        Op::First => {
            let a = var();
            (vec![Ty::list(a.clone()), a.clone()], a)
        }
        Op::Rest => {
            let a = var();
            (vec![Ty::list(a.clone())], Ty::list(a))
        }
        Op::Concat => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::list(a.clone())], Ty::list(a))
        }
        Op::Conj => {
            let a = var();
            (vec![Ty::list(a.clone()), a.clone()], Ty::list(a))
        }
        Op::Map => {
            let (a, b) = (var(), var());
            (vec![Ty::func(vec![a.clone()], b.clone(), &[]), Ty::list(a)], Ty::list(b))
        }
        Op::Filter => {
            let a = var();
            (vec![Ty::func(vec![a.clone()], Ty::bool(), &[]), Ty::list(a.clone())], Ty::list(a))
        }
        // The default comes back for a missing key.
        Op::Get => {
            let v = var();
            (vec![Ty::dict(v.clone()), keyword(), v.clone()], v)
        }
        Op::Assoc => {
            let v = var();
            (vec![Ty::dict(v.clone()), keyword(), v.clone()], Ty::dict(v))
        }
        Op::Dissoc => {
            let v = var();
            (vec![Ty::dict(v.clone()), keyword()], Ty::dict(v))
        }
        Op::H => (vec![Ty::str(), Ty::dict(Ty::str()), Ty::list(Ty::Top), Ty::dict(keyword())], Ty::View),
        Op::Emit => (vec![Ty::str(), Ty::str()], Ty::Effect),
        Op::Trim => (vec![Ty::str()], Ty::str()),
        Op::Fetch => (vec![Ty::str(), Ty::str(), Ty::str(), keyword()], Ty::Effect),
        Op::Split => (vec![Ty::str(), Ty::str()], Ty::list(Ty::str())),
        Op::ParseJson => (vec![Ty::str()], json()),
        Op::ToJson => (vec![Ty::Top], Ty::str()),
        Op::JsonGet => (vec![json(), Ty::str()], json()),
        Op::JsonItems => (vec![json()], Ty::list(json())),
        Op::JsonStr => (vec![json()], Ty::str()),
        Op::JsonInt => (vec![json()], Ty::int()),
        Op::JsonBool => (vec![json()], Ty::bool()),
        Op::JsonType => (vec![json()], Ty::str()),
        Op::HasField => (vec![Ty::Top, Ty::str()], Ty::bool()),
        Op::Without => (vec![Ty::Top, Ty::str()], Ty::Top),
        Op::Nth => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::int()], a)
        }
        Op::Drop => {
            let a = var();
            (vec![Ty::list(a.clone()), Ty::int()], Ty::list(a))
        }
        Op::WithEnv => (vec![Ty::Top, Ty::Top], Ty::Top),
    }
}

fn keyword() -> Ty {
    Ty::Prim(crate::types::Prim::Keyword)
}

fn json() -> Ty {
    Ty::Prim(crate::types::Prim::Json)
}

/// The type a value must have for patterns that cover it to cover it:
/// the variants named, each with what its payload patterns cover, or a
/// record with the fields a pattern of `_`s names.
fn cover_type(patterns: &[&Pattern]) -> Option<Ty> {
    let patterns: &[&Pattern] = &patterns.iter().map(|p| p.core()).collect::<Vec<_>>();
    // A function's clauses: each argument holds only what its column's
    // patterns name, which is what deciding they cover assumed.
    if let Some(keys) = crate::term::argument_columns(patterns) {
        let column = |k: &String| -> Vec<&Pattern> {
            patterns
                .iter()
                .filter_map(|p| match p {
                    Pattern::Fields(f) => f.iter().find(|(l, _)| l == k).map(|(_, p)| p),
                    _ => None,
                })
                .collect()
        };
        return Some(Ty::record(keys.iter().map(|k| (k.clone(), column_type(&column(k))))));
    }
    // An irrefutable clause covers by itself; the fields it names are
    // required by the reads that test them.
    if patterns.iter().any(|p| p.irrefutable()) {
        return None;
    }
    if patterns.iter().all(|p| matches!(p, Pattern::List(..))) {
        return Some(Ty::list(Ty::Top));
    }
    if patterns.iter().all(|p| matches!(p, Pattern::Bool(_))) {
        return Some(Ty::bool());
    }
    let Some(variants) = patterns.iter().map(|p| p.variant()).collect::<Option<Vec<_>>>() else {
        // A record pattern of `_`s: the value must have its fields. Of
        // several, the one that asks least, which the others narrow.
        let Some(Pattern::Fields(fields)) = patterns
            .iter()
            .filter(|p| p.fields_only())
            .min_by_key(|p| if let Pattern::Fields(f) = p { f.len() } else { 0 })
        else {
            return None;
        };
        return Some(Ty::record(fields.iter().map(|(k, _)| (k.clone(), Ty::Top))));
    };
    let payload = |k: &str| -> Option<Ty> {
        let inner: Vec<&Pattern> = variants.iter().filter(|v| v.0 == k).filter_map(|v| v.1).collect();
        if inner.is_empty() { None } else { Some(cover_type(&inner).unwrap_or(Ty::Top)) }
    };
    // One field named in every clause: a record's field, which a variant
    // with that key also has.
    if let [(k, Some(_)), rest @ ..] = variants.as_slice()
        && rest.iter().all(|v| v.0 == *k && v.1.is_some())
    {
        return Some(Ty::record([(k.to_string(), payload(k).unwrap_or(Ty::Top))]));
    }
    Some(Ty::tags(variants.iter().map(|v| (v.0.to_string(), payload(v.0)))))
}

/// What a column of a function's clauses lets its argument be: what
/// deciding the clauses cover assumed (see [`crate::term::Sig`]).
fn column_type(patterns: &[&Pattern]) -> Ty {
    fn ty(sig: &crate::term::Sig) -> Ty {
        match sig {
            crate::term::Sig::Open | crate::term::Sig::Fields(_) => Ty::Top,
            crate::term::Sig::Bool => Ty::bool(),
            // One variant with a payload is as much a record's field.
            crate::term::Sig::Tags(tags) if tags.len() == 1 && tags.values().all(Option::is_some) => {
                let (k, p) = tags.iter().next().unwrap();
                Ty::record([(k.clone(), p.as_ref().map(ty).unwrap())])
            }
            crate::term::Sig::Tags(tags) => Ty::tags(tags.iter().map(|(k, p)| (k.clone(), p.as_ref().map(ty)))),
        }
    }
    ty(&crate::term::Sig::of(patterns))
}
