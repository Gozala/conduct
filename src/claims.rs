//! Claims: the attributes and types a codebase declares, and how a
//! type a claim writes becomes a type the checker uses.
//!
//! An attribute is a global name (`io.gozala.person/name`) with a local
//! one and the type of its values. A record type lists attributes; a
//! variant lists cases in rank order; a claim with parameters is a type
//! constructor, `(ok ?a)`. Expanding a type constructor substitutes its
//! arguments for its parameters, so types are structural all the way
//! down.

use crate::syntax::{Member, TypeExpr};
use crate::types::{Prim, Solver, Ty};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

/// A type a claim defines.
#[derive(Clone, Debug)]
pub struct TypeDef {
    /// Its parameters: each one's label (the first may have none) and
    /// the variable it binds.
    pub params: Vec<(Option<String>, String)>,
    pub body: TypeExpr,
    /// The claim as written, which two claims of one name must agree on.
    pub text: String,
}

/// An attribute of a record type, as its constructor and its pattern
/// take it.
#[derive(Clone, Debug)]
pub struct Field {
    /// The label: the attribute's local name, or the last part of its
    /// global name when it is written in place.
    pub label: String,
    pub global: String,
    pub optional: bool,
}

#[derive(Clone, Default)]
pub struct Claims {
    /// Attributes: local name to global name.
    pub attributes: BTreeMap<String, String>,
    /// Each global name's declared value type, as written and as a type.
    pub attribute_types: BTreeMap<String, (String, Ty)>,
    /// Types: record types, variants, constructors and aliases.
    pub types: BTreeMap<String, TypeDef>,
}

/// How a claim's type variables are read.
pub enum Mode<'a> {
    /// As types the caller chooses: what a definition is checked against.
    Rigid,
    /// As fresh variables at a level: what callers instantiate.
    Flexible(&'a Solver, u32),
}

/// The variables of the claim being read.
pub struct Env<'a> {
    pub vars: HashMap<String, Ty>,
    pub mode: Mode<'a>,
    depth: u32,
}

impl<'a> Env<'a> {
    pub fn new(mode: Mode<'a>) -> Self {
        Env { vars: HashMap::new(), mode, depth: 0 }
    }

    fn var(&mut self, name: &str) -> Ty {
        if let Some(t) = self.vars.get(name) {
            return t.clone();
        }
        let t = match self.mode {
            Mode::Rigid => Ty::Rigid(Rc::from(name)),
            Mode::Flexible(s, level) => s.fresh_ty(level),
        };
        self.vars.insert(name.to_string(), t.clone());
        t
    }

    /// A variable for capabilities a claim leaves to inference: always
    /// flexible, even while checking.
    pub fn inferred(&mut self, solver: &Solver, level: u32) -> Ty {
        solver.fresh_ty(level)
    }
}

/// Above this many nested expansions a type is taken to be recursive.
const DEPTH: u32 = 32;

type Result<T> = std::result::Result<T, String>;

impl Claims {
    /// The global name of an attribute, written by its local name or its
    /// global one.
    pub fn attribute(&self, name: &str) -> Option<String> {
        if let Some(global) = self.attributes.get(name) {
            return Some(global.clone());
        }
        self.attribute_types.contains_key(name).then(|| name.to_string())
    }

    /// The local name a global name is declared under, if any: an
    /// attribute's, or the label a record type written in place gives
    /// it, as `(claim (ok ?a) : {(conduct.result/ok ?a)})` does `ok`.
    pub fn local(&self, global: &str) -> Option<String> {
        if let Some((l, _)) = self.attributes.iter().find(|(_, g)| *g == global) {
            return Some(l.clone());
        }
        let inline = self.types.values().any(|t| {
            matches!(&t.body, TypeExpr::Record(members, _)
                if members.iter().any(|m| matches!(m, Member::Inline(g, _) if g == global)))
        });
        inline.then(|| global.rsplit('/').next().unwrap().to_string())
    }

    /// How a global name is shown: by the attribute's local name, or, as
    /// the one attribute of a record type a claim writes in place, by
    /// that type's constructor, `(ok Int)`.
    pub fn shown(&self, global: &str) -> Option<String> {
        if let Some((l, _)) = self.attributes.iter().find(|(_, g)| *g == global) {
            return Some(l.clone());
        }
        self.types.iter().find_map(|(name, t)| match &t.body {
            TypeExpr::Record(members, 1) if matches!(members.as_slice(), [Member::Inline(g, _)] if g == global) => {
                Some(format!("{}{name}", crate::types::CONSTRUCTOR))
            }
            _ => None,
        })
    }

    /// Whether `name` is declared as anything: an attribute or a type.
    pub fn claims(&self, name: &str) -> bool {
        self.attributes.contains_key(name) || self.types.contains_key(name)
    }

    /// The attributes of a record type, in the order its claim lists
    /// them: the first is its constructor's default label.
    pub fn record(&self, name: &str) -> Option<Vec<Field>> {
        let TypeExpr::Record(members, optional) = &self.types.get(name)?.body else { return None };
        members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let (label, global) = match m {
                    Member::Name(n, _) => (n.clone(), self.attributes.get(n)?.clone()),
                    Member::Inline(g, _) => (g.rsplit('/').next().unwrap().to_string(), g.clone()),
                };
                Some(Field { label, global, optional: i >= *optional })
            })
            .collect()
    }

    /// The cases of a variant of record types, in rank order.
    pub fn variant(&self, name: &str) -> Option<Vec<String>> {
        let TypeExpr::Union(cases) = &self.types.get(name)?.body else { return None };
        cases
            .iter()
            .map(|c| match c {
                TypeExpr::Name(n, _) | TypeExpr::App(n, _, _) if self.record(n).is_some() => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    /// The global names of a record type's required attributes.
    pub fn required(&self, record: &str) -> BTreeSet<String> {
        self.record(record).unwrap_or_default().into_iter().filter(|f| !f.optional).map(|f| f.global).collect()
    }

    /// The type a claim writes.
    pub fn ty(&self, t: &TypeExpr, env: &mut Env) -> Result<Ty> {
        Ok(match t {
            TypeExpr::Name(name, _) => match name.as_str() {
                "text" => Ty::str(),
                "integer" => Ty::int(),
                "boolean" => Ty::bool(),
                "json" => Ty::Prim(Prim::Json),
                "keyword" => Ty::Prim(Prim::Keyword),
                "any" => Ty::Top,
                "nothing" => Ty::Bot,
                "view" => Ty::View,
                "effect" => Ty::Effect,
                _ => {
                    if let Some(def) = self.types.get(name) {
                        if !def.params.is_empty() {
                            return Err(format!("`{name}` takes {} type(s): `({name} ..)`", def.params.len()));
                        }
                        return self.expand(name, def, vec![], env);
                    }
                    if let Some(global) = self.attributes.get(name) {
                        return Ok(self.attribute_types[global].1.clone());
                    }
                    return Err(format!(
                        "`{name}` is not a type: declare it, or use `text`, `integer`, `boolean`, `keyword`, `any`"
                    ));
                }
            },
            TypeExpr::Var(v) => env.var(v),
            TypeExpr::App(head, args, _) => match (head.as_str(), args.as_slice()) {
                ("task", [(None, t)]) => Ty::task(self.ty(t, env)?, Ty::pure()),
                ("task", [(None, t), (Some(w), caps)]) if w == "with" => {
                    // `{print}` reads as a record type; here it names
                    // capabilities.
                    let caps = match caps {
                        TypeExpr::Record(members, _) => Ty::caps(members.iter().map(|m| m.to_string())),
                        other => self.ty(other, env)?,
                    };
                    Ty::task(self.ty(t, env)?, caps)
                }
                ("list", [(None, t)]) => Ty::list(self.ty(t, env)?),
                ("dict", [(None, t)]) => Ty::dict(self.ty(t, env)?),
                _ => {
                    let def = self.types.get(head).ok_or_else(|| format!("`{head}` is not a type constructor"))?;
                    let mut bound = Vec::new();
                    for (i, (label, var)) in def.params.iter().enumerate() {
                        let arg = args
                            .iter()
                            .find(|(l, _)| match l {
                                None => i == 0,
                                Some(l) => Some(l) == label.as_ref(),
                            })
                            .ok_or_else(|| {
                                format!(
                                    "`({head} ..)` needs {}",
                                    label.as_ref().map(|l| format!("`{l}:`")).unwrap_or("its first type".into())
                                )
                            })?;
                        bound.push((var.clone(), self.ty(&arg.1, env)?));
                    }
                    if args.len() > def.params.len() {
                        return Err(format!("`{head}` takes {} type(s), given {}", def.params.len(), args.len()));
                    }
                    self.expand(head, def, bound, env)?
                }
            },
            TypeExpr::Attribute(global, value, _) => Ty::variant(global.clone(), self.ty(value, env)?),
            TypeExpr::Record(members, optional) => {
                let mut fields = Vec::new();
                for (i, m) in members.iter().enumerate() {
                    let (global, t) = match m {
                        Member::Name(n, _) => {
                            let global = self.attributes.get(n).ok_or_else(|| {
                                if self.types.contains_key(n) {
                                    format!(
                                        "`{n}` is a type; members of a record are attributes: declare one that \
                                         holds it, as `(type start : (domain/start {n}))` does"
                                    )
                                } else {
                                    format!("`{n}` in a record type is not a declared attribute")
                                }
                            })?;
                            (global.clone(), self.attribute_types[global].1.clone())
                        }
                        Member::Inline(g, value) => (g.clone(), self.ty(value, env)?),
                    };
                    fields.push((global, if i >= *optional { Ty::opt(t) } else { t }));
                }
                match fields.as_slice() {
                    [(global, t)] if !matches!(t, Ty::Opt(_)) => Ty::variant(global.clone(), t.clone()),
                    _ => Ty::record(fields),
                }
            }
            TypeExpr::Union(cases) => {
                let cases = cases.iter().map(|c| self.ty(c, env)).collect::<Result<Vec<_>>>()?;
                union(cases)?
            }
            TypeExpr::Keyword(k) => Ty::keys([k.clone()]),
            // A function type with capabilities answers a task that needs
            // them: `((\ ?a) with: {print} : {})`.
            TypeExpr::Fn(params, with, ret) => {
                let params = self.params(params, env)?;
                let ret = self.ty(ret, env)?;
                let ret = match with {
                    Some(w) => Ty::task(ret, self.ty(w, env)?),
                    None => ret,
                };
                function(params, ret)?
            }
            TypeExpr::Caps(caps) => Ty::caps(caps.iter().cloned()),
        })
    }

    /// A function's parameters' types, each with its label.
    pub fn params(&self, params: &[(Option<String>, TypeExpr)], env: &mut Env) -> Result<Vec<(Option<String>, Ty)>> {
        params.iter().map(|(l, t)| Ok((l.clone(), self.ty(t, env)?))).collect()
    }

    fn expand(&self, name: &str, def: &TypeDef, bound: Vec<(String, Ty)>, env: &mut Env) -> Result<Ty> {
        if env.depth >= DEPTH {
            return Err(format!("`{name}` refers to itself; recursive types are not supported yet"));
        }
        let outer = std::mem::replace(&mut env.vars, bound.into_iter().collect());
        env.depth += 1;
        let out = self.ty(&def.body, env);
        env.depth -= 1;
        env.vars = outer;
        out
    }
}

/// A function type: the first parameter, then the others in label
/// order, as calls pass them.
pub fn function(params: Vec<(Option<String>, Ty)>, ret: Ty) -> Result<Ty> {
    let mut params = params.into_iter();
    let first = params.next().ok_or("a function takes at least one parameter")?;
    let mut rest: Vec<(Option<String>, Ty)> = params.collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    let mut types = vec![first.1];
    let mut labels = Vec::new();
    for (label, t) in rest {
        labels.push(label.ok_or("only the first parameter goes without a label")?);
        types.push(t);
    }
    Ok(Ty::func_eff(types, ret, Ty::pure(), labels))
}

/// The cases of a variant as one type: variants and keywords as one set
/// of tags; records of several attributes joined, those not every case
/// has becoming optional.
fn union(cases: Vec<Ty>) -> Result<Ty> {
    if cases.iter().all(|c| matches!(c, Ty::Tags(_))) {
        let mut tags = BTreeMap::new();
        for c in cases {
            let Ty::Tags(t) = c else { unreachable!() };
            for (k, p) in t.iter() {
                if tags.insert(k.clone(), p.clone()).is_some() {
                    return Err(format!("`{k}` is a case of the variant twice"));
                }
            }
        }
        return Ok(Ty::tags(tags));
    }
    let mut records = Vec::new();
    for c in cases {
        match c {
            Ty::Record(fields) => records.push((*fields).clone()),
            Ty::Tags(tags) if tags.len() == 1 => {
                let (k, p) = tags.iter().next().unwrap();
                let Some(p) = p else {
                    return Err(format!("the keyword `:{k}` cannot be a case beside records"));
                };
                records.push([(k.clone(), p.clone())].into_iter().collect());
            }
            _ => return Err("a variant's cases are keywords and records".into()),
        }
    }
    let mut all: BTreeSet<String> = BTreeSet::new();
    records.iter().for_each(|r| all.extend(r.keys().cloned()));
    let mut fields = Vec::new();
    for key in all {
        let required = records.iter().all(|r| r.get(&key).is_some_and(|t| !matches!(t, Ty::Opt(_))));
        let t = records.iter().find_map(|r| r.get(&key)).unwrap().clone();
        let t = match t {
            Ty::Opt(inner) => (*inner).clone(),
            t => t,
        };
        fields.push((key, if required { t } else { Ty::opt(t) }));
    }
    Ok(Ty::record(fields))
}

/// The variables a type mentions.
pub fn vars(t: &TypeExpr, out: &mut BTreeSet<String>) {
    match t {
        TypeExpr::Var(v) => {
            out.insert(v.clone());
        }
        TypeExpr::App(_, args, _) => args.iter().for_each(|(_, t)| vars(t, out)),
        TypeExpr::Attribute(_, t, _) => vars(t, out),
        TypeExpr::Record(members, _) => members.iter().for_each(|m| {
            if let Member::Inline(_, t) = m {
                vars(t, out)
            }
        }),
        TypeExpr::Union(cases) => cases.iter().for_each(|c| vars(c, out)),
        TypeExpr::Fn(params, with, ret) => {
            params.iter().for_each(|(_, t)| vars(t, out));
            if let Some(w) = with {
                vars(w, out);
            }
            vars(ret, out);
        }
        TypeExpr::Name(..) | TypeExpr::Keyword(_) | TypeExpr::Caps(_) => {}
    }
}
