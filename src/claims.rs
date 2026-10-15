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
use crate::types::{Prim, Solver, Ty, Var};
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
    /// The type of an attribute's values as written, where it names no
    /// variable: read again inside a type, so a type made of itself
    /// through an attribute is.
    pub attribute_exprs: BTreeMap<String, TypeExpr>,
    /// Types: record types, variants, constructors and aliases.
    pub types: BTreeMap<String, TypeDef>,
}

/// How a claim's type variables are read.
pub enum Mode<'a> {
    /// As types the caller chooses: what a definition is checked against.
    Rigid(&'a Solver),
    /// As fresh variables at a level: what callers instantiate.
    Flexible(&'a Solver, u32),
}

/// The variables of the claim being read.
pub struct Env<'a> {
    pub vars: HashMap<String, Ty>,
    pub mode: Mode<'a>,
    depth: u32,
    /// Whether what is read flows out (a result) rather than in (a
    /// parameter).
    positive: bool,
    /// The types being expanded, each with its arguments, the variable
    /// that stands for it inside itself, and whether it did.
    expanding: Vec<(String, Vec<Ty>, Var, bool)>,
}

impl<'a> Env<'a> {
    pub fn new(mode: Mode<'a>) -> Self {
        Env { vars: HashMap::new(), mode, depth: 0, positive: true, expanding: Vec::new() }
    }

    fn var(&mut self, name: &str) -> Ty {
        if let Some(t) = self.vars.get(name) {
            return t.clone();
        }
        let t = match self.mode {
            Mode::Rigid(_) => Ty::Rigid(Rc::from(name)),
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
                let (label, global, opt) = match m {
                    Member::Name(n, _) => (n.clone(), self.attributes.get(n)?.clone(), false),
                    Member::Inline(g, _) => (g.rsplit('/').next().unwrap().to_string(), g.clone(), false),
                    Member::Field(l, _, opt) => (l.clone(), self.labeled(l)?, *opt),
                };
                Some(Field { label, global, optional: opt || i >= *optional })
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

    /// The global name of an attribute a record type labels: a declared
    /// attribute's local name, or the last part of a global name some
    /// record type writes in place.
    pub fn labeled(&self, label: &str) -> Option<String> {
        if let Some(global) = self.attribute(label) {
            return Some(global);
        }
        // A global name is itself.
        if label.contains('/') {
            return Some(label.to_string());
        }
        let mut inline = self.types.values().flat_map(|t| match &t.body {
            TypeExpr::Record(members, _) => members
                .iter()
                .filter_map(|m| match m {
                    Member::Inline(g, _) if g.rsplit('/').next() == Some(label) => Some(g.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        });
        let first = inline.next()?;
        inline.all(|g| g == first).then_some(first)
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
                "natural" => Ty::nat(),
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
                        return self.attribute_ty(global, env);
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
                            (global.clone(), self.attribute_ty(global, env)?)
                        }
                        Member::Inline(g, value) => (g.clone(), self.ty(value, env)?),
                        Member::Field(l, value, _) => {
                            let global = self.labeled(l).ok_or_else(|| {
                                format!(
                                    "`{l}:` is not a declared attribute: declare it, `(type {l} : (domain/{l} type))`"
                                )
                            })?;
                            (global, self.ty(value, env)?)
                        }
                    };
                    let opt = i >= *optional || matches!(m, Member::Field(_, _, true));
                    fields.push((global, if opt { Ty::opt(t) } else { t }));
                }
                // A record of one attribute is the variant it is, except
                // where a value flows in written as it is shown, `{x:
                // integer}`: there it asks only for the attribute.
                let shown = members.iter().all(|m| matches!(m, Member::Field(..)));
                match fields.as_slice() {
                    [(global, t)] if !matches!(t, Ty::Opt(_)) && (env.positive || !shown) => {
                        Ty::variant(global.clone(), t.clone())
                    }
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
            TypeExpr::Inter(parts) => {
                let mut parts = parts
                    .iter()
                    .map(|p| match self.ty(p, env)? {
                        // `{x: integer ?a ...}`: the record `?a`, which has `x`.
                        Ty::Tags(tags) if matches!(p, TypeExpr::Record(..)) => {
                            Ok(Ty::record(tags.iter().map(|(k, t)| (k.clone(), t.clone().unwrap_or(Ty::Top)))))
                        }
                        t => Ok(t),
                    })
                    .collect::<Result<Vec<_>>>()?;
                if parts.len() == 1 { parts.pop().unwrap() } else { Ty::Inter(Rc::new(parts)) }
            }
            TypeExpr::Update(base, steps) => {
                let mut out = self.ty(base, env)?;
                for (label, value) in steps {
                    let global = self.labeled(label).ok_or_else(|| format!("`{label}` is not a declared attribute"))?;
                    out = match value {
                        Some(v) => Ty::update(out, global, self.ty(v, env)?),
                        None => Ty::without(out, global),
                    };
                }
                out
            }
            // A variable that is the type again: bounded by it both ways.
            TypeExpr::Rec(v, body) => {
                let solver = match env.mode {
                    Mode::Rigid(s) | Mode::Flexible(s, _) => s,
                };
                let Ty::Var(var) = solver.fresh_ty(1) else { unreachable!() };
                let outer = env.vars.insert(v.clone(), Ty::Var(var.clone()));
                let t = self.ty(body, env);
                match outer {
                    Some(o) => env.vars.insert(v.clone(), o),
                    None => env.vars.remove(v),
                };
                let t = t?;
                var.lower.borrow_mut().push(t.clone());
                var.upper.borrow_mut().push(t);
                Ty::Var(var)
            }
        })
    }

    /// The type of an attribute's values, read inside a type.
    fn attribute_ty(&self, global: &str, env: &mut Env) -> Result<Ty> {
        let Some(expr) = self.attribute_exprs.get(global) else { return Ok(self.attribute_types[global].1.clone()) };
        let outer = std::mem::take(&mut env.vars);
        let out = self.ty(expr, env);
        env.vars = outer;
        out
    }

    /// A function's parameters' types, each with its label.
    pub fn params(&self, params: &[(Option<String>, TypeExpr)], env: &mut Env) -> Result<Vec<(Option<String>, Ty)>> {
        env.positive = !env.positive;
        let out = params.iter().map(|(l, t)| Ok((l.clone(), self.ty(t, env)?))).collect();
        env.positive = !env.positive;
        out
    }

    /// A type by its definition. A type met again inside itself, with
    /// the same arguments, is the type again: a variable bounded by it
    /// both ways, as `(rec ?a ..)` writes one.
    fn expand(&self, name: &str, def: &TypeDef, bound: Vec<(String, Ty)>, env: &mut Env) -> Result<Ty> {
        let args: Vec<Ty> = bound.iter().map(|(_, t)| t.clone()).collect();
        if let Some(entry) = env
            .expanding
            .iter_mut()
            .find(|(n, a, _, _)| n == name && a.len() == args.len() && a.iter().zip(&args).all(|(x, y)| same(x, y)))
        {
            entry.3 = true;
            return Ok(Ty::Var(entry.2.clone()));
        }
        if env.depth >= DEPTH {
            let names: Vec<&str> = env.expanding.iter().map(|(n, ..)| n.as_str()).collect();
            let recurring = names.iter().find(|n| names.iter().filter(|m| m == n).count() > 1).unwrap_or(&name);
            return Err(format!(
                "`{recurring}` refers to itself with other types each time; only a type that recurs with the same ones \
                 is supported"
            ));
        }
        let solver = match env.mode {
            Mode::Rigid(s) | Mode::Flexible(s, _) => s,
        };
        let Ty::Var(var) = solver.fresh_ty(1) else { unreachable!() };
        env.expanding.push((name.to_string(), args, var, false));
        let outer = std::mem::replace(&mut env.vars, bound.into_iter().collect());
        env.depth += 1;
        let out = self.ty(&def.body, env);
        env.depth -= 1;
        env.vars = outer;
        let (_, _, var, recurs) = env.expanding.pop().unwrap();
        let out = out?;
        if !recurs {
            return Ok(out);
        }
        var.lower.borrow_mut().push(out.clone());
        var.upper.borrow_mut().push(out);
        Ok(Ty::Var(var))
    }
}

/// Whether two types are the same one: the same variable or primitive,
/// or one built once and shared, as an argument passed on unchanged is.
fn same(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Var(x), Ty::Var(y)) => Rc::ptr_eq(x, y),
        (Ty::Rigid(x), Ty::Rigid(y)) => x == y,
        (Ty::Prim(x), Ty::Prim(y)) => x == y,
        (Ty::Top, Ty::Top) | (Ty::Bot, Ty::Bot) | (Ty::View, Ty::View) | (Ty::Effect, Ty::Effect) => true,
        (Ty::List(x), Ty::List(y)) | (Ty::Dict(x), Ty::Dict(y)) | (Ty::Opt(x), Ty::Opt(y)) => Rc::ptr_eq(x, y),
        (Ty::Record(x), Ty::Record(y)) => Rc::ptr_eq(x, y),
        (Ty::Tags(x), Ty::Tags(y)) => Rc::ptr_eq(x, y),
        (Ty::Union(x), Ty::Union(y)) | (Ty::Inter(x), Ty::Inter(y)) => Rc::ptr_eq(x, y),
        (Ty::Fn(x, ..), Ty::Fn(y, ..)) => Rc::ptr_eq(x, y),
        _ => false,
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
    // A record of one attribute among cases is the variant it is.
    let cases: Vec<Ty> = cases
        .into_iter()
        .map(|c| match c {
            Ty::Record(fields) if fields.len() == 1 && fields.values().all(|t| !matches!(t, Ty::Opt(_))) => {
                let (k, t) = fields.iter().next().unwrap();
                Ty::variant(k.clone(), t.clone())
            }
            c => c,
        })
        .collect();
    let cases_all = cases.clone();
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
                let Some(p) = p else { return Ok(general(cases_all)) };
                records.push([(k.clone(), p.clone())].into_iter().collect());
            }
            _ => return Ok(general(cases_all)),
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

/// Cases that are not all keywords and records: one of several types,
/// with the variants among them as one set.
fn general(cases: Vec<Ty>) -> Ty {
    let mut tags = BTreeMap::new();
    let mut out = Vec::new();
    for c in cases {
        match c {
            Ty::Tags(t) => tags.extend(t.iter().map(|(k, p)| (k.clone(), p.clone()))),
            Ty::Union(inner) => out.extend(inner.iter().cloned()),
            c => out.push(c),
        }
    }
    if !tags.is_empty() {
        out.push(Ty::tags(tags));
    }
    if out.len() == 1 { out.pop().unwrap() } else { Ty::Union(Rc::new(out)) }
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
            if let Member::Inline(_, t) | Member::Field(_, t, _) = m {
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
        TypeExpr::Inter(parts) => parts.iter().for_each(|p| vars(p, out)),
        TypeExpr::Update(base, steps) => {
            vars(base, out);
            steps.iter().filter_map(|(_, v)| v.as_ref()).for_each(|v| vars(v, out));
        }
        TypeExpr::Rec(v, body) => {
            let mut inner = BTreeSet::new();
            vars(body, &mut inner);
            inner.remove(v);
            out.extend(inner);
        }
        TypeExpr::Name(..) | TypeExpr::Keyword(_) | TypeExpr::Caps(_) => {}
    }
}
