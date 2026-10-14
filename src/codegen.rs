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
//!
//! # Memory
//!
//! Values are reference counted, after Perceus (Reinking, Xie, de Moura
//! and Leijen, PLDI 2021). Every function owns its parameters and
//! returns a value its caller owns. While compiling a function the code
//! generator tracks which of its locals it still owns:
//!
//! - a use of a local that is needed again later duplicates it, and its
//!   last use moves it;
//! - a local nothing will use again is dropped as soon as that is known:
//!   on entry for an unused parameter, right after binding for an unused
//!   `let`, and at the start of each branch for what only the other
//!   branch uses;
//! - a closure owns copies of what it captured, and its body borrows
//!   them, duplicating at each use.
//!
//! So a value is freed when its last reference goes, at a point the
//! compiler fixed, with no collector. The runtime's built-ins borrow
//! their arguments; the code generator drops temporaries after them.
//! The exceptions are the updates (`assoc` on records and dictionaries,
//! `dissoc`, `conj`, `map`, `filter`), which consume the collection and
//! change it in place when its count is one: Perceus's reuse, decided at
//! run time by the count rather than by pairing drops with allocations
//! at compile time.

use crate::codebase::Codebase;
use crate::term::{Hash, Op, Pattern, Step, Term};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

/// The value no conduct code can write or name: the slot of an absent
/// attribute in glue, JSON's `null` inside a `Json`, the closure
/// argument of a direct call, and what an impossible `match` case lowers
/// to.
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
pub const KIND_FETCH: u32 = 8;
/// A keyword: laid out as a string of its name, without the colon. Every
/// keyword is static, interned by the module.
pub const KIND_KEYWORD: u32 = 9;

const RUNTIME: &str = include_str!("runtime.wat");
const RUNTIME_DATA: &[u8] = b"truefalse[] <value>null{}:,\"\\ustringnumberbooleanarrayobject";
/// Addresses below this hold scratch space: the canonical ABI return
/// area at 0 and the allocator's free-list heads at 64.
const STATIC_START: u32 = 1024;
/// The reference count of a static object, which is never counted.
const IMMORTAL: u32 = u32::MAX;

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

type Result<T> = std::result::Result<T, CodegenError>;
type Names = BTreeSet<String>;

/// Where a local lives inside the function being generated.
#[derive(Clone)]
enum Slot {
    /// A Wasm local the function owns until it moves or drops it.
    Local(String),
    /// The `k`th free variable of the closure being run, borrowed.
    Free(usize),
    /// A local of an enclosing function that this one did not capture.
    Absent,
}

/// The function currently being generated.
struct Frame {
    locals: Vec<String>,
    /// The locals this function still owns.
    owned: Names,
}

impl Frame {
    fn new() -> Frame {
        Frame { locals: vec![], owned: Names::new() }
    }

    fn local(&mut self) -> String {
        let name = format!("$t{}", self.locals.len());
        self.locals.push(name.clone());
        name
    }

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

/// The owned locals a term mentions.
fn fv(term: &Term, env: &[Slot]) -> Names {
    let mut indices = BTreeSet::new();
    free_locals(term, 0, &mut indices);
    indices
        .into_iter()
        .filter_map(|d| match &env[env.len() - 1 - d as usize] {
            Slot::Local(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// Drop each of `names`, then evaluate `body`.
fn with_drops(names: &Names, body: String) -> String {
    if names.is_empty() {
        return body;
    }
    let drops: String = names.iter().map(|n| format!(" (call $drop (local.get {n}))")).collect();
    format!("(block (result i64){drops} {body})")
}

/// Builds one module: the runtime, the definitions it needs, and
/// whatever imports, functions, and exports a caller adds.
pub struct Builder<'a> {
    cb: &'a Codebase,
    imports: Vec<String>,
    funcs: Vec<String>,
    /// Function names in table order.
    table: Vec<String>,
    /// Each table entry's code id: what identifies its code wherever it
    /// is compiled, which `=`, `hash` and `compare` read in place of its
    /// place in this module's table.
    code_ids: Vec<[u8; 8]>,
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
    /// Host functions the module defines itself, by name: their glue
    /// calls the component's imports.
    provided: HashMap<String, String>,
    /// Definitions marked `!` whose performing entry is compiled or
    /// queued: called directly when a task is performed where it is
    /// made, so the common case allocates no task.
    runs: HashSet<Hash>,
    run_queue: Vec<Hash>,
    /// The capability each host function needs, by its name.
    capabilities: HashMap<String, String>,
}

impl<'a> Builder<'a> {
    pub fn new(cb: &'a Codebase) -> Self {
        let mut b = Builder {
            cb,
            imports: vec![],
            funcs: vec![],
            table: vec![],
            code_ids: vec![],
            data: vec![],
            next_addr: STATIC_START,
            strings: HashMap::new(),
            defs: HashMap::new(),
            queue: vec![],
            lambdas: 0,
            max_arity: 4,
            exports: vec![],
            rtdata: 0,
            provided: HashMap::new(),
            runs: HashSet::new(),
            run_queue: vec![],
            capabilities: cb
                .hosts
                .iter()
                .flat_map(|(cap, fs)| fs.keys().map(move |f| (f.clone(), cap.clone())))
                .collect(),
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

    /// A heap object in static data, behind the immortal prefix the
    /// allocator's blocks carry, so reference counting skips it.
    fn static_object(&mut self, object: Vec<u8>) -> u32 {
        let mut bytes = IMMORTAL.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&object);
        self.static_bytes(bytes) + 8
    }

    /// A string object in static data, as a value.
    pub fn static_str(&mut self, s: &str) -> u32 {
        if let Some(addr) = self.strings.get(s) {
            return *addr;
        }
        let mut bytes = KIND_STR.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
        let addr = self.static_object(bytes);
        self.strings.insert(s.to_string(), addr);
        addr
    }

    /// Call the module's function `func` for the host function `name`,
    /// instead of importing it. The function borrows its arguments and
    /// returns an owned value, as an imported one does.
    pub fn provide_host(&mut self, name: &str, func: &str) {
        self.provided.insert(name.to_string(), func.to_string());
    }

    /// The import of the host function `name`, declared once, or the
    /// function provided for it.
    fn host_import(&mut self, name: &str, arity: usize) -> String {
        if let Some(func) = self.provided.get(name) {
            return func.clone();
        }
        let id = format!("$host_{}", Hash::of(name.as_bytes()).hex().get(..12).unwrap_or_default());
        let import = format!(
            "(import \"conduct:host\" \"{name}\" (func {id}{} (result i64)))",
            if arity == 0 { String::new() } else { format!(" (param{})", " i64".repeat(arity)) }
        );
        if !self.imports.contains(&import) {
            self.imports.push(import);
        }
        id
    }

    /// The empty record, in static data, as a value: what a function
    /// with nothing to answer answers.
    pub fn static_empty_record(&mut self) -> u32 {
        // Not a string's text, so no string literal shares the entry.
        let key = "\0{}".to_string();
        if let Some(addr) = self.strings.get(&key) {
            return *addr;
        }
        let mut bytes = KIND_RECORD.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let addr = self.static_object(bytes);
        self.strings.insert(key, addr);
        addr
    }

    /// A keyword object in static data, as a value.
    pub fn static_keyword(&mut self, k: &str) -> u32 {
        let key = format!("\0:{k}");
        if let Some(addr) = self.strings.get(&key) {
            return *addr;
        }
        let mut bytes = KIND_KEYWORD.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(k.len() as u32).to_le_bytes());
        bytes.extend_from_slice(k.as_bytes());
        let addr = self.static_object(bytes);
        self.strings.insert(key, addr);
        addr
    }

    fn def_name(hash: &Hash) -> String {
        format!("$d_{}", &hash.hex()[..16])
    }

    /// Queue a definition (and so everything it reaches) for
    /// compilation, returning its static closure if it is a function.
    fn require(&mut self, hash: Hash) -> Result<Option<u32>> {
        if let Some(addr) = self.defs.get(&hash) {
            return Ok(*addr);
        }
        let def = self
            .cb
            .defs
            .get(&hash)
            .ok_or_else(|| CodegenError(format!("the codebase has no definition {}", hash.short())))?;
        let addr = match &def.term {
            Term::Fn(labels, _) => {
                let arity = &(labels.len() as u32);
                let index = self.table.len() as u32;
                self.table.push(Self::def_name(&hash));
                // A definition's code is identified by its hash.
                self.code_ids.push(hash.0[..8].try_into().unwrap());
                self.max_arity = self.max_arity.max(*arity as usize);
                let mut bytes = KIND_CLOSURE.to_le_bytes().to_vec();
                bytes.extend_from_slice(&0u32.to_le_bytes());
                bytes.extend_from_slice(&index.to_le_bytes());
                bytes.extend_from_slice(&arity.to_le_bytes());
                Some(self.static_object(bytes))
            }
            _ => None,
        };
        self.defs.insert(hash, addr);
        self.queue.push(hash);
        Ok(addr)
    }

    /// A WAT expression for the value of a definition, owned by whoever
    /// evaluates it.
    pub fn value_of(&mut self, hash: Hash) -> Result<String> {
        Ok(match self.require(hash)? {
            Some(addr) => format!("(i64.const {addr})"),
            None => format!("(call {})", Self::def_name(&hash)),
        })
    }

    /// A WAT expression applying a definition to argument expressions.
    /// The call consumes the arguments: pass duplicates of anything the
    /// caller keeps.
    pub fn call(&mut self, hash: Hash, args: &[String]) -> Result<String> {
        let arity = match &self.cb.defs[&hash].term {
            Term::Fn(labels, _) => Some(labels.len()),
            _ => None,
        };
        if arity == Some(args.len()) {
            self.require(hash)?;
            return Ok(format!("(call {} (i64.const {NIL}) {})", Self::def_name(&hash), args.join(" ")));
        }
        let f = self.value_of(hash)?;
        self.max_arity = self.max_arity.max(args.len());
        Ok(format!("(call $apply{}_take {f} {})", args.len(), args.join(" ")))
    }

    /// The body of a task a definition makes, if it is marked `!`: the
    /// definition's term is a task, or a function answering one.
    fn task_body(&self, hash: &Hash) -> Option<(u32, Term)> {
        match &self.cb.defs.get(hash)?.term {
            Term::Task(body) => Some((0, (**body).clone())),
            Term::Fn(labels, body) => match &**body {
                Term::Task(inner) => Some((labels.len() as u32, (**inner).clone())),
                _ => None,
            },
            _ => None,
        }
    }

    fn run_name(hash: &Hash) -> String {
        format!("$r_{}", &hash.hex()[..16])
    }

    /// A WAT expression performing what a definition marked `!` makes,
    /// given its argument expressions (which it consumes): the task's
    /// body, run directly, with no task made.
    pub fn perform(&mut self, hash: Hash, args: &[String]) -> Result<String> {
        let Some((arity, _)) = self.task_body(&hash) else {
            return Err(CodegenError(format!("{} is not marked `!`", hash.short())));
        };
        if arity as usize != args.len() {
            return Err(CodegenError(format!("{} takes {arity} argument(s)", hash.short())));
        }
        if self.runs.insert(hash) {
            self.run_queue.push(hash);
        }
        Ok(format!("(call {} {})", Self::run_name(&hash), args.join(" ")))
    }

    fn compile_queued(&mut self) -> Result<()> {
        loop {
            if let Some(hash) = self.run_queue.pop() {
                let (arity, body) = self.task_body(&hash).expect("queued as a task");
                let cycle = self.cb.defs[&hash].cycle.clone();
                let env: Vec<Slot> = (0..arity).map(|i| Slot::Local(format!("$p{i}"))).collect();
                let (body, frame) = self.function_body(&body, env, &cycle)?;
                let params: String = (0..arity).map(|i| format!(" (param $p{i} i64)")).collect();
                self.funcs.push(format!(
                    "  (func {}{params} (result i64){}\n    {body})",
                    Self::run_name(&hash),
                    locals(&frame)
                ));
                continue;
            }
            let Some(hash) = self.queue.pop() else { break };
            let def = &self.cb.defs[&hash];
            let term = def.term.clone();
            let cycle = def.cycle.clone();
            let name = Self::def_name(&hash);
            let text = match &term {
                Term::Fn(labels, body) => {
                    let arity = &(labels.len() as u32);
                    let env: Vec<Slot> = (0..*arity).map(|i| Slot::Local(format!("$p{i}"))).collect();
                    let (body, frame) = self.function_body(body, env, &cycle)?;
                    let params: String = (0..*arity).map(|i| format!(" (param $p{i} i64)")).collect();
                    format!("  (func {name} (param $env i64){params} (result i64){}\n    {body})", locals(&frame))
                }
                other => {
                    let (body, frame) = self.function_body(other, Vec::new(), &cycle)?;
                    format!("  (func {name} (result i64){}\n    {body})", locals(&frame))
                }
            };
            self.funcs.push(text);
        }
        Ok(())
    }

    /// Compile a function's body: it owns its parameters, drops the ones
    /// it never uses, and must have consumed all of them by its end.
    fn function_body(&mut self, body: &Term, mut env: Vec<Slot>, cycle: &[Hash]) -> Result<(String, Frame)> {
        let mut frame = Frame::new();
        frame.owned = env
            .iter()
            .filter_map(|s| match s {
                Slot::Local(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        let used = fv(body, &env);
        let unused: Names = frame.owned.difference(&used).cloned().collect();
        for n in &unused {
            frame.owned.remove(n);
        }
        let code = self.expr(body, &mut env, &mut frame, cycle, &Names::new())?;
        if !frame.owned.is_empty() {
            return Err(CodegenError(format!(
                "internal: locals still owned at the end of a function: {:?}",
                frame.owned
            )));
        }
        Ok((with_drops(&unused, code), frame))
    }

    /// The live set for each of a sequence of subterms evaluated in
    /// order: what is live after the sequence, plus what later subterms
    /// use.
    fn sequence_live(items: &[&Term], env: &[Slot], live: &Names) -> Vec<Names> {
        let mut out = vec![Names::new(); items.len()];
        let mut acc = live.clone();
        for i in (0..items.len()).rev() {
            out[i] = acc.clone();
            acc.extend(fv(items[i], env));
        }
        out
    }

    fn sequence(
        &mut self,
        items: &[&Term],
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
        live: &Names,
    ) -> Result<Vec<String>> {
        let lives = Self::sequence_live(items, env, live);
        items.iter().zip(lives.iter()).map(|(t, l)| self.expr(t, env, frame, cycle, l)).collect()
    }

    /// Compile one arm of a branch, starting from the locals owned before
    /// the branch, dropping first what this arm does not use.
    fn arm(
        &mut self,
        term: &Term,
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
        live: &Names,
        start: &Names,
    ) -> Result<String> {
        frame.owned = start.clone();
        let mut needed = fv(term, env);
        needed.extend(live.iter().cloned());
        let drops: Names = frame.owned.difference(&needed).cloned().collect();
        for n in &drops {
            frame.owned.remove(n);
        }
        let code = self.expr(term, env, frame, cycle, live)?;
        Ok(with_drops(&drops, code))
    }

    /// An arm that evaluates nothing of the program's: it drops whatever
    /// only the other arm would have used.
    fn empty_arm(frame: &mut Frame, live: &Names, start: &Names, value: String) -> String {
        frame.owned = start.clone();
        let drops: Names = frame.owned.difference(live).cloned().collect();
        for n in &drops {
            frame.owned.remove(n);
        }
        with_drops(&drops, value)
    }

    /// After a branch, every arm has consumed what it last used, so what
    /// is still owned is what was owned before and is live after.
    fn join_arms(frame: &mut Frame, live: &Names, start: &Names, ends: &[Names]) -> Result<()> {
        let expected: Names = start.intersection(live).cloned().collect();
        for end in ends {
            if *end != expected {
                return Err(CodegenError(format!(
                    "internal: branch arms disagree on ownership: {end:?} vs {expected:?}"
                )));
            }
        }
        frame.owned = expected;
        Ok(())
    }

    fn expr(
        &mut self,
        term: &Term,
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
        live: &Names,
    ) -> Result<String> {
        Ok(match term {
            Term::Int(n) => format!("(i64.const {})", int(*n)),
            Term::Bool(b) => format!("(i64.const {})", if *b { TRUE } else { FALSE }),
            Term::Nil => format!("(i64.const {NIL})"),
            Term::Str(s) => format!("(i64.const {})", self.static_str(s)),
            Term::Keyword(k) => format!("(i64.const {})", self.static_keyword(k)),
            Term::Host(name, args) => {
                // The host borrows its arguments, like a built-in.
                let refs: Vec<&Term> = args.iter().collect();
                let values = self.sequence(&refs, env, frame, cycle, live)?;
                let import = self.host_import(name, args.len());
                let temps: Vec<String> = values.iter().map(|_| frame.local()).collect();
                let result = frame.local();
                let implementation = frame.local();
                let mut out = String::from("(block (result i64)");
                for (t, v) in temps.iter().zip(values) {
                    let _ = write!(out, " (local.set {t} {v})");
                }
                let gets: Vec<String> = temps.iter().map(|t| format!("(local.get {t})")).collect();
                // An environment that provides the capability answers in
                // the host's place.
                let cap = self.capabilities.get(name).cloned().unwrap_or_default();
                let dups: Vec<String> = temps.iter().map(|t| format!("(call $dup (local.get {t}))")).collect();
                self.max_arity = self.max_arity.max(args.len());
                let _ = write!(
                    out,
                    " (local.set {implementation} (call $env_find (i64.const {}) (i64.const {})))\
                     (local.set {result} (if (result i64) (i64.eqz (local.get {implementation}))\
                     (then (call {import} {})) (else {})))",
                    field_id(&cap),
                    field_id(name),
                    gets.join(" "),
                    if args.is_empty() {
                        format!("(local.get {implementation})")
                    } else {
                        format!("(call $apply{}_take (local.get {implementation}) {})", args.len(), dups.join(" "))
                    }
                );
                for t in &temps {
                    let _ = write!(out, " (call $drop (local.get {t}))");
                }
                let _ = write!(out, " (local.get {result}))");
                out
            }
            Term::Match(scrutinee, clauses, default) => {
                // A `let` of the value and a chain of `if`s on its
                // equality with each pattern. A match without `_`
                // is exhaustive, so its last `else` is never taken.
                let mut chain = match default {
                    Some(d) => shift(d, 1, 0),
                    None => Term::Nil,
                };
                for (pattern, result) in clauses.iter().rev() {
                    let test = pattern_test(Term::Local(0), pattern);
                    // Each binder is a `let` of what it read, in pattern
                    // order, under the value's own `let`.
                    let paths = pattern.binder_paths();
                    let n = paths.len() as u32;
                    let mut body = shift(result, 1, n);
                    for (j, path) in paths.iter().enumerate().rev() {
                        let read = path.iter().fold(Term::Local(j as u32), |v, step| match step {
                            Step::Field(k) => Term::Field(k.clone(), Box::new(v)),
                            Step::Item(i) => Term::Prim(Op::Nth, vec![v, Term::Int(*i as i64)]),
                            Step::Drop(n) => Term::Prim(Op::Drop, vec![v, Term::Int(*n as i64)]),
                            Step::Without(keys) => {
                                keys.iter().fold(v, |v, k| Term::Prim(Op::Without, vec![v, Term::Str(k.clone())]))
                            }
                        });
                        body = Term::Let(Box::new(read), Box::new(body));
                    }
                    chain = Term::If(Box::new(test), Box::new(body), Box::new(chain));
                }
                let lowered = Term::Let(scrutinee.clone(), Box::new(chain));
                self.expr(&lowered, env, frame, cycle, live)?
            }
            Term::Local(i) => match &env[env.len() - 1 - *i as usize] {
                Slot::Local(name) => {
                    if !frame.owned.contains(name) {
                        return Err(CodegenError(format!("internal: {name} used after it was moved")));
                    }
                    if live.contains(name) {
                        format!("(call $dup (local.get {name}))")
                    } else {
                        frame.owned.remove(name);
                        format!("(local.get {name})")
                    }
                }
                Slot::Free(k) => {
                    format!("(call $dup (i64.load offset={} (i32.wrap_i64 (local.get $env))))", 16 + 8 * k)
                }
                Slot::Absent => return Err(CodegenError("internal: a local was not captured".into())),
            },
            Term::Ref(h) => self.value_of(*h)?,
            Term::CycleRef(i) => self.value_of(cycle[*i as usize])?,
            Term::Fn(labels, body) => self.lambda(labels, body, env, frame, cycle, live)?,
            // A task is a closure of no parameters; performing it calls it.
            Term::Task(body) => self.lambda(&[], body, env, frame, cycle, live)?,
            Term::Perform(task) => {
                let direct = |t: &Term| match t {
                    Term::Ref(h) => Some(*h),
                    Term::CycleRef(i) => Some(cycle[*i as usize]),
                    _ => None,
                };
                match &**task {
                    // Performing what a definition marked `!` makes, where
                    // it is made: run its body, with no task in between.
                    Term::App(callee, args, _)
                        if direct(callee).is_some_and(|h| {
                            self.task_body(&h).is_some_and(|(arity, _)| arity as usize == args.len())
                        }) =>
                    {
                        let h = direct(callee).unwrap();
                        let refs: Vec<&Term> = args.iter().collect();
                        let args = self.sequence(&refs, env, frame, cycle, live)?;
                        self.perform(h, &args)?
                    }
                    t if direct(t).is_some_and(|h| self.task_body(&h).is_some_and(|(arity, _)| arity == 0)) => {
                        self.perform(direct(t).unwrap(), &[])?
                    }
                    other => {
                        let t = self.expr(other, env, frame, cycle, live)?;
                        format!("(call $apply0_take {t})")
                    }
                }
            }
            // A task that performs another with an environment on top.
            Term::Handle(task, provided) => {
                // An operation of no parameters is implemented by its
                // answer, which a host call answers in its place.
                let record =
                    Term::Record(provided.iter().map(|(cap, ops)| (cap.clone(), Term::Record(ops.clone()))).collect());
                let body = Term::Prim(Op::WithEnv, vec![record, (**task).clone()]);
                self.lambda(&[], &body, env, frame, cycle, live)?
            }
            Term::App(callee, args, _) => {
                let direct = match &**callee {
                    Term::Ref(h) => Some(*h),
                    Term::CycleRef(i) => Some(cycle[*i as usize]),
                    _ => None,
                };
                let arg_refs: Vec<&Term> = args.iter().collect();
                match direct {
                    Some(h) => {
                        let args = self.sequence(&arg_refs, env, frame, cycle, live)?;
                        self.call(h, &args)?
                    }
                    None => {
                        let mut items = vec![&**callee];
                        items.extend(arg_refs);
                        let mut parts = self.sequence(&items, env, frame, cycle, live)?;
                        let f = parts.remove(0);
                        self.max_arity = self.max_arity.max(parts.len());
                        format!("(call $apply{}_take {f} {})", parts.len(), parts.join(" "))
                    }
                }
            }
            Term::Let(value, body) => {
                let local = frame.local();
                env.push(Slot::Local(local.clone()));
                let body_fv = fv(body, env);
                env.pop();
                let mut value_live = live.clone();
                value_live.extend(body_fv.iter().filter(|n| **n != local).cloned());
                let v = self.expr(value, env, frame, cycle, &value_live)?;
                env.push(Slot::Local(local.clone()));
                frame.owned.insert(local.clone());
                let unused = !body_fv.contains(&local);
                if unused {
                    frame.owned.remove(&local);
                }
                let body = self.expr(body, env, frame, cycle, live);
                env.pop();
                let drop = if unused { format!(" (call $drop (local.get {local}))") } else { String::new() };
                format!("(block (result i64) (local.set {local} {v}){drop} {})", body?)
            }
            Term::If(c, t, e) => {
                let mut cond_live = live.clone();
                cond_live.extend(fv(t, env));
                cond_live.extend(fv(e, env));
                let c = self.expr(c, env, frame, cycle, &cond_live)?;
                let start = frame.owned.clone();
                let t = self.arm(t, env, frame, cycle, live, &start)?;
                let end_t = frame.owned.clone();
                let e = self.arm(e, env, frame, cycle, live, &start)?;
                let end_e = frame.owned.clone();
                Self::join_arms(frame, live, &start, &[end_t, end_e])?;
                format!("(if (result i64) (i64.eq {c} (i64.const {TRUE})) (then {t}) (else {e}))")
            }
            Term::And(a, b) | Term::Or(a, b) => {
                let is_and = matches!(term, Term::And(..));
                let mut a_live = live.clone();
                a_live.extend(fv(b, env));
                let a = self.expr(a, env, frame, cycle, &a_live)?;
                let start = frame.owned.clone();
                let short =
                    Self::empty_arm(frame, live, &start, format!("(i64.const {})", if is_and { FALSE } else { TRUE }));
                let end_s = frame.owned.clone();
                let long = self.arm(b, env, frame, cycle, live, &start)?;
                let end_l = frame.owned.clone();
                Self::join_arms(frame, live, &start, &[end_s, end_l])?;
                if is_and {
                    format!("(if (result i64) (i64.eq {a} (i64.const {TRUE})) (then {long}) (else {short}))")
                } else {
                    format!("(if (result i64) (i64.eq {a} (i64.const {TRUE})) (then {short}) (else {long}))")
                }
            }
            Term::Record(fields) => {
                let items: Vec<&Term> = fields.iter().map(|(_, t)| t).collect();
                let values = self.sequence(&items, env, frame, cycle, live)?;
                // The values are evaluated in the order ownership was
                // worked out in, the term's, and only then stored in the
                // record's order, by field id.
                let mut out = String::from("(block (result i64)");
                let mut sorted: Vec<(i64, String)> = Vec::new();
                for ((name, _), v) in fields.iter().zip(values) {
                    let t = frame.local();
                    let _ = write!(out, " (local.set {t} {v})");
                    sorted.push((field_id(name), t));
                }
                sorted.sort_by_key(|f| f.0);
                let p = frame.local32();
                let _ = write!(
                    out,
                    " (local.set {p} (call $obj (i32.const {KIND_RECORD}) (i32.const {}) (i32.const {})))",
                    sorted.len(),
                    16 * sorted.len()
                );
                for (i, (id, t)) in sorted.iter().enumerate() {
                    let _ = write!(
                        out,
                        " (i64.store offset={} (local.get {p}) (i64.const {id})) (i64.store offset={} (local.get {p}) (local.get {t}))",
                        8 + 16 * i,
                        16 + 16 * i
                    );
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::Field(name, record) => {
                let r = self.expr(record, env, frame, cycle, live)?;
                format!("(call $rec_take {r} (i64.const {}))", field_id(name))
            }
            Term::Update(record, name, value) => {
                let parts = self.sequence(&[&**record, &**value], env, frame, cycle, live)?;
                // `$rec_set` consumes both, and updates in place when the
                // record is unique.
                format!("(call $rec_set {} (i64.const {}) {})", parts[0], field_id(name), parts[1])
            }
            Term::Dict(entries) => {
                let items: Vec<&Term> = entries.iter().map(|(_, t)| t).collect();
                let values = self.sequence(&items, env, frame, cycle, live)?;
                let p = frame.local32();
                let mut out =
                    format!("(block (result i64) (local.set {p} (call $dict_new (i32.const {})))", entries.len());
                for (i, ((k, _), v)) in entries.iter().zip(values).enumerate() {
                    let key = self.static_str(k);
                    let _ = write!(out, " (call $dict_put (local.get {p}) (i32.const {i}) (i64.const {key}) {v})");
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::List(items) => {
                let refs: Vec<&Term> = items.iter().collect();
                let values = self.sequence(&refs, env, frame, cycle, live)?;
                let p = frame.local32();
                let mut out =
                    format!("(block (result i64) (local.set {p} (call $list_new (i32.const {})))", values.len());
                for (i, v) in values.iter().enumerate() {
                    let _ = write!(out, " (call $list_set (local.get {p}) (i32.const {i}) {v})");
                }
                let _ = write!(out, " (i64.extend_i32_u (local.get {p})))");
                out
            }
            Term::Prim(Op::Without, args) => {
                let [value, Term::Str(name)] = args.as_slice() else {
                    return Err(CodegenError("internal: a record's rest names the field it leaves out".into()));
                };
                let v = self.expr(value, env, frame, cycle, live)?;
                let (t, r) = (frame.local(), frame.local());
                format!(
                    "(block (result i64) (local.set {t} {v}) (local.set {r} (call $rec_without (local.get {t}) (i64.const {}))) (call $drop (local.get {t})) (local.get {r}))",
                    field_id(name)
                )
            }
            Term::Prim(Op::HasField, args) => {
                let [value, Term::Str(name)] = args.as_slice() else {
                    return Err(CodegenError("internal: a field test names its field".into()));
                };
                let v = self.expr(value, env, frame, cycle, live)?;
                let t = frame.local();
                format!(
                    "(block (result i64) (local.set {t} {v}) (call $has_field (local.get {t}) (i64.const {})) (call $drop (local.get {t})))",
                    field_id(name)
                )
            }
            Term::Prim(op, args) => {
                let refs: Vec<&Term> = args.iter().collect();
                let values = self.sequence(&refs, env, frame, cycle, live)?;
                // Built-ins borrow: hold each argument in a temporary,
                // call, then drop the temporaries. The few that update a
                // collection consume it instead, so they can reuse it.
                let consumed = consumes(*op);
                let temps: Vec<String> = values.iter().map(|_| frame.local()).collect();
                let result = frame.local();
                let gets: Vec<String> = temps.iter().map(|t| format!("(local.get {t})")).collect();
                let call = prim(*op, &gets, self);
                let mut out = String::from("(block (result i64)");
                for (t, v) in temps.iter().zip(values) {
                    let _ = write!(out, " (local.set {t} {v})");
                }
                let _ = write!(out, " (local.set {result} {call})");
                for (i, t) in temps.iter().enumerate() {
                    if consumed != Some(i) {
                        let _ = write!(out, " (call $drop (local.get {t}))");
                    }
                }
                let _ = write!(out, " (local.get {result}))");
                out
            }
        })
    }

    fn lambda(
        &mut self,
        labels: &[String],
        body: &Term,
        env: &mut Vec<Slot>,
        frame: &mut Frame,
        cycle: &[Hash],
        live: &Names,
    ) -> Result<String> {
        // The locals of the enclosing function this lambda mentions, as
        // de Bruijn indices from outside it.
        let arity = labels.len() as u32;
        let mut free = BTreeSet::new();
        free_locals(body, arity, &mut free);
        let free: Vec<u32> = free.into_iter().collect();
        let code_id = lambda_code_id(labels, body, &free, cycle);
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
        self.code_ids.push(code_id);
        self.max_arity = self.max_arity.max(arity as usize);
        let (code, inner_frame) = self.function_body(body, inner, cycle)?;
        let params: String = (0..arity).map(|i| format!(" (param $p{i} i64)")).collect();
        self.funcs
            .push(format!("  (func {name} (param $env i64){params} (result i64){}\n    {code})", locals(&inner_frame)));
        // The closure owns a copy of each captured value: capturing moves
        // a local on its last use and duplicates it otherwise.
        let captured_terms: Vec<Term> = free.iter().map(|d| Term::Local(*d)).collect();
        let refs: Vec<&Term> = captured_terms.iter().collect();
        let captured = self.sequence(&refs, env, frame, cycle, live)?;
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
    pub fn finish(mut self) -> Result<String> {
        self.compile_queued()?;
        let mut m = String::from("(module\n");
        for n in 0..=self.max_arity {
            let params: String = " i64".repeat(n);
            let _ = writeln!(m, "  (type $fn{n} (func (param i64{params}) (result i64)))");
        }
        for i in &self.imports {
            let _ = writeln!(m, "  {i}");
        }
        let ids: Vec<u8> = self.code_ids.iter().flatten().copied().collect();
        let code_ids = self.static_bytes(ids);
        let heap_start = (self.next_addr + 7) & !7;
        let pages = heap_start / 65536 + 1;
        let _ = writeln!(m, "  (memory (export \"memory\") {pages})");
        let _ = writeln!(m, "  (global $hp (mut i32) (i32.const {heap_start}))");
        let _ = writeln!(m, "  (global $rtdata i32 (i32.const {}))", self.rtdata);
        let _ = writeln!(m, "  (global $code_ids i32 (i32.const {code_ids}))");
        let _ = writeln!(m, "  (table $fns {} funcref)", self.table.len().max(1));
        if !self.table.is_empty() {
            let _ = writeln!(m, "  (elem (table $fns) (i32.const 0) func {})", self.table.join(" "));
        }
        for (addr, bytes) in &self.data {
            let _ = writeln!(m, "  (data (i32.const {addr}) \"{}\")", escape(bytes));
        }
        m.push_str(RUNTIME);
        let _ = writeln!(
            m,
            "  (func $rec_take (param $r i64) (param $id i64) (result i64)\n    (local $v i64)\n    (local.set $v (call $rec_get (local.get $r) (local.get $id)))\n    (call $drop (local.get $r))\n    (local.get $v))"
        );
        for n in 0..=self.max_arity {
            let params: String = (0..n).map(|i| format!(" (param $a{i} i64)")).collect();
            let args: String = (0..n).map(|i| format!(" (local.get $a{i})")).collect();
            // `$applyN` borrows the closure and consumes the arguments;
            // `$applyN_take` consumes the closure too.
            let _ = writeln!(
                m,
                "  (func $apply{n} (param $f i64){params} (result i64)\n    (call_indirect $fns (type $fn{n}) (local.get $f){args} (i32.load offset=8 (i32.wrap_i64 (local.get $f)))))"
            );
            let _ = writeln!(
                m,
                "  (func $apply{n}_take (param $f i64){params} (result i64)\n    (local $r i64)\n    (local.set $r (call $apply{n} (local.get $f){args}))\n    (call $drop (local.get $f))\n    (local.get $r))"
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

/// The argument a built-in consumes, if any: the collection an update
/// may reuse in place. Every other argument is borrowed.
fn consumes(op: Op) -> Option<usize> {
    match op {
        Op::Conj | Op::Assoc | Op::Dissoc | Op::SetAt | Op::InsertAt | Op::RemoveAt => Some(0),
        Op::Map | Op::Filter => Some(1),
        _ => None,
    }
}

/// A built-in applied to argument expressions. Every built-in borrows
/// its arguments and returns an owned value.
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
        Op::Str => {
            if a.is_empty() {
                return format!("(i64.const {})", b.static_str(""));
            }
            let mut out = format!("(call $to_str {})", a[0]);
            for x in &a[1..] {
                out = format!("(call $concat_owned {out} (call $to_str {x}))");
            }
            out
        }
        Op::Count => format!("(call $count {})", a[0]),
        Op::Nth => format!("(call $list_nth {} {})", a[0], a[1]),
        Op::Drop => format!("(call $list_drop {} {})", a[0], a[1]),
        Op::First => format!("(call $first {} {})", a[0], a[1]),
        Op::Rest => format!("(call $rest {})", a[0]),
        Op::Concat => format!("(call $concat {} {})", a[0], a[1]),
        Op::Conj => format!("(call $conj_into {} {})", a[0], a[1]),
        Op::Map => format!("(call $map_into {} {})", a[0], a[1]),
        Op::Filter => format!("(call $filter_into {} {})", a[0], a[1]),
        Op::Get => format!("(call $dict_get_or {} {} {})", a[0], a[1], a[2]),
        Op::Assoc => format!("(call $dict_set {} {} {})", a[0], a[1], a[2]),
        Op::Dissoc => format!("(call $dict_remove {} {})", a[0], a[1]),
        Op::H => format!("(call $view_new {} {} {} {})", a[0], a[1], a[2], a[3]),
        Op::Emit => format!("(call $emit_new {} {})", a[0], a[1]),
        Op::Trim => format!("(call $str_trim {})", a[0]),
        Op::Fetch => format!("(call $fetch_new {} {} {} {})", a[0], a[1], a[2], a[3]),
        Op::Split => format!("(call $str_split {} {})", a[0], a[1]),
        Op::ParseJson => format!("(call $json_parse {})", a[0]),
        Op::ToJson => format!("(call $json_encode {})", a[0]),
        Op::JsonGet => format!("(call $json_get {} {})", a[0], a[1]),
        Op::JsonItems => format!("(call $json_items {})", a[0]),
        Op::JsonStr => format!("(call $json_str {})", a[0]),
        Op::JsonInt => format!("(call $json_int {})", a[0]),
        Op::JsonBool => format!("(call $json_bool {})", a[0]),
        Op::JsonType => format!("(call $json_type {})", a[0]),
        Op::BitAnd => format!("(call $bit_and {} {})", a[0], a[1]),
        Op::BitOr => format!("(call $bit_or {} {})", a[0], a[1]),
        Op::BitXor => format!("(call $bit_xor {} {})", a[0], a[1]),
        Op::ShiftLeft => format!("(call $shift_left {} {})", a[0], a[1]),
        Op::ShiftRight => format!("(call $shift_right {} {})", a[0], a[1]),
        Op::CountOnes => format!("(call $count_ones {})", a[0]),
        Op::Hash => format!("(call $hash {})", a[0]),
        Op::SetAt => format!("(call $set_at {} {} {})", a[0], a[1], a[2]),
        Op::InsertAt => format!("(call $insert_at {} {} {})", a[0], a[1], a[2]),
        Op::RemoveAt => format!("(call $remove_at {} {})", a[0], a[1]),
        Op::Item => format!("(call $item {} {} {})", a[0], a[1], a[2]),
        Op::Natural => format!("(call $natural_or {} {})", a[0], a[1]),
        Op::Compare => format!("(call $compare {} {})", a[0], a[1]),
        Op::ByteCount => format!("(call $byte_count {})", a[0]),
        Op::ByteAt => format!("(call $byte_at {} {} {})", a[0], a[1], a[2]),
        Op::CodePointAt => format!("(call $code_point_at {} {} {})", a[0], a[1], a[2]),
        Op::Slice => format!("(call $text_slice {} {} {})", a[0], a[1], a[2]),
        Op::Occurs => bool(format!("(call $occurs {} {} {})", a[0], a[1], a[2])),
        Op::IndexOf => format!("(call $index_of {} {} {} {})", a[0], a[1], a[2], a[3]),
        Op::FromCodePoint => format!("(call $from_code_point {})", a[0]),
        Op::HasField => unreachable!("a field test is compiled where it is met"),
        Op::Without => unreachable!("a record's rest is compiled where it is met"),
        Op::WithEnv => format!("(call $with_env {} {})", a[0], a[1]),
    }
}

fn free_locals(term: &Term, binders: u32, out: &mut BTreeSet<u32>) {
    match term {
        Term::Local(i) => {
            if *i >= binders {
                out.insert(*i - binders);
            }
        }
        Term::Fn(labels, body) => free_locals(body, binders + labels.len() as u32, out),
        Term::Let(v, body) => {
            free_locals(v, binders, out);
            free_locals(body, binders + 1, out);
        }
        Term::App(c, args, _) => {
            free_locals(c, binders, out);
            args.iter().for_each(|a| free_locals(a, binders, out));
        }
        Term::If(a, b, c) => {
            free_locals(a, binders, out);
            free_locals(b, binders, out);
            free_locals(c, binders, out);
        }
        Term::And(a, b) | Term::Or(a, b) => {
            free_locals(a, binders, out);
            free_locals(b, binders, out);
        }
        Term::Field(_, t) => free_locals(t, binders, out),
        Term::Update(r, _, v) => {
            free_locals(r, binders, out);
            free_locals(v, binders, out);
        }
        Term::Record(fs) | Term::Dict(fs) => fs.iter().for_each(|(_, t)| free_locals(t, binders, out)),
        Term::List(items) | Term::Prim(_, items) => items.iter().for_each(|t| free_locals(t, binders, out)),
        Term::Host(_, args) => args.iter().for_each(|t| free_locals(t, binders, out)),
        Term::Task(t) | Term::Perform(t) => free_locals(t, binders, out),
        Term::Handle(t, caps) => {
            free_locals(t, binders, out);
            caps.iter().flat_map(|(_, ops)| ops).for_each(|(_, i)| free_locals(i, binders, out));
        }
        Term::Match(scrutinee, clauses, default) => {
            free_locals(scrutinee, binders, out);
            clauses.iter().for_each(|(p, r)| free_locals(r, binders + p.binders().len() as u32, out));
            if let Some(d) = default {
                free_locals(d, binders, out);
            }
        }
        Term::Int(_)
        | Term::Str(_)
        | Term::Bool(_)
        | Term::Nil
        | Term::Ref(_)
        | Term::CycleRef(_)
        | Term::Keyword(_) => {}
    }
}

/// A Bool term that says whether `value` matches `pattern`.
fn pattern_test(value: Term, pattern: &Pattern) -> Term {
    match pattern {
        Pattern::Keyword(k) => Term::Prim(Op::Eq, vec![value, Term::Keyword(k.clone())]),
        Pattern::Str(s) => Term::Prim(Op::Eq, vec![value, Term::Str(s.clone())]),
        Pattern::Int(n) => Term::Prim(Op::Eq, vec![value, Term::Int(*n)]),
        Pattern::Bool(b) => Term::Prim(Op::Eq, vec![value, Term::Bool(*b)]),
        Pattern::Any | Pattern::Bind(_) => Term::Bool(true),
        Pattern::Rest(_, p) => pattern_test(value, p),
        Pattern::List(items, rest) => {
            // The length first, then each item, read only once it is
            // known to be there.
            let count = Term::Prim(Op::Count, vec![value.clone()]);
            let n = Term::Int(items.len() as i64);
            let mut test = Term::Prim(if rest.is_some() { Op::Ge } else { Op::Eq }, vec![count, n]);
            for (i, p) in items.iter().enumerate() {
                match pattern_test(Term::Prim(Op::Nth, vec![value.clone(), Term::Int(i as i64)]), p) {
                    Term::Bool(true) => {}
                    t => test = Term::And(Box::new(test), Box::new(t)),
                }
            }
            test
        }
        Pattern::Fields(fields) => {
            // The value may be a keyword or a variant with another key,
            // so a field is read only once it is known to be there.
            let mut tests = fields.iter().map(|(name, p)| {
                let has = Term::Prim(Op::HasField, vec![value.clone(), Term::Str(name.clone())]);
                let test = pattern_test(Term::Field(name.clone(), Box::new(value.clone())), p);
                match test {
                    Term::Bool(true) => has,
                    test => Term::And(Box::new(has), Box::new(test)),
                }
            });
            let first = tests.next().expect("a record pattern has a field");
            tests.fold(first, |acc, t| Term::And(Box::new(acc), Box::new(t)))
        }
    }
}

/// A lambda's code id, as a definition's is its hash: the hash of the
/// lambda with each captured local numbered by its slot among the
/// captures, so the id depends on the lambda and not on where it is
/// written. A lambda in a recursive definition is hashed with the hashes
/// of the cycle it refers to. `free` holds the captured locals, as
/// indices from outside the lambda, in slot order.
fn lambda_code_id(labels: &[String], body: &Term, free: &[u32], cycle: &[Hash]) -> [u8; 8] {
    let arity = labels.len() as u32;
    let slots = |i: u32| arity + free.iter().position(|d| *d == i).expect("a captured local") as u32;
    let closed = Term::Fn(labels.to_vec(), Box::new(relocal(body, arity, &slots)));
    let mut bytes = b"lambda".to_vec();
    closed.encode(&mut bytes, &|i| i);
    if closed.is_recursive() {
        cycle.iter().for_each(|h| bytes.extend_from_slice(&h.0));
    }
    Hash::of(&bytes).0[..8].try_into().unwrap()
}

/// `term` with every local at or above `cutoff`, under that many binders,
/// renumbered: local `cutoff + i` becomes `cutoff + map(i)`.
fn relocal(term: &Term, cutoff: u32, map: &dyn Fn(u32) -> u32) -> Term {
    let s = |t: &Term| Box::new(relocal(t, cutoff, map));
    let all = |ts: &[Term]| ts.iter().map(|t| relocal(t, cutoff, map)).collect::<Vec<_>>();
    let entries =
        |fs: &[(String, Term)]| fs.iter().map(|(k, t)| (k.clone(), relocal(t, cutoff, map))).collect::<Vec<_>>();
    match term {
        Term::Local(i) if *i >= cutoff => Term::Local(cutoff + map(i - cutoff)),
        Term::Fn(labels, body) => Term::Fn(labels.clone(), Box::new(relocal(body, cutoff + labels.len() as u32, map))),
        Term::App(c, args, labels) => Term::App(s(c), all(args), labels.clone()),
        Term::Let(v, body) => Term::Let(s(v), Box::new(relocal(body, cutoff + 1, map))),
        Term::If(a, b, c) => Term::If(s(a), s(b), s(c)),
        Term::And(a, b) => Term::And(s(a), s(b)),
        Term::Or(a, b) => Term::Or(s(a), s(b)),
        Term::Record(fs) => Term::Record(entries(fs)),
        Term::Dict(fs) => Term::Dict(entries(fs)),
        Term::Field(k, t) => Term::Field(k.clone(), s(t)),
        Term::Update(r, k, v) => Term::Update(s(r), k.clone(), s(v)),
        Term::List(items) => Term::List(all(items)),
        Term::Prim(op, args) => Term::Prim(*op, all(args)),
        Term::Host(name, args) => Term::Host(name.clone(), all(args)),
        Term::Task(t) => Term::Task(s(t)),
        Term::Perform(t) => Term::Perform(s(t)),
        Term::Handle(t, caps) => Term::Handle(
            s(t),
            caps.iter()
                .map(|(c, ops)| (c.clone(), ops.iter().map(|(o, i)| (o.clone(), relocal(i, cutoff, map))).collect()))
                .collect(),
        ),
        Term::Match(x, clauses, default) => Term::Match(
            s(x),
            clauses.iter().map(|(p, r)| (p.clone(), relocal(r, cutoff + p.binders().len() as u32, map))).collect(),
            default.as_ref().map(|d| s(d)),
        ),
        other => other.clone(),
    }
}

/// `term` with every local at or above `cutoff` moved `by` binders out,
/// for placing it under `by` more binders.
fn shift(term: &Term, by: u32, cutoff: u32) -> Term {
    let s = |t: &Term| Box::new(shift(t, by, cutoff));
    let all = |ts: &[Term]| ts.iter().map(|t| shift(t, by, cutoff)).collect::<Vec<_>>();
    let entries = |fs: &[(String, Term)]| fs.iter().map(|(k, t)| (k.clone(), shift(t, by, cutoff))).collect::<Vec<_>>();
    match term {
        Term::Local(i) if *i >= cutoff => Term::Local(i + by),
        Term::Fn(labels, body) => Term::Fn(labels.clone(), Box::new(shift(body, by, cutoff + labels.len() as u32))),
        Term::App(c, args, labels) => Term::App(s(c), all(args), labels.clone()),
        Term::Let(v, body) => Term::Let(s(v), Box::new(shift(body, by, cutoff + 1))),
        Term::If(a, b, c) => Term::If(s(a), s(b), s(c)),
        Term::And(a, b) => Term::And(s(a), s(b)),
        Term::Or(a, b) => Term::Or(s(a), s(b)),
        Term::Record(fs) => Term::Record(entries(fs)),
        Term::Dict(fs) => Term::Dict(entries(fs)),
        Term::Field(k, t) => Term::Field(k.clone(), s(t)),
        Term::Update(r, k, v) => Term::Update(s(r), k.clone(), s(v)),
        Term::List(items) => Term::List(all(items)),
        Term::Prim(op, args) => Term::Prim(*op, all(args)),
        Term::Host(name, args) => Term::Host(name.clone(), all(args)),
        Term::Task(t) => Term::Task(s(t)),
        Term::Perform(t) => Term::Perform(s(t)),
        Term::Handle(t, caps) => Term::Handle(
            s(t),
            caps.iter()
                .map(|(c, ops)| (c.clone(), ops.iter().map(|(o, i)| (o.clone(), shift(i, by, cutoff))).collect()))
                .collect(),
        ),
        Term::Match(x, clauses, default) => Term::Match(
            s(x),
            clauses.iter().map(|(p, r)| (p.clone(), shift(r, by, cutoff + p.binders().len() as u32))).collect(),
            default.as_ref().map(|d| s(d)),
        ),
        other => other.clone(),
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
/// arguments returning its value. The caller owns what an export
/// returns, and gives it back with `conduct_drop`; `conduct_live` counts
/// the blocks allocated and not yet freed. Returns the module's text.
pub fn compile_exports(cb: &Codebase, exports: &[(&str, Hash)]) -> Result<String> {
    let mut b = Builder::new(cb);
    for (name, hash) in exports {
        b.require(*hash)?;
        let def_name = Builder::def_name(hash);
        match &cb.defs[hash].term {
            Term::Fn(labels, _) => {
                let arity = &(labels.len() as u32);
                let params: String = (0..*arity).map(|i| format!(" (param $a{i} i64)")).collect();
                let args: String = (0..*arity).map(|i| format!(" (local.get $a{i})")).collect();
                b.add_func(format!(
                    "  (func (export \"{name}\"){params} (result i64) (call {def_name} (i64.const {NIL}){args}))"
                ));
            }
            _ => b.add_export(format!("(export \"{name}\" (func {def_name}))")),
        }
    }
    finish_exports(b)
}

/// Finish a module, exporting what tests read: the live block count,
/// the allocation count, and a way to drop a value.
pub fn finish_exports(mut b: Builder) -> Result<String> {
    b.add_func("  (func (export \"conduct_live\") (result i32) (global.get $live))");
    b.add_func("  (func (export \"conduct_allocs\") (result i32) (global.get $allocs))");
    b.add_func("  (func (export \"conduct_drop\") (param $v i64) (call $drop (local.get $v)))");
    b.finish()
}

/// A value read back out of a module's memory.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Nil,
    Str(String),
    Keyword(String),
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
    Fetch {
        method: String,
        url: String,
        body: String,
        handler: String,
    },
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
    // A dictionary's key is a string or, set by `assoc`, a keyword.
    let str_of = |v: i64| match decode(memory, v) {
        Value::Str(s) | Value::Keyword(s) => s,
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
        KIND_KEYWORD => Value::Keyword(String::from_utf8(memory[p + 8..p + 8 + len].to_vec()).unwrap()),
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
        KIND_FETCH => Value::Fetch {
            method: str_of(i64_at(p + 8)),
            url: str_of(i64_at(p + 16)),
            body: str_of(i64_at(p + 24)),
            handler: str_of(i64_at(p + 32)),
        },
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
