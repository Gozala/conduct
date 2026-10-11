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
    IsNil,
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
        Op::IsNil,
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
            Op::IsNil => "nil?",
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
            | Op::IsNil
            | Op::Count
            | Op::First
            | Op::Rest
            | Op::Trim
            | Op::ParseJson
            | Op::ToJson
            | Op::JsonItems
            | Op::JsonStr
            | Op::JsonInt
            | Op::JsonBool => 1,
            Op::Assoc => 3,
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
    Nil,
    /// A local, by de Bruijn index: 0 is the innermost binder.
    Local(u32),
    /// Another definition.
    Ref(Hash),
    /// A member of the definition's own cycle of mutually recursive
    /// definitions, by its index in the cycle.
    CycleRef(u32),
    /// A function of `arity` parameters. The last parameter is local 0
    /// in the body.
    Fn(u32, Box<Term>),
    App(Box<Term>, Vec<Term>),
    /// Binds local 0 in the body.
    Let(Box<Term>, Box<Term>),
    If(Box<Term>, Box<Term>, Box<Term>),
    /// Binds local 0 in the then branch.
    IfSome(Box<Term>, Box<Term>, Box<Term>),
    Coalesce(Box<Term>, Box<Term>),
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
}

/// What a `match` clause tests its value against.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Keyword(String),
    Str(String),
    Int(i64),
    /// The value is `nil`.
    Nil,
    /// The value is not `nil`: `some?`.
    Some,
    /// Any value: `_`. At the top of a clause it is the last clause; in
    /// a record pattern it only asks that the field be there.
    Any,
    /// A record whose fields match their patterns, sorted by name.
    Fields(Vec<(String, Pattern)>),
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
            Pattern::Nil => out.push(3),
            Pattern::Some => out.push(4),
            Pattern::Any => out.push(6),
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

    /// Whether the pattern matches every value its fields' reads accept:
    /// `_`, or a record of `_`s.
    pub fn irrefutable(&self) -> bool {
        match self {
            Pattern::Any => true,
            Pattern::Fields(fields) => fields.iter().all(|(_, p)| p.irrefutable()),
            _ => false,
        }
    }

    /// Whether these patterns, without a `_` clause, can cover every value
    /// the match is given: all keywords, or `nil` and `some?`, or one
    /// field, the same in each, whose patterns cover.
    pub fn cover(patterns: &[&Pattern]) -> bool {
        if patterns.iter().all(|p| matches!(p, Pattern::Keyword(_))) || patterns.iter().any(|p| p.irrefutable()) {
            return true;
        }
        if patterns.len() == 2 && patterns.contains(&&Pattern::Nil) && patterns.contains(&&Pattern::Some) {
            return true;
        }
        let Some(Pattern::Fields(first)) = patterns.first() else { return false };
        let [(name, _)] = first.as_slice() else { return false };
        let mut inner = Vec::new();
        for p in patterns {
            match p {
                Pattern::Fields(fs) if fs.len() == 1 && fs[0].0 == *name => inner.push(&fs[0].1),
                _ => return false,
            }
        }
        Pattern::cover(&inner)
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
            Term::App(callee, args) => {
                callee.walk(f);
                args.iter().for_each(|a| a.walk(f));
            }
            Term::Let(a, b) | Term::Coalesce(a, b) | Term::And(a, b) | Term::Or(a, b) => {
                a.walk(f);
                b.walk(f);
            }
            Term::Update(r, _, v) => {
                r.walk(f);
                v.walk(f);
            }
            Term::If(a, b, c) | Term::IfSome(a, b, c) => {
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
            Term::Fn(n, body) => {
                out.push(0x09);
                uint(out, *n as u64);
                body.encode(out, cycle_index);
            }
            Term::App(callee, args) => {
                out.push(0x0a);
                callee.encode(out, cycle_index);
                many(out, args, cycle_index);
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
            Term::IfSome(v, t, e) => {
                out.push(0x0d);
                v.encode(out, cycle_index);
                t.encode(out, cycle_index);
                e.encode(out, cycle_index);
            }
            Term::Coalesce(a, b) => {
                out.push(0x0e);
                a.encode(out, cycle_index);
                b.encode(out, cycle_index);
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
