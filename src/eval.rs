//! An interpreter for resolved terms, which runs macros while code is
//! compiled. A macro is pure: it builds forms from forms, so a task, a
//! host call or a view is an error here.

use crate::codebase::Definition;
use crate::term::{Hash, Op, Pattern, Term};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Str(Rc<str>),
    Bool(bool),
    Keyword(Rc<str>),
    List(Rc<Vec<Value>>),
    /// A record, by global attribute names. `{}` is an empty one.
    Record(Rc<BTreeMap<String, Value>>),
    /// A dictionary, its keys sorted.
    Dict(Rc<BTreeMap<String, Value>>),
    Closure(Rc<Closure>),
    /// What an exhaustive `match` lowers its impossible case to.
    Nil,
}

#[derive(Debug)]
pub struct Closure {
    arity: usize,
    body: Rc<Term>,
    env: Vec<Value>,
    cycle: Rc<Vec<Hash>>,
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    /// A variant: a record of one attribute.
    pub fn variant(key: &str, payload: Value) -> Value {
        Value::Record(Rc::new(BTreeMap::from([(key.to_string(), payload)])))
    }

    fn fields(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Record(fields) => Some(fields),
            Value::Dict(d) if d.is_empty() => Some(d),
            _ => None,
        }
    }

    fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) | (Value::Keyword(a), Value::Keyword(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::List(a), Value::List(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y)),
            (Value::Record(a) | Value::Dict(a), Value::Record(b) | Value::Dict(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|((k, x), (l, y))| k == l && x.equals(y))
            }
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::Nil, Value::Nil) => true,
            _ => false,
        }
    }

    fn text(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Str(s) => s.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Keyword(k) => format!(":{k}"),
            Value::List(items) => format!("[{}]", items.iter().map(Value::text).collect::<Vec<_>>().join(" ")),
            _ => "<value>".into(),
        }
    }
}

type Result<T> = std::result::Result<T, String>;

pub struct Machine<'a> {
    pub defs: &'a HashMap<Hash, Definition>,
    /// How many more calls may be made: a macro that does not stop is
    /// an error, not a hang.
    pub fuel: u64,
}

impl Machine<'_> {
    /// The value of a definition.
    pub fn definition(&mut self, hash: Hash) -> Result<Value> {
        let def = self.defs.get(&hash).ok_or_else(|| format!("no definition {}", hash.short()))?;
        let cycle = Rc::new(def.cycle.clone());
        self.eval(&def.term, &mut Vec::new(), &cycle)
    }

    /// Call a function value with its arguments in parameter order.
    pub fn call(&mut self, f: &Value, args: Vec<Value>) -> Result<Value> {
        let Value::Closure(c) = f else {
            return Err("a macro called something that is not a function".into());
        };
        if c.arity != args.len() {
            return Err(format!("a function of {} parameters was given {}", c.arity, args.len()));
        }
        if self.fuel == 0 {
            return Err("ran too long: does it recurse without stopping?".into());
        }
        self.fuel -= 1;
        let mut env = c.env.clone();
        env.extend(args);
        let body = c.body.clone();
        let cycle = c.cycle.clone();
        self.eval(&body, &mut env, &cycle)
    }

    fn eval(&mut self, term: &Term, env: &mut Vec<Value>, cycle: &Rc<Vec<Hash>>) -> Result<Value> {
        Ok(match term {
            Term::Int(n) => Value::Int(*n),
            Term::Str(s) => Value::str(s),
            Term::Bool(b) => Value::Bool(*b),
            Term::Nil => Value::Nil,
            Term::Keyword(k) => Value::Keyword(k.as_str().into()),
            Term::Local(i) => env[env.len() - 1 - *i as usize].clone(),
            Term::Ref(h) => self.definition(*h)?,
            Term::CycleRef(i) => self.definition(cycle[*i as usize])?,
            Term::Fn(labels, body) => Value::Closure(Rc::new(Closure {
                arity: labels.len(),
                body: Rc::new((**body).clone()),
                env: env.clone(),
                cycle: cycle.clone(),
            })),
            Term::App(callee, args, _) => {
                let f = self.eval(callee, env, cycle)?;
                let args = args.iter().map(|a| self.eval(a, env, cycle)).collect::<Result<Vec<_>>>()?;
                self.call(&f, args)?
            }
            Term::Let(value, body) => {
                let v = self.eval(value, env, cycle)?;
                env.push(v);
                let out = self.eval(body, env, cycle);
                env.pop();
                out?
            }
            Term::If(c, t, e) => match self.eval(c, env, cycle)? {
                Value::Bool(true) => self.eval(t, env, cycle)?,
                Value::Bool(false) => self.eval(e, env, cycle)?,
                other => return Err(format!("`if` needs a Bool, not `{}`", other.text())),
            },
            Term::And(a, b) => match self.eval(a, env, cycle)? {
                Value::Bool(true) => self.eval(b, env, cycle)?,
                v => v,
            },
            Term::Or(a, b) => match self.eval(a, env, cycle)? {
                Value::Bool(false) => self.eval(b, env, cycle)?,
                v => v,
            },
            Term::Record(fields) => {
                let mut out = BTreeMap::new();
                for (k, t) in fields {
                    out.insert(k.clone(), self.eval(t, env, cycle)?);
                }
                Value::Record(Rc::new(out))
            }
            Term::Field(k, r) => {
                let r = self.eval(r, env, cycle)?;
                r.fields().and_then(|f| f.get(k)).cloned().ok_or_else(|| format!("no attribute `{k}`"))?
            }
            Term::Update(r, k, v) => {
                let r = self.eval(r, env, cycle)?;
                let mut fields = r.fields().cloned().ok_or("`assoc` sets an attribute of a record")?;
                fields.insert(k.clone(), self.eval(v, env, cycle)?);
                Value::Record(Rc::new(fields))
            }
            Term::Dict(entries) => {
                let mut out = BTreeMap::new();
                for (k, t) in entries {
                    out.insert(k.clone(), self.eval(t, env, cycle)?);
                }
                Value::Dict(Rc::new(out))
            }
            Term::List(items) => {
                Value::List(Rc::new(items.iter().map(|t| self.eval(t, env, cycle)).collect::<Result<_>>()?))
            }
            Term::Prim(op, args) => {
                let args = args.iter().map(|a| self.eval(a, env, cycle)).collect::<Result<Vec<_>>>()?;
                self.prim(*op, args)?
            }
            Term::Match(scrutinee, clauses, default) => {
                let v = self.eval(scrutinee, env, cycle)?;
                for (pattern, result) in clauses {
                    let mut bound = Vec::new();
                    if matches(pattern, &v, &mut bound) {
                        let n = bound.len();
                        env.extend(bound);
                        let out = self.eval(result, env, cycle);
                        env.truncate(env.len() - n);
                        return out;
                    }
                }
                match default {
                    Some(d) => self.eval(d, env, cycle)?,
                    None => return Err("no clause of a `match` fits".into()),
                }
            }
            Term::Task(_) | Term::Perform(_) | Term::Handle(..) | Term::Host(..) => {
                return Err("a macro is pure: it cannot make or perform a task".into());
            }
        })
    }

    fn prim(&mut self, op: Op, args: Vec<Value>) -> Result<Value> {
        let int = |v: &Value| match v {
            Value::Int(n) => Ok(*n),
            other => Err(format!("`{}` needs an Int, not `{}`", op.name(), other.text())),
        };
        let list = |v: &Value| match v {
            Value::List(items) => Ok(items.clone()),
            other => Err(format!("`{}` needs a list, not `{}`", op.name(), other.text())),
        };
        let text = |v: &Value| match v {
            Value::Str(s) | Value::Keyword(s) => Ok(s.clone()),
            other => Err(format!("`{}` needs a string, not `{}`", op.name(), other.text())),
        };
        let dict = |v: &Value| match v {
            Value::Dict(d) => Ok((**d).clone()),
            Value::Record(r) if r.is_empty() => Ok(BTreeMap::new()),
            other => Err(format!("`{}` needs a dictionary, not `{}`", op.name(), other.text())),
        };
        let a = &args;
        Ok(match op {
            Op::Add => Value::Int(int(&a[0])?.wrapping_add(int(&a[1])?)),
            Op::Sub => Value::Int(int(&a[0])?.wrapping_sub(int(&a[1])?)),
            Op::Mul => Value::Int(int(&a[0])?.wrapping_mul(int(&a[1])?)),
            Op::Quot | Op::Rem => {
                let d = int(&a[1])?;
                if d == 0 {
                    return Err("division by zero".into());
                }
                Value::Int(if op == Op::Quot { int(&a[0])? / d } else { int(&a[0])? % d })
            }
            Op::Lt => Value::Bool(int(&a[0])? < int(&a[1])?),
            Op::Gt => Value::Bool(int(&a[0])? > int(&a[1])?),
            Op::Le => Value::Bool(int(&a[0])? <= int(&a[1])?),
            Op::Ge => Value::Bool(int(&a[0])? >= int(&a[1])?),
            Op::Eq => Value::Bool(a[0].equals(&a[1])),
            Op::NotEq => Value::Bool(!a[0].equals(&a[1])),
            Op::Not => match a[0] {
                Value::Bool(b) => Value::Bool(!b),
                ref other => return Err(format!("`not` needs a Bool, not `{}`", other.text())),
            },
            Op::Str => Value::str(&a.iter().map(Value::text).collect::<String>()),
            Op::Count => Value::Int(list(&a[0])?.len() as i64),
            Op::First => list(&a[0])?.first().cloned().unwrap_or_else(|| a[1].clone()),
            Op::Rest => Value::List(Rc::new(list(&a[0])?.iter().skip(1).cloned().collect())),
            Op::Concat => Value::List(Rc::new(list(&a[0])?.iter().chain(list(&a[1])?.iter()).cloned().collect())),
            Op::Conj => {
                let mut items = (*list(&a[0])?).clone();
                items.push(a[1].clone());
                Value::List(Rc::new(items))
            }
            Op::Map => {
                let items = list(&a[1])?;
                Value::List(Rc::new(items.iter().map(|x| self.call(&a[0], vec![x.clone()])).collect::<Result<_>>()?))
            }
            Op::Filter => {
                let mut out = Vec::new();
                for x in list(&a[1])?.iter() {
                    if let Value::Bool(true) = self.call(&a[0], vec![x.clone()])? {
                        out.push(x.clone());
                    }
                }
                Value::List(Rc::new(out))
            }
            Op::Get => dict(&a[0])?.get(&*text(&a[1])?).cloned().unwrap_or_else(|| a[2].clone()),
            Op::Assoc => {
                let mut d = dict(&a[0])?;
                d.insert(text(&a[1])?.to_string(), a[2].clone());
                Value::Dict(Rc::new(d))
            }
            Op::Dissoc => {
                let mut d = dict(&a[0])?;
                d.remove(&*text(&a[1])?);
                Value::Dict(Rc::new(d))
            }
            Op::Trim => Value::str(text(&a[0])?.trim()),
            Op::Split => {
                let (s, sep) = (text(&a[0])?, text(&a[1])?);
                Value::List(Rc::new(s.split(&*sep).map(Value::str).collect()))
            }
            Op::HasField => Value::Bool(a[0].fields().is_some_and(|f| f.contains_key(&*text(&a[1]).unwrap_or_default()))),
            Op::H
            | Op::Emit
            | Op::Fetch
            | Op::ParseJson
            | Op::ToJson
            | Op::JsonGet
            | Op::JsonItems
            | Op::JsonStr
            | Op::JsonInt
            | Op::JsonBool
            | Op::JsonType
            | Op::WithEnv => return Err(format!("`{}` is not available to a macro", op.name())),
        })
    }
}

/// Whether `v` fits `pattern`, pushing what it binds in order.
fn matches(pattern: &Pattern, v: &Value, bound: &mut Vec<Value>) -> bool {
    match pattern {
        Pattern::Any => true,
        Pattern::Bind(_) => {
            bound.push(v.clone());
            true
        }
        Pattern::Keyword(k) => matches!(v, Value::Keyword(x) if **x == **k),
        Pattern::Str(s) => matches!(v, Value::Str(x) if **x == **s),
        Pattern::Int(n) => matches!(v, Value::Int(x) if x == n),
        Pattern::Fields(fields) => {
            let Some(have) = v.fields() else { return false };
            fields.iter().all(|(k, p)| have.get(k).is_some_and(|x| matches(p, x, bound)))
        }
    }
}
