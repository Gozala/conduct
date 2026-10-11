//! Compilation of definitions to a core Wasm module.
//!
//! The compiler writes WebAssembly text and assembles it with the `wat`
//! crate. Every value is an `i64` (see `runtime.wat` for the encoding),
//! so one function type per arity covers every function: a function of
//! `n` parameters is `(env i64, p0 i64, ..) -> i64`, where `env` is the
//! closure it was called through.
//!
//! A top-level function compiles to a Wasm function plus a closure
//! object in static data, so it can be called directly or passed as a
//! value. Any other top-level definition compiles to a function of no
//! arguments that computes its value; definitions are pure, so
//! computing one again is harmless.

use crate::codebase::Codebase;
use crate::term::{Hash, Op, Term};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

pub const NIL: i64 = 0x02;
pub const FALSE: i64 = 0x0a;
pub const TRUE: i64 = 0x12;

pub const KIND_STR: u32 = 1;
pub const KIND_LIST: u32 = 2;
pub const KIND_RECORD: u32 = 3;
pub const KIND_DICT: u32 = 4;
pub const KIND_CLOSURE: u32 = 5;
pub const KIND_VIEW: u32 = 6;
pub const KIND_EMIT: u32 = 7;

const RUNTIME: &str = include_str!("runtime.wat");
const RUNTIME_DATA: &[u8] = b"truefalse[] <value>";
/// Addresses below this hold scratch space, such as the area that
/// canonical ABI results are returned through.
const STATIC_START: u32 = 1024;

pub fn int(n: i64) -> i64 {
    (n << 1) | 1
}

/// The id a field name is stored under in a record. It depends only on
/// the name, so records built by different modules agree.
pub fn field_id(name: &str) -> i64 {
    let h = blake3::hash(name.as_bytes());
    let b = h.as_bytes();
    (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) & 0x7fff_ffff) as i64
}

#[derive(Debug)]
pub struct CodegenError(pub String);

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CodegenError {}

/// Where a local lives inside the function being generated.
#[derive(Clone)]
enum Slot {
    Local(String),
    /// The `k`th free variable of the closure being run.
    Free(usize),
    /// A local of an enclosing function that this one did not capture.
    Absent,
}

/// The function currently being generated.
struct Frame {
    locals: Vec<String>,
}

impl Frame {
    fn local(&mut self) -> String {
        let name = format!("$t{}", self.locals.len());
        self.locals.push(name.clone());
        name
    }
}

/// Builds one module: the runtime, the definitions it needs, and
/// whatever imports, functions, and exports a caller adds.
pub struct Builder<'a> {
    cb: &'a Codebase,
    imports: Vec<String>,
    funcs: Vec<String>,
    /// Function names in table order.
    table: Vec<String>,
    data: Vec<(u32, Vec<u8>)>,
    next_addr: u32,
    strings: HashMap<String, u32>,
    /// Definitions compiled or queued, with their static closure
    /// address if they are functions.
    defs: HashMap<Hash, Option<u32>>,
    queue: Vec<Hash>,
    lambdas: usize,
    max_arity: usize,
    exports: Vec<String>,
    rtdata: u32,
}

impl<'a> Builder<'a> {
    pub fn new(cb: &'a Codebase) -> Self {
        let mut b = Builder {
            cb,
            imports: vec![],
            funcs: vec![],
            table: vec![],
            data: vec![],
            next_addr: STATIC_START,
            strings: HashMap::new(),
            defs: HashMap::new(),
            queue: vec![],
            lambdas: 0,
            max_arity: 4,
            exports: vec![],
            rtdata: 0,
        };
        b.rtdata = b.static_bytes(RUNTIME_DATA.to_vec());
        b
    }

    pub fn add_import(&mut self, text: impl Into<String>) {
        self.imports.push(text.into());
    }

    pub fn add_func(&mut self, text: impl Into<String>) {
        self.funcs.push(text.into());
    }

    pub fn add_export(&mut self, text: impl Into<String>) {
        self.exports.push(text.into());
    }

    fn static_bytes(&mut self, bytes: Vec<u8>) -> u32 {
        let addr = self.next_addr;
        self.next_addr = (addr + bytes.len() as u32 + 7) & !7;
        self.data.push((addr, bytes));
        addr
    }

    /// A string object in static data, as a value.
    pub fn static_str(&mut self, s: &str) -> u32 {
        if let Some(addr) = self.strings.get(s) {
            return *addr;
        }
        let mut bytes = KIND_STR.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
        let addr = self.static_bytes(bytes);
        self.strings.insert(s.to_string(), addr);
        addr
    }

    fn def_name(hash: &Hash) -> String {
        format!("$d_{}", &hash.hex()[..16])
    }

    /// Queue a definition (and so everything it reaches) for
    /// compilation, returning its static closure if it is a function.
    fn require(&mut self, hash: Hash) -> Result<Option<u32>, CodegenError> {
        if let Some(addr) = self.defs.get(&hash) {
            return Ok(*addr);
        }
        let def = self
            .cb
            .defs
            .get(&hash)
            .ok_or_else(|| CodegenError(format!("the codebase has no definition {}", hash.short())))?;
        let addr = match &def.term {
            Term::Fn(arity, _) => {
                let index = self.table.len() as u32;
                self.table.push(Self::def_name(&hash));
                self.max_arity = self.max_arity.max(*arity as usize);
                let mut bytes = KIND_CLOSURE.to_le_bytes().to_vec();
                bytes.extend_from_slice(&0u32.to_le_bytes());
                bytes.extend_from_slice(&index.to_le_bytes());
                bytes.extend_from_slice(&arity.to_le_bytes());
                Some(self.static_bytes(bytes))
            }
            _ => None,
        };
        self.defs.insert(hash, addr);
        self.queue.push(hash);
        Ok(addr)
    }

    /// A WAT expression for the value of a definition.
    pub fn value_of(&mut self, hash: Hash) -> Result<String, CodegenError> {
        Ok(match self.require(hash)? {
            Some(addr) => format!("(i64.const {addr})"),
            None => format!("(call {})", Self::def_name(&hash)),
        })
    }

    /// A WAT expression applying a definition to argument expressions.
    pub fn call(&mut self, hash: Hash, args: &[String]) -> Result<String, CodegenError> {
        let arity = match &self.cb.defs[&hash].term {
            Term::Fn(arity, _) => Some(*arity as usize),
            _ => None,
        };
        if arity == Some(args.len()) {
            self.require(hash)?;
            return Ok(format!("(call {} (i64.const {NIL}) {})", Self::def_name(&hash), args.join(" ")));
        }
        let f = self.value_of(hash)?;
        self.max_arity = self.max_arity.max(args.len());
        Ok(format!("(call $apply{} {f} {})", args.len(), args.join(" ")))
    }

    fn compile_queued(&mut self) -> Result<(), CodegenError> {
        while let Some(hash) = self.queue.pop() {
            let def = &self.cb.defs[&hash];
            let term = def.term.clone();
            let cycle = def.cycle.clone();
            let name = Self::def_name(&hash);
            let mut frame = Frame { locals: vec![] };
            let text = match &term {
                Term::Fn(arity, body) => {
                    let mut env: Vec<Slot> = (0..*arity).map(|i| Slot::Local(format!("$p{i}"))).collect();
                    let body = self.expr(body, &mut env, &mut frame, &cycle)?;
                    let params: String = (0..*arity).map(|i| format!(" (param $p{i} i64)")).collect();
                    format!("  (func {name} (param $env i64){params} (result i64){}\n    {body})", locals(&frame))
                }
                other => {
                    let body = self.expr(other, &mut Vec::new(), &mut frame, &cycle)?;
                    format!("  (func {name} (result i64){}\n    {body})", locals(&frame))
                }
            };
            self.funcs.push(text);
        }
        Ok(())
    }

    fn expr(
        &mut self,
        term: &Term,
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
    ) -> Result<String, CodegenError> {
        Ok(match term {
            Term::Int(n) => format!("(i64.const {})", int(*n)),
            Term::Bool(b) => format!("(i64.const {})", if *b { TRUE } else { FALSE }),
            Term::Nil => format!("(i64.const {NIL})"),
            Term::Str(s) => format!("(i64.const {})", self.static_str(s)),
            Term::Local(i) => match &env[env.len() - 1 - *i as usize] {
                Slot::Local(name) => format!("(local.get {name})"),
                Slot::Free(k) => format!("(i64.load offset={} (i32.wrap_i64 (local.get $env)))", 16 + 8 * k),
                Slot::Absent => return Err(CodegenError("internal: a local was not captured".into())),
            },
            Term::Ref(h) => self.value_of(*h)?,
            Term::CycleRef(i) => self.value_of(cycle[*i as usize])?,
            Term::Fn(arity, body) => self.lambda(*arity, body, env, frame, cycle)?,
            Term::App(callee, args) => {
                let args: Vec<String> =
                    args.iter().map(|a| self.expr(a, env, frame, cycle)).collect::<Result<_, _>>()?;
                match &**callee {
                    Term::Ref(h) => self.call(*h, &args)?,
                    Term::CycleRef(i) => self.call(cycle[*i as usize], &args)?,
                    other => {
                        let f = self.expr(other, env, frame, cycle)?;
                        self.max_arity = self.max_arity.max(args.len());
                        format!("(call $apply{} {f} {})", args.len(), args.join(" "))
                    }
                }
            }
            Term::Let(value, body) => {
                let v = self.expr(value, env, frame, cycle)?;
                let local = frame.local();
                env.push(Slot::Local(local.clone()));
                let body = self.expr(body, env, frame, cycle);
                env.pop();
                format!("(block (result i64) (local.set {local} {v}) {})", body?)
            }
            Term::If(c, t, e) => {
                let c = self.expr(c, env, frame, cycle)?;
                let t = self.expr(t, env, frame, cycle)?;
                let e = self.expr(e, env, frame, cycle)?;
                format!("(if (result i64) (i64.eq {c} (i64.const {TRUE})) (then {t}) (else {e}))")
            }
            Term::IfSome(value, then, otherwise) => {
                let v = self.expr(value, env, frame, cycle)?;
                let local = frame.local();
                env.push(Slot::Local(local.clone()));
                let t = self.expr(then, env, frame, cycle);
                env.pop();
                let e = self.expr(otherwise, env, frame, cycle)?;
                format!(
                    "(block (result i64) (local.set {local} {v}) (if (result i64) (i64.ne (local.get {local}) (i64.const {NIL})) (then {}) (else {e})))",
                    t?
                )
            }
            Term::Coalesce(a, b) => {
                let a = self.expr(a, env, frame, cycle)?;
                let b = self.expr(b, env, frame, cycle)?;
                let local = frame.local();
                format!(
                    "(block (result i64) (local.set {local} {a}) (if (result i64) (i64.ne (local.get {local}) (i64.const {NIL})) (then (local.get {local})) (else {b})))"
                )
            }
            Term::And(a, b) => {
                let a = self.expr(a, env, frame, cycle)?;
                let b = self.expr(b, env, frame, cycle)?;
                format!("(if (result i64) (i64.eq {a} (i64.const {TRUE})) (then {b}) (else (i64.const {FALSE})))")
            }
            Term::Or(a, b) => {
                let a = self.expr(a, env, frame, cycle)?;
                let b = self.expr(b, env, frame, cycle)?;
                format!("(if (result i64) (i64.eq {a} (i64.const {TRUE})) (then (i64.const {TRUE})) (else {b}))")
            }
            Term::Record(fields) => {
                let mut sorted: Vec<(i64, String)> = Vec::new();
                for (name, t) in fields {
                    sorted.push((field_id(name), self.expr(t, env, frame, cycle)?));
                }
                sorted.sort_by_key(|f| f.0);
                let p = frame.local32();
                let mut out = format!(
                    "(block (result i64) (local.set {p} (call $obj (i32.const {KIND_RECORD}) (i32.const {}) (i32.const {})))",
                    sorted.len(),
                    16 * sorted.len()
                );
                for (i, (id, v)) in sorted.iter().enumerate() {
                    let _ = write!(
                        out,
                        " (i64.store offset={} (local.get {p}) (i64.const {id})) (i64.store offset={} (local.get {p}) {v})",
                        8 + 16 * i,
                        16 + 16 * i
                    );
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::Field(name, record) => {
                let r = self.expr(record, env, frame, cycle)?;
                format!("(call $rec_get {r} (i64.const {}))", field_id(name))
            }
            Term::Dict(entries) => {
                let mut parts = Vec::new();
                for (k, t) in entries {
                    let key = self.static_str(k);
                    parts.push((key, self.expr(t, env, frame, cycle)?));
                }
                let p = frame.local32();
                let mut out =
                    format!("(block (result i64) (local.set {p} (call $dict_new (i32.const {})))", parts.len());
                for (i, (k, v)) in parts.iter().enumerate() {
                    let _ = write!(out, " (call $dict_put (local.get {p}) (i32.const {i}) (i64.const {k}) {v})");
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::List(items) => {
                let parts: Vec<String> =
                    items.iter().map(|t| self.expr(t, env, frame, cycle)).collect::<Result<_, _>>()?;
                let p = frame.local32();
                let mut out =
                    format!("(block (result i64) (local.set {p} (call $list_new (i32.const {})))", parts.len());
                for (i, v) in parts.iter().enumerate() {
                    let _ = write!(out, " (call $list_set (local.get {p}) (i32.const {i}) {v})");
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::Prim(op, args) => {
                let a: Vec<String> = args.iter().map(|t| self.expr(t, env, frame, cycle)).collect::<Result<_, _>>()?;
                prim(*op, &a, self)
            }
        })
    }

    fn lambda(
        &mut self,
        arity: u32,
        body: &Term,
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
    ) -> Result<String, CodegenError> {
        // The locals of the enclosing function this lambda mentions, as
        // de Bruijn indices from outside it.
        let mut free = BTreeSet::new();
        free_locals(body, arity, &mut free);
        let free: Vec<u32> = free.into_iter().collect();
        let mut inner: Vec<Slot> = vec![Slot::Absent; env.len()];
        for (k, d) in free.iter().enumerate() {
            let pos = env.len() - 1 - *d as usize;
            inner[pos] = Slot::Free(k);
        }
        inner.extend((0..arity).map(|i| Slot::Local(format!("$p{i}"))));
        let name = format!("$lambda{}", self.lambdas);
        self.lambdas += 1;
        let index = self.table.len();
        self.table.push(name.clone());
        self.max_arity = self.max_arity.max(arity as usize);
        let mut inner_frame = Frame { locals: vec![] };
        let body = self.expr(body, &mut inner, &mut inner_frame, cycle)?;
        let params: String = (0..arity).map(|i| format!(" (param $p{i} i64)")).collect();
        self.funcs
            .push(format!("  (func {name} (param $env i64){params} (result i64){}\n    {body})", locals(&inner_frame)));
        let captured: Vec<String> =
            free.iter().map(|d| self.expr(&Term::Local(*d), env, frame, cycle)).collect::<Result<_, _>>()?;
        let p = frame.local32();
        let mut out = format!(
            "(block (result i64) (local.set {p} (call $clo_new (i32.const {index}) (i32.const {arity}) (i32.const {})))",
            captured.len()
        );
        for (k, v) in captured.iter().enumerate() {
            let _ = write!(out, " (i64.store offset={} (local.get {p}) {v})", 16 + 8 * k);
        }
        let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
        Ok(out)
    }

    /// Compile everything queued and assemble the module text.
    pub fn finish(mut self) -> Result<String, CodegenError> {
        self.compile_queued()?;
        let mut m = String::from("(module\n");
        for n in 0..=self.max_arity {
            let params: String = " i64".repeat(n);
            let _ = writeln!(m, "  (type $fn{n} (func (param i64{params}) (result i64)))");
        }
        for i in &self.imports {
            let _ = writeln!(m, "  {i}");
        }
        let heap_start = (self.next_addr + 7) & !7;
        let pages = heap_start / 65536 + 1;
        let _ = writeln!(m, "  (memory (export \"memory\") {pages})");
        let _ = writeln!(m, "  (global $hp (mut i32) (i32.const {heap_start}))");
        let _ = writeln!(m, "  (global $rtdata i32 (i32.const {}))", self.rtdata);
        let _ = writeln!(m, "  (table $fns {} funcref)", self.table.len().max(1));
        if !self.table.is_empty() {
            let _ = writeln!(m, "  (elem (table $fns) (i32.const 0) func {})", self.table.join(" "));
        }
        for (addr, bytes) in &self.data {
            let _ = writeln!(m, "  (data (i32.const {addr}) \"{}\")", escape(bytes));
        }
        m.push_str(RUNTIME);
        for n in 0..=self.max_arity {
            let params: String = (0..n).map(|i| format!(" (param $a{i} i64)")).collect();
            let args: String = (0..n).map(|i| format!(" (local.get $a{i})")).collect();
            let _ = writeln!(
                m,
                "  (func $apply{n} (param $f i64){params} (result i64)\n    (call_indirect $fns (type $fn{n}) (local.get $f){args} (i32.load offset=8 (i32.wrap_i64 (local.get $f)))))"
            );
        }
        for f in &self.funcs {
            m.push_str(f);
            m.push('\n');
        }
        for e in &self.exports {
            let _ = writeln!(m, "  {e}");
        }
        m.push_str(")\n");
        Ok(m)
    }
}

impl Frame {
    /// Pointers to objects under construction are kept as i32 locals;
    /// their names carry a `w` so `locals` declares them as such.
    fn local32(&mut self) -> String {
        let name = format!("$w{}", self.locals.len());
        self.locals.push(name.clone());
        name
    }
}

fn locals(frame: &Frame) -> String {
    frame
        .locals
        .iter()
        .map(|l| if l.starts_with("$w") { format!(" (local {l} i32)") } else { format!(" (local {l} i64)") })
        .collect()
}

fn prim(op: Op, a: &[String], b: &mut Builder) -> String {
    let bool = |e: String| format!("(call $bool {e})");
    match op {
        Op::Add => format!("(call $int_add {} {})", a[0], a[1]),
        Op::Sub => format!("(call $int_sub {} {})", a[0], a[1]),
        Op::Mul => format!("(call $int_mul {} {})", a[0], a[1]),
        Op::Quot => format!("(call $int_quot {} {})", a[0], a[1]),
        Op::Rem => format!("(call $int_rem {} {})", a[0], a[1]),
        Op::Lt => bool(format!("(i64.lt_s {} {})", a[0], a[1])),
        Op::Gt => bool(format!("(i64.gt_s {} {})", a[0], a[1])),
        Op::Le => bool(format!("(i64.le_s {} {})", a[0], a[1])),
        Op::Ge => bool(format!("(i64.ge_s {} {})", a[0], a[1])),
        Op::Eq => bool(format!("(call $eq {} {})", a[0], a[1])),
        Op::NotEq => bool(format!("(i32.eqz (call $eq {} {}))", a[0], a[1])),
        Op::Not => bool(format!("(i64.ne {} (i64.const {TRUE}))", a[0])),
        Op::IsNil => bool(format!("(i64.eq {} (i64.const {NIL}))", a[0])),
        Op::Str => {
            if a.is_empty() {
                return format!("(i64.const {})", b.static_str(""));
            }
            let mut out = format!("(call $to_str {})", a[0]);
            for x in &a[1..] {
                out = format!("(call $str_concat {out} (call $to_str {x}))");
            }
            out
        }
        Op::Count => format!("(call $count {})", a[0]),
        Op::First => format!("(call $first {})", a[0]),
        Op::Rest => format!("(call $rest {})", a[0]),
        Op::Concat => format!("(call $concat {} {})", a[0], a[1]),
        Op::Conj => format!("(call $conj {} {})", a[0], a[1]),
        Op::Map => format!("(call $map {} {})", a[0], a[1]),
        Op::Filter => format!("(call $filter {} {})", a[0], a[1]),
        Op::Get => format!("(call $dict_get {} {})", a[0], a[1]),
        Op::Assoc => format!("(call $dict_assoc {} {} {})", a[0], a[1], a[2]),
        Op::Dissoc => format!("(call $dict_dissoc {} {})", a[0], a[1]),
        Op::H => format!("(call $view_new {} {} {} {})", a[0], a[1], a[2], a[3]),
        Op::Emit => format!("(call $emit_new {} {})", a[0], a[1]),
    }
}

/// The de Bruijn indices, from outside, of the locals a function body
/// mentions beyond its own `binders` parameters.
fn free_locals(term: &Term, binders: u32, out: &mut BTreeSet<u32>) {
    match term {
        Term::Local(i) => {
            if *i >= binders {
                out.insert(*i - binders);
            }
        }
        Term::Fn(arity, body) => free_locals(body, binders + arity, out),
        Term::Let(v, body) => {
            free_locals(v, binders, out);
            free_locals(body, binders + 1, out);
        }
        Term::IfSome(v, t, e) => {
            free_locals(v, binders, out);
            free_locals(t, binders + 1, out);
            free_locals(e, binders, out);
        }
        Term::App(c, args) => {
            free_locals(c, binders, out);
            args.iter().for_each(|a| free_locals(a, binders, out));
        }
        Term::If(a, b, c) => {
            free_locals(a, binders, out);
            free_locals(b, binders, out);
            free_locals(c, binders, out);
        }
        Term::Coalesce(a, b) | Term::And(a, b) | Term::Or(a, b) => {
            free_locals(a, binders, out);
            free_locals(b, binders, out);
        }
        Term::Field(_, t) => free_locals(t, binders, out),
        Term::Record(fs) | Term::Dict(fs) => fs.iter().for_each(|(_, t)| free_locals(t, binders, out)),
        Term::List(items) | Term::Prim(_, items) => items.iter().for_each(|t| free_locals(t, binders, out)),
        Term::Int(_) | Term::Str(_) | Term::Bool(_) | Term::Nil | Term::Ref(_) | Term::CycleRef(_) => {}
    }
}

fn escape(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes {
        let _ = write!(s, "\\{b:02x}");
    }
    s
}

/// Compile definitions to a core module that exports each under the
/// given name: a function definition as a Wasm function taking and
/// returning values, any other definition as a function of no
/// arguments returning its value. Returns the module's text.
pub fn compile_exports(cb: &Codebase, exports: &[(&str, Hash)]) -> Result<String, CodegenError> {
    let mut b = Builder::new(cb);
    for (name, hash) in exports {
        b.require(*hash)?;
        let def_name = Builder::def_name(hash);
        match &cb.defs[hash].term {
            Term::Fn(arity, _) => {
                let params: String = (0..*arity).map(|i| format!(" (param $a{i} i64)")).collect();
                let args: String = (0..*arity).map(|i| format!(" (local.get $a{i})")).collect();
                b.add_func(format!(
                    "  (func (export \"{name}\"){params} (result i64) (call {def_name} (i64.const {NIL}){args}))"
                ));
            }
            _ => b.add_export(format!("(export \"{name}\" (func {def_name}))")),
        }
    }
    b.finish()
}

/// A value read back out of a module's memory.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Nil,
    Str(String),
    List(Vec<Value>),
    /// Fields by id; `name_fields` turns ids into names.
    Record(Vec<(i64, Value)>),
    Dict(Vec<(String, Value)>),
    Closure,
    View {
        tag: String,
        attrs: Vec<(String, Value)>,
        children: Vec<Value>,
        events: Vec<(String, Value)>,
    },
    Emit(String, String),
}

/// Decode `v` by reading the module's memory.
pub fn decode(memory: &[u8], v: i64) -> Value {
    let u32_at = |a: usize| u32::from_le_bytes(memory[a..a + 4].try_into().unwrap());
    let i64_at = |a: usize| i64::from_le_bytes(memory[a..a + 8].try_into().unwrap());
    if v & 1 == 1 {
        return Value::Int(v >> 1);
    }
    match v {
        NIL => return Value::Nil,
        TRUE => return Value::Bool(true),
        FALSE => return Value::Bool(false),
        _ => {}
    }
    let p = v as usize;
    let (kind, len) = (u32_at(p), u32_at(p + 4) as usize);
    let str_of = |v: i64| match decode(memory, v) {
        Value::Str(s) => s,
        other => panic!("expected a string, found {other:?}"),
    };
    let pairs = |p: usize, n: usize| -> Vec<(String, Value)> {
        (0..n).map(|i| (str_of(i64_at(p + 8 + 16 * i)), decode(memory, i64_at(p + 16 + 16 * i)))).collect()
    };
    let list = |v: i64| match decode(memory, v) {
        Value::List(items) => items,
        other => panic!("expected a list, found {other:?}"),
    };
    let dict = |v: i64| match decode(memory, v) {
        Value::Dict(items) => items,
        other => panic!("expected a dict, found {other:?}"),
    };
    match kind {
        KIND_STR => Value::Str(String::from_utf8(memory[p + 8..p + 8 + len].to_vec()).unwrap()),
        KIND_LIST => Value::List((0..len).map(|i| decode(memory, i64_at(p + 8 + 8 * i))).collect()),
        KIND_RECORD => {
            Value::Record((0..len).map(|i| (i64_at(p + 8 + 16 * i), decode(memory, i64_at(p + 16 + 16 * i)))).collect())
        }
        KIND_DICT => Value::Dict(pairs(p, len)),
        KIND_CLOSURE => Value::Closure,
        KIND_VIEW => Value::View {
            tag: str_of(i64_at(p + 8)),
            attrs: dict(i64_at(p + 16)),
            children: list(i64_at(p + 24)),
            events: dict(i64_at(p + 32)),
        },
        KIND_EMIT => Value::Emit(str_of(i64_at(p + 8)), str_of(i64_at(p + 16))),
        k => panic!("unknown object kind {k} at {p}"),
    }
}

/// Replace record field ids with names from `names`.
pub fn name_fields(v: Value, names: &[&str]) -> Vec<(String, Value)> {
    match v {
        Value::Record(fields) => fields
            .into_iter()
            .map(|(id, v)| {
                let name = names.iter().find(|n| field_id(n) == id).map(|n| n.to_string()).unwrap_or(format!("#{id}"));
                (name, v)
            })
            .collect(),
        other => panic!("expected a record, found {other:?}"),
    }
}
