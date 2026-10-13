//! Resolved terms: no names, only de Bruijn indices for locals and
//! hashes for other definitions. A definition's identity is the hash of
//! its term's canonical encoding, so renaming a definition or any of
//! its locals leaves its hash unchanged.

use std::collections::BTreeMap;
use std::fmt;

/// The identity of a definition: the BLAKE3 hash of its canonical
/// encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    pub fn of(bytes: &[u8]) -> Hash {
        Hash(*blake3::hash(bytes).as_bytes())
    }

    /// A hash from its hex, as [`Hash::hex`] writes it.
    pub fn from_hex(hex: &str) -> Option<Hash> {
        if hex.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(Hash(out))
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
    /// Perform a task with an environment on top of the others: what
    /// `(perform t with: env)` compiles to. Not in source either.
    WithEnv,
    /// A list's item at an index, and a list without its first items:
    /// what a list pattern reads. Not in source.
    Nth,
    Drop,
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
            Op::WithEnv => "with-env",
            Op::Nth => "nth",
            Op::Drop => "drop",
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
    /// Work not done yet: a task whose performing evaluates the body.
    Task(Box<Term>),
    /// Perform a task: what `!` after a name means.
    Perform(Box<Term>),
    /// A task that performs another in an environment: each capability
    /// it provides, with an implementation of each of its operations.
    Handle(Box<Term>, Vec<(String, Vec<(String, Term)>)>),
}

/// What a `match` clause tests its value against.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Keyword(String),
    Str(String),
    Int(i64),
    Bool(bool),
    /// Any value: `_`. At the top of a clause it is the last clause; in
    /// a record pattern it only asks that the field be there.
    Any,
    /// A record whose fields match their patterns, sorted by name. With
    /// one field it is also a variant: `{:ok t}`.
    Fields(Vec<(String, Pattern)>),
    /// Any value, bound to a name in the clause's result. The name is
    /// not part of the hash, as a parameter's is not.
    Bind(String),
    /// A list: patterns for its first items and, after `...`, a pattern
    /// (a name or `_`) for the list of the rest. Without one, the list
    /// has exactly as many items as there are patterns.
    List(Vec<Pattern>, Option<Box<Pattern>>),
    /// A pattern that also binds the whole value it matches: a record
    /// pattern's `rest ...`, `{action: :toggle command ...}`.
    As(String, Box<Pattern>),
}

/// One step of the way to what a pattern binds: a record's field, a
/// list's item, or a list without its first items.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Field(String),
    Item(usize),
    Drop(usize),
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
            Pattern::Bool(b) => {
                out.push(3);
                out.push(*b as u8);
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
            Pattern::As(_, p) => {
                out.push(9);
                p.encode(out);
            }
            Pattern::List(items, rest) => {
                out.push(8);
                out.extend_from_slice(&(items.len() as u32).to_le_bytes());
                items.iter().for_each(|p| p.encode(out));
                match rest {
                    Some(r) => {
                        out.push(1);
                        r.encode(out);
                    }
                    None => out.push(0),
                }
            }
        }
    }

    /// What the pattern tests, without a name for the whole value.
    pub fn core(&self) -> &Pattern {
        match self {
            Pattern::As(_, p) => p.core(),
            p => p,
        }
    }

    /// Whether the pattern matches every value: `_` or a name.
    pub fn irrefutable(&self) -> bool {
        matches!(self.core(), Pattern::Any | Pattern::Bind(_))
    }

    /// Whether the pattern matches every record that has its fields: a
    /// record pattern of irrefutable fields.
    pub fn fields_only(&self) -> bool {
        matches!(self.core(), Pattern::Fields(fields) if fields.iter().all(|(_, p)| p.irrefutable()))
    }

    /// The names the pattern binds, in order: the last is local 0 in the
    /// clause's result.
    pub fn binders(&self) -> Vec<String> {
        let mut out = Vec::new();
        fn go(p: &Pattern, out: &mut Vec<String>) {
            match p {
                Pattern::Bind(name) => out.push(name.clone()),
                Pattern::As(name, p) => {
                    out.push(name.clone());
                    go(p, out);
                }
                Pattern::Fields(fields) => fields.iter().for_each(|(_, p)| go(p, out)),
                Pattern::List(items, rest) => {
                    items.iter().for_each(|p| go(p, out));
                    if let Some(r) = rest {
                        go(r, out);
                    }
                }
                _ => {}
            }
        }
        go(self, &mut out);
        out
    }

    /// The way to each binder's value, in the order of
    /// [`Pattern::binders`].
    pub fn binder_paths(&self) -> Vec<Vec<Step>> {
        let mut out = Vec::new();
        fn go(p: &Pattern, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>) {
            let at = |step: Step, p: &Pattern, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>| {
                path.push(step);
                go(p, path, out);
                path.pop();
            };
            match p {
                Pattern::Bind(_) => out.push(path.clone()),
                Pattern::As(_, p) => {
                    out.push(path.clone());
                    go(p, path, out);
                }
                Pattern::Fields(fields) => {
                    for (name, p) in fields {
                        at(Step::Field(name.clone()), p, path, out);
                    }
                }
                Pattern::List(items, rest) => {
                    for (i, p) in items.iter().enumerate() {
                        at(Step::Item(i), p, path, out);
                    }
                    if let Some(r) = rest {
                        at(Step::Drop(items.len()), r, path, out);
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
        match self.core() {
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
        let patterns: &[&Pattern] = &patterns.iter().map(|p| p.core()).collect::<Vec<_>>();
        if patterns.iter().any(|p| p.irrefutable()) {
            return true;
        }
        if patterns.iter().all(|p| matches!(p, Pattern::List(..))) {
            return Pattern::cover_lists(patterns);
        }
        if patterns.iter().all(|p| matches!(p, Pattern::Bool(_))) {
            return [true, false].iter().all(|b| patterns.contains(&&Pattern::Bool(*b)));
        }
        if let Some(keys) = argument_columns(patterns) {
            let rows = patterns
                .iter()
                .map(|p| match p {
                    Pattern::Fields(f) => {
                        keys.iter().map(|k| f.iter().find(|(l, _)| l == k).unwrap().1.clone()).collect()
                    }
                    _ => unreachable!(),
                })
                .collect();
            let sigs = keys
                .iter()
                .map(|k| {
                    let column: Vec<&Pattern> = patterns
                        .iter()
                        .filter_map(|p| match p {
                            Pattern::Fields(f) => f.iter().find(|(l, _)| l == k).map(|(_, p)| p),
                            _ => None,
                        })
                        .collect();
                    Sig::of(&column)
                })
                .collect();
            return Pattern::exhaustive(rows, sigs);
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

/// What a column of patterns lets a value be, when it decides whether
/// they cover: the variants they name, when none of them matches
/// anything, each with what its payload may be; or anything.
#[derive(Clone, Debug, PartialEq)]
pub enum Sig {
    Open,
    Tags(BTreeMap<String, Option<Sig>>),
    /// A function's arguments: each parameter's.
    Fields(BTreeMap<String, Sig>),
    /// `true` or `false`.
    Bool,
}

impl Sig {
    /// What the arguments of a function with these clauses may be.
    pub fn of_clauses(patterns: &[&Pattern]) -> Sig {
        let Some(keys) = argument_columns(patterns) else { return Sig::of(patterns) };
        Sig::Fields(
            keys.iter()
                .map(|k| {
                    let column: Vec<&Pattern> = patterns
                        .iter()
                        .filter_map(|p| match p {
                            Pattern::Fields(f) => f.iter().find(|(l, _)| l == k).map(|(_, p)| p),
                            _ => None,
                        })
                        .collect();
                    (k.clone(), Sig::of(&column))
                })
                .collect(),
        )
    }

    pub fn of(patterns: &[&Pattern]) -> Sig {
        let patterns: &[&Pattern] = &patterns.iter().map(|p| p.core()).collect::<Vec<_>>();
        if patterns.is_empty() || patterns.iter().any(|p| p.irrefutable()) {
            return Sig::Open;
        }
        if patterns.iter().all(|p| matches!(p, Pattern::Bool(_))) {
            return Sig::Bool;
        }
        let Some(variants) = patterns.iter().map(|p| p.variant()).collect::<Option<Vec<_>>>() else {
            return Sig::Open;
        };
        let mut tags = BTreeMap::new();
        for (k, _) in &variants {
            let payloads: Vec<&Pattern> = variants.iter().filter(|v| v.0 == *k).filter_map(|v| v.1).collect();
            let keyword = variants.iter().any(|v| v.0 == *k && v.1.is_none());
            // A keyword and a variant of one key are different values.
            if keyword && !payloads.is_empty() {
                return Sig::Open;
            }
            tags.insert(k.to_string(), (!keyword).then(|| Sig::of(&payloads)));
        }
        Sig::Tags(tags)
    }
}

/// The key of a function's `i`th argument in the record its clauses
/// match.
pub fn argument_key(i: usize) -> String {
    format!("conduct.argument/{i:03}")
}

/// Whether patterns all read a function's arguments: the record its
/// clauses match, when it takes more than one.
pub fn argument_columns(patterns: &[&Pattern]) -> Option<Vec<String>> {
    let Some(Pattern::Fields(first)) = patterns.first() else { return None };
    let keys: Vec<String> = first.iter().map(|(k, _)| k.clone()).collect();
    if keys.len() < 2 || !keys.iter().all(|k| k.starts_with("conduct.argument/")) {
        return None;
    }
    let same = |p: &&Pattern| matches!(p, Pattern::Fields(f) if f.iter().map(|(k, _)| k).eq(keys.iter()));
    patterns.iter().all(same).then_some(keys)
}

impl Pattern {
    /// Whether rows of patterns, one per column, leave no values
    /// unmatched (Maranget's usefulness, for exhaustiveness), each column
    /// holding what its [`Sig`] says: the variants of `Tags`, which the
    /// type checker requires of a function's arguments (see `cover_type`),
    /// or anything.
    pub fn exhaustive(rows: Vec<Vec<Pattern>>, sigs: Vec<Sig>) -> bool {
        // A name for the whole value tests nothing.
        let rows: Vec<Vec<Pattern>> = rows
            .into_iter()
            .map(|mut r| {
                if let Some(head) = r.first_mut() {
                    *head = head.core().clone();
                }
                r
            })
            .collect();
        let Some(first) = rows.first() else { return false };
        if first.is_empty() {
            return true;
        }
        let tail = |row: &[Pattern]| row[1..].to_vec();
        let heads: Vec<&Pattern> = rows.iter().map(|r| &r[0]).collect();
        let default = || {
            Pattern::exhaustive(
                rows.iter().filter(|r| r[0].irrefutable()).map(|r| tail(r)).collect(),
                sigs[1..].to_vec(),
            )
        };
        let refutable: Vec<&&Pattern> = heads.iter().filter(|p| !p.irrefutable()).collect();
        if let Sig::Tags(tags) = &sigs[0] {
            // Every variant the column may hold, whether a row names it
            // or not.
            return tags.iter().all(|(k, payload)| {
                let rows = rows
                    .iter()
                    .filter_map(|r| {
                        let p = match r[0].variant() {
                            // A keyword row matches the keyword, a variant
                            // row the variant.
                            Some((key, p)) if key == k && p.is_some() == payload.is_some() => {
                                p.cloned().unwrap_or(Pattern::Any)
                            }
                            Some(_) => return None,
                            None if r[0].irrefutable() => Pattern::Any,
                            None => return None,
                        };
                        Some(std::iter::once(p).chain(tail(r)).collect())
                    })
                    .collect();
                let inner = payload.clone().unwrap_or(Sig::Open);
                Pattern::exhaustive(rows, std::iter::once(inner).chain(sigs[1..].iter().cloned()).collect())
            });
        }
        if refutable.is_empty() {
            return default();
        }
        if refutable.iter().all(|p| matches!(p, Pattern::Bool(_))) {
            // A Bool is one of two; where the column may hold anything,
            // the rows that match anything cover the rest.
            let bools = [true, false].iter().all(|b| {
                let rows =
                    rows.iter().filter(|r| r[0].irrefutable() || r[0] == Pattern::Bool(*b)).map(|r| tail(r)).collect();
                Pattern::exhaustive(rows, sigs[1..].to_vec())
            });
            return bools && (sigs[0] == Sig::Bool || default());
        }
        if refutable.iter().all(|p| matches!(p, Pattern::List(..))) {
            // Every length up to one past the longest pattern, and those
            // past it behave as it does. Items may be anything.
            let longest =
                refutable.iter().map(|p| if let Pattern::List(i, _) = p { i.len() } else { 0 }).max().unwrap();
            return (0..=longest + 1).all(|n| {
                let rows = rows
                    .iter()
                    .filter_map(|r| {
                        let items: Vec<Pattern> = match &r[0] {
                            Pattern::List(items, rest) if items.len() == n || rest.is_some() && items.len() <= n => {
                                items
                                    .iter()
                                    .cloned()
                                    .chain(std::iter::repeat_n(Pattern::Any, n - items.len()))
                                    .collect()
                            }
                            p if p.irrefutable() => vec![Pattern::Any; n],
                            _ => return None,
                        };
                        Some(items.into_iter().chain(tail(r)).collect())
                    })
                    .collect();
                Pattern::exhaustive(rows, std::iter::repeat_n(Sig::Open, n).chain(sigs[1..].iter().cloned()).collect())
            });
        }
        if let Some(keys) = refutable.iter().map(|p| p.variant().map(|v| v.0.to_string())).collect::<Option<Vec<_>>>() {
            // An open column: the variants named, and anything else
            // through the rows that match anything.
            let mut keys = keys;
            keys.sort();
            keys.dedup();
            let open: BTreeMap<String, Option<Sig>> = keys.into_iter().map(|k| (k, Some(Sig::Open))).collect();
            let mut with_named = sigs.clone();
            with_named[0] = Sig::Tags(open);
            return Pattern::exhaustive(rows.clone(), with_named) && default();
        }
        default()
    }

    /// Whether list patterns cover every list: every length has a
    /// pattern whose items are all irrefutable, and so does every length
    /// past the longest, through a `...`.
    fn cover_lists(patterns: &[&Pattern]) -> bool {
        let shapes: Vec<(usize, bool)> = patterns
            .iter()
            .filter_map(|p| match p {
                Pattern::List(items, rest) if items.iter().all(Pattern::irrefutable) => {
                    Some((items.len(), rest.is_some()))
                }
                _ => None,
            })
            .collect();
        let Some(open) = shapes.iter().filter(|(_, r)| *r).map(|(n, _)| *n).min() else {
            return false;
        };
        (0..open).all(|n| shapes.contains(&(n, false)))
    }

    /// Whether every value `self` matches, `other` matches too.
    pub fn narrower(&self, other: &Pattern) -> bool {
        use Pattern::*;
        match (self.core(), other.core()) {
            (_, Any | Bind(_)) => true,
            (Any | Bind(_), _) => false,
            (Keyword(a), Keyword(b)) | (Str(a), Str(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            // A record pattern asks for its fields: one that asks for
            // more, each as narrowly, is narrower.
            (Fields(a), Fields(b)) => b.iter().all(|(k, pb)| a.iter().any(|(l, pa)| l == k && pa.narrower(pb))),
            (List(ai, ar), List(bi, br)) => {
                let bt = |i: usize| bi.get(i).or(br.as_deref());
                match (ar, br) {
                    (Some(_), Some(_)) => {
                        bi.len() <= ai.len() && ai.iter().enumerate().all(|(i, p)| p.narrower(bt(i).unwrap()))
                    }
                    (Some(_), None) => false,
                    (None, _) => {
                        (br.is_some() && bi.len() <= ai.len() || bi.len() == ai.len())
                            && ai.iter().enumerate().all(|(i, p)| p.narrower(bt(i).unwrap()))
                    }
                }
            }
            _ => false,
        }
    }

    /// Whether some value matches both, where values are what `sig`
    /// says.
    pub fn overlaps(&self, other: &Pattern, sig: &Sig) -> bool {
        use Pattern::*;
        let open = Sig::Open;
        let field = |k: &str| match sig {
            Sig::Fields(fields) => fields.get(k).unwrap_or(&open),
            Sig::Tags(tags) => tags.get(k).and_then(|p| p.as_ref()).unwrap_or(&open),
            Sig::Open | Sig::Bool => &open,
        };
        match (self.core(), other.core()) {
            (Any | Bind(_), _) | (_, Any | Bind(_)) => true,
            (Keyword(a), Keyword(b)) | (Str(a), Str(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            // Where only the variants named may come, variants with
            // different keys are different values; anywhere else a record
            // may have both fields.
            (Fields(a), Fields(b)) if a.len() == 1 && b.len() == 1 && a[0].0 != b[0].0 => !matches!(sig, Sig::Tags(_)),
            (Fields(a), Fields(b)) => {
                a.iter().all(|(k, pa)| b.iter().find(|(l, _)| l == k).is_none_or(|(_, pb)| pa.overlaps(pb, field(k))))
            }
            (List(ai, ar), List(bi, br)) => {
                let n = match (ar, br) {
                    (None, None) if ai.len() != bi.len() => return false,
                    (None, Some(_)) if ai.len() < bi.len() => return false,
                    (Some(_), None) if bi.len() < ai.len() => return false,
                    (None, _) => ai.len(),
                    (_, None) => bi.len(),
                    _ => ai.len().max(bi.len()),
                };
                let at = |i: usize| ai.get(i).or(ar.as_deref());
                let bt = |i: usize| bi.get(i).or(br.as_deref());
                (0..n).all(|i| at(i).unwrap().overlaps(bt(i).unwrap(), &Sig::Open))
            }
            _ => false,
        }
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
            Term::Fn(_, b) | Term::Field(_, b) | Term::Task(b) | Term::Perform(b) => b.walk(f),
            Term::Handle(t, caps) => {
                t.walk(f);
                caps.iter().flat_map(|(_, ops)| ops).for_each(|(_, i)| i.walk(f));
            }
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
            Term::Task(body) => {
                out.push(0x1a);
                body.encode(out, cycle_index);
            }
            Term::Perform(t) => {
                out.push(0x1b);
                t.encode(out, cycle_index);
            }
            Term::Handle(t, caps) => {
                out.push(0x1c);
                t.encode(out, cycle_index);
                uint(out, caps.len() as u64);
                for (cap, ops) in caps {
                    text(out, cap);
                    entries(out, ops, cycle_index);
                }
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
