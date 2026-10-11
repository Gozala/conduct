//! Typing rules: the type of every term, with no annotations.

use crate::term::{Hash, Op, Term};
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
}

type Result<T> = std::result::Result<T, TypeError>;

impl Infer<'_> {
    pub fn term(&self, term: &Term, env: &mut Vec<Binding>, level: u32) -> Result<Ty> {
        let s = self.solver;
        Ok(match term {
            Term::Int(_) => Ty::int(),
            Term::Str(_) => Ty::str(),
            Term::Bool(_) => Ty::bool(),
            Term::Nil => Ty::nil(),
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
                let ret = self.term(body, env, level);
                env.truncate(env.len() - *arity as usize);
                Ty::func(params, ret?)
            }
            Term::App(callee, args) => {
                let f = self.term(callee, env, level)?;
                let args = args.iter().map(|a| self.term(a, env, level)).collect::<Result<Vec<_>>>()?;
                let ret = s.fresh_ty(level);
                s.constrain(&f, &Ty::func(args, ret.clone()))?;
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
            Term::IfSome(value, then, otherwise) => {
                let v = self.term(value, env, level)?;
                let bound = s.fresh_ty(level);
                s.constrain(&Ty::without(v), &bound)?;
                env.push(Binding::Mono(bound));
                let t = self.term(then, env, level);
                env.pop();
                self.join([t?, self.term(otherwise, env, level)?], level)?
            }
            Term::Coalesce(a, b) => {
                let a = self.term(a, env, level)?;
                let b = self.term(b, env, level)?;
                self.join([Ty::without(a), b], level)?
            }
            Term::And(a, b) | Term::Or(a, b) => {
                for part in [a, b] {
                    let t = self.term(part, env, level)?;
                    s.constrain(&t, &Ty::bool()).map_err(|e| TypeError(format!("`and` and `or` take Bools: {e}")))?;
                }
                Ty::bool()
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
            Term::Dict(entries) => {
                let values = entries.iter().map(|(_, t)| self.term(t, env, level)).collect::<Result<Vec<_>>>()?;
                Ty::dict(self.join(values, level)?)
            }
            Term::List(items) => {
                let items = items.iter().map(|t| self.term(t, env, level)).collect::<Result<Vec<_>>>()?;
                Ty::list(self.join(items, level)?)
            }
            Term::Prim(op, args) => {
                let args = args.iter().map(|a| self.term(a, env, level)).collect::<Result<Vec<_>>>()?;
                let (params, ret) = signature(s, *op, args.len(), level);
                for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                    s.constrain(arg, param)
                        .map_err(|e| TypeError(format!("argument {} of `{}`: {e}", i + 1, op.name())))?;
                }
                ret
            }
        })
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
pub fn signature(s: &Solver, op: Op, argc: usize, level: u32) -> (Vec<Ty>, Ty) {
    let var = || s.fresh_ty(level);
    let int2 = || vec![Ty::int(), Ty::int()];
    match op {
        Op::Add | Op::Sub | Op::Mul | Op::Quot | Op::Rem => (int2(), Ty::int()),
        Op::Lt | Op::Gt | Op::Le | Op::Ge => (int2(), Ty::bool()),
        Op::Eq | Op::NotEq => (vec![Ty::Top, Ty::Top], Ty::bool()),
        Op::Not => (vec![Ty::bool()], Ty::bool()),
        Op::IsNil => (vec![Ty::Top], Ty::bool()),
        Op::Str => (vec![Ty::Top; argc], Ty::str()),
        Op::Count => (vec![Ty::list(Ty::Top)], Ty::int()),
        Op::First => {
            let a = var();
            let out = optional(s, a.clone(), level);
            (vec![Ty::list(a)], out)
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
        Op::Get => {
            let v = var();
            let out = optional(s, v.clone(), level);
            (vec![Ty::dict(v), Ty::str()], out)
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
        Op::Emit => (vec![Ty::str(), Ty::str()], Ty::Emit),
    }
}

/// `t | nil`, as a variable with both as lower bounds.
fn optional(s: &Solver, t: Ty, level: u32) -> Ty {
    let out = s.fresh(level);
    out.lower.borrow_mut().push(t);
    out.lower.borrow_mut().push(Ty::nil());
    Ty::Var(out)
}
