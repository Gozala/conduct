//! Resolved terms: no names, only de Bruijn indices for locals and
//! hashes for other definitions. A definition's identity is the hash of
//! its term's canonical encoding, so renaming a definition or any of
//! its locals leaves its hash unchanged.

use std::fmt;

/// The identity of a definition: the BLAKE3 hash of its canonical
/// encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    pub fn of(bytes: &[u8]) -> Hash {
        Hash(*blake3::hash(bytes).as_bytes())
    }

    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The first characters of the hash, enough to tell definitions
    /// apart in a listing.
    pub fn short(&self) -> String {
        format!("#{}", &self.hex()[..10])
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short())
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.hex())
    }
}

/// Built-in operations. They are always applied in full; a built-in is
/// not a first-class value in this iteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Quot,
    Rem,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    NotEq,
    Not,
    Str,
    Count,
    First,
    Rest,
    Concat,
    Conj,
    Map,
    Filter,
    Get,
    Assoc,
    Dissoc,
    H,
    Emit,
    Trim,
    Fetch,
    Split,
    ParseJson,
    ToJson,
    JsonGet,
    JsonItems,
    JsonStr,
    JsonInt,
    JsonBool,
    JsonType,
    /// Whether a value is a record with a field: only `match` uses it,
    /// so it has no name in source and is not in [`Op::ALL`].
    HasField,
}

impl Op {
    pub const ALL: &[Op] = &[
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Quot,
        Op::Rem,
        Op::Lt,
        Op::Gt,
        Op::Le,
        Op::Ge,
        Op::Eq,
        Op::NotEq,
        Op::Not,
        Op::Str,
        Op::Count,
        Op::First,
        Op::Rest,
        Op::Concat,
        Op::Conj,
        Op::Map,
        Op::Filter,
        Op::Get,
        Op::Assoc,
        Op::Dissoc,
        Op::H,
        Op::Emit,
        Op::Trim,
        Op::Fetch,
        Op::Split,
        Op::ParseJson,
        Op::ToJson,
        Op::JsonGet,
        Op::JsonItems,
        Op::JsonStr,
        Op::JsonInt,
        Op::JsonBool,
        Op::JsonType,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Quot => "quot",
            Op::Rem => "rem",
            Op::Lt => "<",
            Op::Gt => ">",
            Op::Le => "<=",
            Op::Ge => ">=",
            Op::Eq => "=",
            Op::NotEq => "not=",
            Op::Not => "not",
            Op::Str => "str",
            Op::Count => "count",
            Op::First => "first",
            Op::Rest => "rest",
            Op::Concat => "concat",
            Op::Conj => "conj",
            Op::Map => "map",
            Op::Filter => "filter",
            Op::Get => "get",
            Op::Assoc => "assoc",
            Op::Dissoc => "dissoc",
            Op::H => "h",
            Op::Emit => "emit",
            Op::Trim => "trim",
            Op::Fetch => "fetch",
            Op::Split => "split",
            Op::ParseJson => "parse-json",
            Op::ToJson => "to-json",
            Op::JsonGet => "json-get",
            Op::JsonItems => "json-items",
            Op::JsonStr => "json-str",
            Op::JsonInt => "json-int",
            Op::JsonBool => "json-bool",
            Op::JsonType => "json-type",
            Op::HasField => "has-field?",
        }
    }

    pub fn from_name(name: &str) -> Option<Op> {
        Op::ALL.iter().copied().find(|op| op.name() == name)
    }

    /// The number of arguments, or `None` for `str`, which takes any.
    pub fn arity(self) -> Option<usize> {
        Some(match self {
            Op::Str => return None,
            Op::Not
            | Op::Count
            | Op::Rest
            | Op::Trim
            | Op::ParseJson
            | Op::ToJson
            | Op::JsonItems
            | Op::JsonStr
            | Op::JsonInt
            | Op::JsonBool
            | Op::JsonType => 1,
            Op::Assoc | Op::Get => 3,
            Op::H | Op::Fetch => 4,
            _ => 2,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    Int(i64),
    Str(String),
    Bool(bool),
    /// No value: never written in source, only what an exhaustive
    /// `match` lowers its impossible last case to.
    Nil,
    /// A local, by de Bruijn index: 0 is the innermost binder.
    Local(u32),
    /// Another definition.
    Ref(Hash),
    /// A member of the definition's own cycle of mutually recursive
    /// definitions, by its index in the cycle.
    CycleRef(u32),
    /// A function of one parameter per label: the first is the first
    /// parameter's, the others' follow in order. Parameters are passed
    /// in that order; the last is local 0 in the body.
    Fn(Vec<String>, Box<Term>),
    /// A call: the arguments in the callee's parameter order, and the
    /// labels of all but the first, which a call through a function
    /// value passes without its label.
    App(Box<Term>, Vec<Term>, Vec<String>),
    /// Binds local 0 in the body.
    Let(Box<Term>, Box<Term>),
    If(Box<Term>, Box<Term>, Box<Term>),
    And(Box<Term>, Box<Term>),
    Or(Box<Term>, Box<Term>),
    /// Fields sorted by name, so that field order in the source does not
    /// change a definition's hash.
    Record(Vec<(String, Term)>),
    Field(String, Box<Term>),
    /// A copy of a record with one field set.
    Update(Box<Term>, String, Box<Term>),
    /// Entries in source order, which a view's attributes keep.
    Dict(Vec<(String, Term)>),
    List(Vec<Term>),
    Prim(Op, Vec<Term>),
    Keyword(String),
    /// The scrutinee, each clause's pattern and result, and the result of
    /// the final `_` clause if there is one.
    Match(Box<Term>, Vec<(Pattern, Term)>, Option<Box<Term>>),
    /// A call of a function the host provides, by its import name. Only
    /// a host definition's body has one.
    Host(String, Vec<Term>),
}

/// What a `match` clause tests its value against.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Keyword(String),
    Str(String),
    Int(i64),
    /// Any value: `_`. At the top of a clause it is the last clause; in
    /// a record pattern it only asks that the field be there.
    Any,
    /// A record whose fields match their patterns, sorted by name. With
    /// one field it is also a variant: `{:ok t}`.
    Fields(Vec<(String, Pattern)>),
    /// Any value, bound to a name in the clause's result. The name is
    /// not part of the hash, as a parameter's is not.
    Bind(String),
}

impl Pattern {
    fn encode(&self, out: &mut Vec<u8>) {
        let text = |out: &mut Vec<u8>, s: &str| {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        };
        match self {
            Pattern::Keyword(k) => {
                out.push(0);
                text(out, k);
            }
            Pattern::Str(s) => {
                out.push(1);
                text(out, s);
            }
            Pattern::Int(n) => {
                out.push(2);
                out.extend_from_slice(&n.to_le_bytes());
            }
            Pattern::Any => out.push(6),
            Pattern::Bind(_) => out.push(7),
            Pattern::Fields(fields) => {
                out.push(5);
                out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
                for (name, p) in fields {
                    text(out, name);
                    p.encode(out);
                }
            }
        }
    }

    /// Whether the pattern matches every value: `_` or a name.
    pub fn irrefutable(&self) -> bool {
        matches!(self, Pattern::Any | Pattern::Bind(_))
    }

    /// Whether the pattern matches every record that has its fields: a
    /// record pattern of irrefutable fields.
    pub fn fields_only(&self) -> bool {
        matches!(self, Pattern::Fields(fields) if fields.iter().all(|(_, p)| p.irrefutable()))
    }

    /// The names the pattern binds, in order: the last is local 0 in the
    /// clause's result.
    pub fn binders(&self) -> Vec<String> {
        let mut out = Vec::new();
        fn go(p: &Pattern, out: &mut Vec<String>) {
            match p {
                Pattern::Bind(name) => out.push(name.clone()),
                Pattern::Fields(fields) => fields.iter().for_each(|(_, p)| go(p, out)),
                _ => {}
            }
        }
        go(self, &mut out);
        out
    }

    /// The field path each binder reads its value from, in the order of
    /// [`Pattern::binders`].
    pub fn binder_paths(&self) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        fn go(p: &Pattern, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
            match p {
                Pattern::Bind(_) => out.push(path.clone()),
                Pattern::Fields(fields) => {
                    for (name, p) in fields {
                        path.push(name.clone());
                        go(p, path, out);
                        path.pop();
                    }
                }
                _ => {}
            }
        }
        go(self, &mut Vec::new(), &mut out);
        out
    }

    /// A variant pattern's key and payload pattern: a keyword, or a
    /// record pattern of one field.
    pub fn variant(&self) -> Option<(&str, Option<&Pattern>)> {
        match self {
            Pattern::Keyword(k) => Some((k, None)),
            Pattern::Fields(fs) if fs.len() == 1 => Some((&fs[0].0, Some(&fs[0].1))),
            _ => None,
        }
    }

    /// Whether these patterns, without a `_` clause, can cover every value
    /// the match is given: variants (keywords and
    /// one-field records) whose payload patterns cover, key by key, or a
    /// record pattern of `_`s among others. When every clause names the
    /// same one field, that is a field of a record as much as a variant,
    /// and the same test holds.
    pub fn cover(patterns: &[&Pattern]) -> bool {
        if patterns.iter().any(|p| p.irrefutable()) {
            return true;
        }
        // Otherwise a record pattern of `_`s covers what has its fields,
        // which the type checker then requires of every value.
        let Some(variants) = patterns.iter().map(|p| p.variant()).collect::<Option<Vec<_>>>() else {
            return patterns.iter().any(|p| p.fields_only());
        };
        let mut keys: Vec<&str> = variants.iter().map(|v| v.0).collect();
        keys.dedup();
        keys.iter().all(|k| {
            let payloads: Vec<Option<&Pattern>> = variants.iter().filter(|v| v.0 == *k).map(|v| v.1).collect();
            match payloads.iter().copied().collect::<Option<Vec<&Pattern>>>() {
                Some(inner) => Pattern::cover(&inner),
                // A keyword covers its key; a keyword and a variant of the
                // same name cannot both be.
                None => payloads.iter().all(|p| p.is_none()),
            }
        })
    }
}

impl Term {
    /// Visit every hash this term refers to.
    pub fn refs(&self, out: &mut Vec<Hash>) {
        self.walk(&mut |t| {
            if let Term::Ref(h) = t
                && !out.contains(h)
            {
                out.push(*h);
            }
        });
    }

    pub fn walk(&self, f: &mut dyn FnMut(&Term)) {
        f(self);
        match self {
            Term::Int(_)
            | Term::Str(_)
            | Term::Bool(_)
            | Term::Nil
            | Term::Local(_)
            | Term::Ref(_)
            | Term::Keyword(_)
            | Term::CycleRef(_) => {}
            Term::Host(_, args) => args.iter().for_each(|a| a.walk(f)),
            Term::Match(scrutinee, clauses, default) => {
                scrutinee.walk(f);
                for (_, result) in clauses {
                    result.walk(f);
                }
                if let Some(d) = default {
                    d.walk(f);
                }
            }
            Term::Fn(_, b) | Term::Field(_, b) => b.walk(f),
            Term::App(callee, args, _) => {
                callee.walk(f);
                args.iter().for_each(|a| a.walk(f));
            }
            Term::Let(a, b) | Term::And(a, b) | Term::Or(a, b) => {
                a.walk(f);
                b.walk(f);
            }
            Term::Update(r, _, v) => {
                r.walk(f);
                v.walk(f);
            }
            Term::If(a, b, c) => {
                a.walk(f);
                b.walk(f);
                c.walk(f);
            }
            Term::Record(fields) | Term::Dict(fields) => fields.iter().for_each(|(_, t)| t.walk(f)),
            Term::List(items) | Term::Prim(_, items) => items.iter().for_each(|t| t.walk(f)),
        }
    }

    /// Whether the term mentions `CycleRef`.
    pub fn is_recursive(&self) -> bool {
        let mut found = false;
        self.walk(&mut |t| found |= matches!(t, Term::CycleRef(_)));
        found
    }

    /// The canonical encoding the hash is taken over. Cycle references
    /// are written with `cycle_index`, which lets the caller encode them
    /// all alike while it orders a cycle's members.
    pub fn encode(&self, out: &mut Vec<u8>, cycle_index: &dyn Fn(u32) -> u32) {
        fn uint(out: &mut Vec<u8>, mut n: u64) {
            loop {
                let byte = (n & 0x7f) as u8;
                n >>= 7;
                if n == 0 {
                    out.push(byte);
                    return;
                }
                out.push(byte | 0x80);
            }
        }
        fn text(out: &mut Vec<u8>, s: &str) {
            uint(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        fn many(out: &mut Vec<u8>, items: &[Term], ci: &dyn Fn(u32) -> u32) {
            uint(out, items.len() as u64);
            items.iter().for_each(|t| t.encode(out, ci));
        }
        fn entries(out: &mut Vec<u8>, items: &[(String, Term)], ci: &dyn Fn(u32) -> u32) {
            uint(out, items.len() as u64);
            for (k, t) in items {
                text(out, k);
                t.encode(out, ci);
            }
        }
        match self {
            Term::Int(n) => {
                out.push(0x01);
                // Zigzag, so that small negative numbers stay short.
                uint(out, ((n << 1) ^ (n >> 63)) as u64);
            }
            Term::Str(s) => {
                out.push(0x02);
                text(out, s);
            }
            Term::Bool(b) => out.push(if *b { 0x04 } else { 0x03 }),
            Term::Nil => out.push(0x05),
            Term::Local(i) => {
                out.push(0x06);
                uint(out, *i as u64);
            }
            Term::Ref(h) => {
                out.push(0x07);
                out.extend_from_slice(&h.0);
            }
            Term::CycleRef(i) => {
                out.push(0x08);
                uint(out, cycle_index(*i) as u64);
            }
            Term::Fn(labels, body) => {
                out.push(0x09);
                uint(out, labels.len() as u64);
                labels.iter().for_each(|l| text(out, l));
                body.encode(out, cycle_index);
            }
            Term::App(callee, args, labels) => {
                out.push(0x0a);
                callee.encode(out, cycle_index);
                many(out, args, cycle_index);
                uint(out, labels.len() as u64);
                labels.iter().for_each(|l| text(out, l));
            }
            Term::Let(v, b) => {
                out.push(0x0b);
                v.encode(out, cycle_index);
                b.encode(out, cycle_index);
            }
            Term::If(c, t, e) => {
                out.push(0x0c);
                c.encode(out, cycle_index);
                t.encode(out, cycle_index);
                e.encode(out, cycle_index);
            }
            Term::And(a, b) => {
                out.push(0x0f);
                a.encode(out, cycle_index);
                b.encode(out, cycle_index);
            }
            Term::Or(a, b) => {
                out.push(0x10);
                a.encode(out, cycle_index);
                b.encode(out, cycle_index);
            }
            Term::Record(fields) => {
                out.push(0x11);
                entries(out, fields, cycle_index);
            }
            Term::Field(name, t) => {
                out.push(0x12);
                text(out, name);
                t.encode(out, cycle_index);
            }
            Term::Update(r, name, v) => {
                out.push(0x16);
                text(out, name);
                r.encode(out, cycle_index);
                v.encode(out, cycle_index);
            }
            Term::Dict(items) => {
                out.push(0x13);
                entries(out, items, cycle_index);
            }
            Term::List(items) => {
                out.push(0x14);
                many(out, items, cycle_index);
            }
            Term::Prim(op, args) => {
                out.push(0x15);
                text(out, op.name());
                many(out, args, cycle_index);
            }
            Term::Keyword(k) => {
                out.push(0x17);
                text(out, k);
            }
            Term::Host(name, args) => {
                out.push(0x19);
                text(out, name);
                many(out, args, cycle_index);
            }
            Term::Match(scrutinee, clauses, default) => {
                out.push(0x18);
                scrutinee.encode(out, cycle_index);
                uint(out, clauses.len() as u64);
                for (pattern, result) in clauses {
                    pattern.encode(out);
                    result.encode(out, cycle_index);
                }
                match default {
                    Some(d) => {
                        out.push(1);
                        d.encode(out, cycle_index);
                    }
                    None => out.push(0),
                }
            }
        }
    }
}
