//! Typing rules: the type of every term, with no annotations.

use crate::term::{Hash, Op, Pattern, Term};
use crate::types::{Scheme, Solver, Ty, TypeError};

pub enum Binding {
    Mono(Ty),
    Poly(Scheme),
}

/// What a term can refer to outside itself.
pub trait Globals {
    /// The type scheme of another definition.
    fn scheme(&self, hash: &Hash) -> Option<Scheme>;
}

pub struct Infer<'a> {
    pub solver: &'a Solver,
    pub globals: &'a dyn Globals,
    /// The types of the members of the cycle being checked.
    pub cycle: Vec<Ty>,
    /// The effect of each function body being checked, innermost last:
    /// every call adds the callee's effect to the innermost.
    pub effects: std::cell::RefCell<Vec<Ty>>,
}

type Result<T> = std::result::Result<T, TypeError>;

impl Infer<'_> {
    pub fn term(&self, term: &Term, env: &mut Vec<Binding>, level: u32) -> Result<Ty> {
        let s = self.solver;
        Ok(match term {
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
                s.instantiate(&scheme, level)
            }
            Term::CycleRef(i) => self.cycle[*i as usize].clone(),
            Term::Fn(arity, body) => {
                let params: Vec<Ty> = (0..*arity).map(|_| s.fresh_ty(level)).collect();
                env.extend(params.iter().cloned().map(Binding::Mono));
                // The body's effect is the function's, not its creator's.
                let effect = s.fresh_ty(level);
                self.effects.borrow_mut().push(effect.clone());
                let ret = self.term(body, env, level);
                self.effects.borrow_mut().pop();
                env.truncate(env.len() - *arity as usize);
                Ty::func_eff(params, ret?, effect)
            }
            Term::App(callee, args) => {
                let f = self.term(callee, env, level)?;
                let args = args.iter().map(|a| self.term(a, env, level)).collect::<Result<Vec<_>>>()?;
                let ret = s.fresh_ty(level);
                let effect = s.fresh_ty(level);
                s.constrain(&f, &Ty::func_eff(args, ret.clone(), effect.clone()))?;
                self.perform(effect)?;
                ret
            }
            Term::Let(value, body) => {
                let t = self.term(value, env, level + 1)?;
                env.push(Binding::Poly(Scheme { level, body: t }));
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
                Ty::variant(fields[0].0.clone(), self.term(&fields[0].1, env, level)?)
            }
            Term::Record(fields) => Ty::record(
                fields.iter().map(|(k, t)| Ok((k.clone(), self.term(t, env, level)?))).collect::<Result<Vec<_>>>()?,
            ),
            Term::Field(name, record) => {
                let r = self.term(record, env, level)?;
                let out = s.fresh_ty(level);
                s.constrain(&r, &Ty::record([(name.clone(), out.clone())]))?;
                out
            }
            Term::Update(record, name, value) => {
                let r = self.term(record, env, level)?;
                let v = self.term(value, env, level)?;
                Ty::update(r, name.clone(), v)
            }
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
                let (params, ret, effect) = signature(s, *op, args.len(), level);
                for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                    s.constrain(arg, param)
                        .map_err(|e| TypeError(format!("argument {} of `{}`: {e}", i + 1, op.name())))?;
                }
                self.perform(effect)?;
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
            return Ok((Ty::update(t, name.clone(), inner), binders));
        }
        Ok(match pattern {
            Pattern::Keyword(k) => (Ty::keys([k.clone()]), vec![]),
            Pattern::Str(_) => (Ty::str(), vec![]),
            Pattern::Int(_) => (Ty::int(), vec![]),
            Pattern::Any => (t, vec![]),
            Pattern::Bind(_) => (t.clone(), vec![t]),
            Pattern::Fields(fields) => {
                // A field is read only where the value has it, so a
                // variant pattern asks nothing of the other variants.
                let mut out = t.clone();
                let mut binders = Vec::new();
                for (name, p) in fields {
                    let field = s.fresh_ty(level);
                    s.constrain(&t, &Ty::if_has(name.clone(), field.clone()))?;
                    let (inner, bound) = self.narrow(field, &[], p, level)?;
                    out = Ty::update(out, name.clone(), inner);
                    binders.extend(bound);
                }
                (out, binders)
            }
        })
    }

    /// The innermost function body may use what `effect` uses.
    fn perform(&self, effect: Ty) -> Result<()> {
        let current = self.effects.borrow().last().cloned();
        match current {
            Some(body) => self.solver.constrain(&effect, &body),
            None => Ok(()),
        }
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
pub fn signature(s: &Solver, op: Op, argc: usize, level: u32) -> (Vec<Ty>, Ty, Ty) {
    let var = || s.fresh_ty(level);
    // `map` and `filter` use whatever their function uses.
    if let Op::Map | Op::Filter = op {
        let (a, b, e) = (var(), var(), var());
        let f_ret = if op == Op::Map { b.clone() } else { Ty::bool() };
        let out = if op == Op::Map { Ty::list(b) } else { Ty::list(a.clone()) };
        return (vec![Ty::func_eff(vec![a.clone()], f_ret, e.clone()), Ty::list(a)], out, e);
    }
    let (params, ret) = pure_signature(s, op, argc, level);
    (params, ret, Ty::pure())
}

fn pure_signature(s: &Solver, op: Op, argc: usize, level: u32) -> (Vec<Ty>, Ty) {
    let var = || s.fresh_ty(level);
    let int2 = || vec![Ty::int(), Ty::int()];
    match op {
        Op::Add | Op::Sub | Op::Mul | Op::Quot | Op::Rem => (int2(), Ty::int()),
        Op::Lt | Op::Gt | Op::Le | Op::Ge => (int2(), Ty::bool()),
        Op::Eq | Op::NotEq => (vec![Ty::Top, Ty::Top], Ty::bool()),
        Op::Not => (vec![Ty::bool()], Ty::bool()),
        Op::Str => (vec![Ty::Top; argc], Ty::str()),
        Op::Count => (vec![Ty::list(Ty::Top)], Ty::int()),
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
            (vec![Ty::func(vec![a.clone()], b.clone()), Ty::list(a)], Ty::list(b))
        }
        Op::Filter => {
            let a = var();
            (vec![Ty::func(vec![a.clone()], Ty::bool()), Ty::list(a.clone())], Ty::list(a))
        }
        // The default comes back for a missing key.
        Op::Get => {
            let v = var();
            (vec![Ty::dict(v.clone()), Ty::str(), v.clone()], v)
        }
        Op::Assoc => {
            let v = var();
            (vec![Ty::dict(v.clone()), Ty::str(), v.clone()], Ty::dict(v))
        }
        Op::Dissoc => {
            let v = var();
            (vec![Ty::dict(v.clone()), Ty::str()], Ty::dict(v))
        }
        Op::H => (vec![Ty::str(), Ty::dict(Ty::str()), Ty::list(Ty::Top), Ty::dict(Ty::str())], Ty::View),
        Op::Emit => (vec![Ty::str(), Ty::str()], Ty::Effect),
        Op::Trim => (vec![Ty::str()], Ty::str()),
        Op::Fetch => (vec![Ty::str(), Ty::str(), Ty::str(), Ty::str()], Ty::Effect),
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
    }
}

fn json() -> Ty {
    Ty::Prim(crate::types::Prim::Json)
}

/// The type a value must have for patterns that cover it to cover it:
/// the variants named, each with what its payload patterns cover, or a
/// record with the fields a pattern of `_`s names.
fn cover_type(patterns: &[&Pattern]) -> Option<Ty> {
    // An irrefutable clause covers by itself; the fields it names are
    // required by the reads that test them.
    if patterns.iter().any(|p| p.irrefutable()) {
        return None;
    }
    let Some(variants) = patterns.iter().map(|p| p.variant()).collect::<Option<Vec<_>>>() else {
        // A record pattern of `_`s: the value must have its fields.
        let Some(Pattern::Fields(fields)) = patterns.iter().find(|p| p.fields_only()) else { return None };
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
