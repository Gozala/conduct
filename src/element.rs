//! Elements of the TodoMVC demo's element world, written in conduct.
//!
//! An element is four pure definitions, in the Elm architecture's
//! shape, plus two constants:
//!
//! ```text
//! attributes : (list text)                    the attributes it observes
//! styles     : text                           the CSS of its shadow root
//! init       : ((\ (dict text)) : ?m)         its first model
//! update     : ((\ command model: ?m) : {model: ?m, effects: (list effect)})
//! view       : ((\ ?m) : view)
//! ```
//!
//! where `command` is `{action: keyword, kind: text, value?: text,
//! checked?: boolean, key?: text, status?: integer, attribute?: keyword}`:
//! an action, and the
//! attributes of its event that apply to it, matched by a clause,
//! `{action: :keydown key: "Enter"}`. The element prelude ([`PRELUDE`])
//! declares these attributes.
//!
//! `update` hears everything that happens to the element, by action:
//! the actions the view's events name, `{:click :toggle}`, `:connected`
//! when the element joins the document, and `:attribute-changed` (with
//! the attribute as a keyword, and its new value) after an observed
//! attribute changes: `init` sees the first attributes, and the model
//! keeps what the element needs of them. It performs nothing itself. It returns effects as values,
//! `(emit name detail)` for a custom event and
//! `(fetch method url body action)` for an HTTP request whose response
//! comes back to `update` as `action`, with the body as the command's
//! value and the status as its `status`. The glue performs them. So
//! element code stays pure, and the glue is the only code with effects.
//!
//! The glue, generated here, keeps each element's model and attributes
//! between calls and speaks the canonical ABI of the `demo:element`
//! world. The exports that run `update` (`connected`,
//! `attribute-changed`, `handle-event`) are lifted with the async ABI's
//! callback form, so that one whose `update` asks for requests returns
//! to the host while they are in flight, and finishes once every
//! response has reached `update` (see the `fetch` module). The others
//! are lifted synchronously, which the component model allows for an
//! `async` function type. An element imports `wasi:http` only if its
//! `update` can ask for a request.

use crate::codebase::Codebase;
use crate::codegen::{Builder, CodegenError, field_id};
use crate::fetch;
use crate::term::{Hash, Op, Term};
use crate::types::{Ty, TypeError};
use std::collections::HashSet;
use std::fmt::Write as _;

/// The attributes every element's code can use.
pub const PRELUDE: &str = include_str!("element.cdt");

/// A codebase for an element: the core attributes and the element
/// prelude's.
pub fn codebase() -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(PRELUDE).expect("the element prelude is valid");
    cb
}

/// The element world, as the demo defines it, without its WASI imports.
pub const ELEMENT_WIT: &str = include_str!("../wit/element.wit");
/// The element world with a memory count exported, for tests.
pub(crate) const DEBUG_WIT: &str = include_str!("../wit/debug.wit");
/// The WASI packages the element world names.
pub(crate) const WASI_WIT: [(&str, &str); 2] =
    [("clocks.wit", include_str!("../wit/deps/clocks.wit")), ("http.wit", include_str!("../wit/deps/http.wit"))];

/// What glue needs to pass strings over the canonical ABI: lowering a
/// string, lifting an argument the host allocated, and the allocator the
/// host allocates with.
pub(crate) const ABI_FUNCS: &str = r#"  (func $str_out (param $at i32) (param $s i64)
    ;; Lower a string to (ptr, len) at `at`. The bytes stay where they
    ;; are: whoever owns the string keeps it until `post-return`.
    (i32.store (local.get $at) (call $str_data (local.get $s)))
    (i32.store offset=4 (local.get $at) (call $len (local.get $s))))
  (func $arg_str (param $p i32) (param $n i32) (result i64)
    ;; Lift a string argument. The callee owns the buffer the host
    ;; allocated through `cabi_realloc`, so it frees it once copied. An
    ;; empty string may not have been allocated at all.
    (local $s i64)
    (local.set $s (call $str_new (local.get $p) (local.get $n)))
    (if (local.get $n) (then (call $free (local.get $p))))
    (local.get $s))
  (func $opt_str (param $disc i32) (param $p i32) (param $n i32) (result i64)
    (if (result i64) (local.get $disc)
      (then (call $arg_str (local.get $p) (local.get $n)))
      (else (i64.const 2))))
  (func (export "cabi_realloc") (param $old i32) (param $old_size i32) (param $align i32) (param $size i32) (result i32)
    (local $p i32)
    ;; Nothing is allocated for nothing: whoever lifts an empty string or
    ;; list frees nothing, whether or not the host asked for room.
    (if (i32.and (i32.eqz (local.get $old)) (i32.eqz (local.get $size))) (then (return (i32.const 8))))
    (local.set $p (call $alloc (local.get $size)))
    (if (local.get $old)
      (then
        (memory.copy (local.get $p) (local.get $old)
          (select (local.get $old_size) (local.get $size) (i32.lt_u (local.get $old_size) (local.get $size))))
        (call $free (local.get $old))))
    (local.get $p))"#;

#[derive(Debug)]
pub enum ElementError {
    Missing(&'static str),
    Contract { name: &'static str, error: TypeError },
    Codegen(CodegenError),
    Encode(String),
}

impl std::fmt::Display for ElementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementError::Missing(name) => write!(f, "an element needs a definition named `{name}`"),
            ElementError::Contract { name, error } => {
                write!(f, "`{name}` does not fit the element contract: {error}")
            }
            ElementError::Codegen(e) => write!(f, "{e}"),
            ElementError::Encode(e) => write!(f, "component encoding failed: {e}"),
        }
    }
}

impl std::error::Error for ElementError {}

impl From<CodegenError> for ElementError {
    fn from(e: CodegenError) -> Self {
        ElementError::Codegen(e)
    }
}

/// The definitions that make up an element.
pub struct ElementDefs {
    pub attributes: Option<Hash>,
    pub styles: Option<Hash>,
    pub init: Option<Hash>,
    pub update: Option<Hash>,
    pub view: Hash,
}

/// The global names of the contract's attributes, as the prelude claims
/// them.
const MODEL: &str = "conduct.element/model";
const EFFECTS: &str = "conduct.element/effects";
const ACTION: &str = "conduct.event/action";
const KIND: &str = "conduct.event/kind";
const VALUE: &str = "conduct.event/value";
const CHECKED: &str = "conduct.event/checked";
const KEY: &str = "conduct.event/key";
const ATTRIBUTE: &str = "conduct.event/attribute";
const STATUS: &str = "conduct.event/status";

/// The command `update` receives: its action, its event's kind, and
/// whichever of the others the event has.
pub fn event_type() -> Ty {
    Ty::record([
        (ACTION.to_string(), Ty::Prim(crate::types::Prim::Keyword)),
        (KIND.to_string(), Ty::str()),
        (VALUE.to_string(), Ty::opt(Ty::str())),
        (CHECKED.to_string(), Ty::opt(Ty::bool())),
        (KEY.to_string(), Ty::opt(Ty::str())),
        (STATUS.to_string(), Ty::opt(Ty::int())),
        (ATTRIBUTE.to_string(), Ty::opt(Ty::Prim(crate::types::Prim::Keyword))),
    ])
}

/// Find the element's definitions in the codebase's names, and check
/// their types against the contract. The model type is whatever `init`
/// returns; `update` and `view` must accept it, and `update` must
/// return it again.
///
/// The host calls them, and passes no methods: one that asks for them
/// is an error.
pub fn check(cb: &Codebase) -> Result<ElementDefs, ElementError> {
    let find = |name: &str| cb.names.get(name).copied();
    let defs = ElementDefs {
        attributes: find("attributes"),
        styles: find("styles"),
        init: find("init"),
        update: find("update"),
        view: find("view").ok_or(ElementError::Missing("view"))?,
    };
    let s = &cb.solver;
    let level = 1;
    if let Some(h) = cb.names.get("view") {
        passes_no_methods(cb, "view", *h).map_err(|error| ElementError::Contract { name: "view", error })?;
    }
    let ty_of = |h: Hash| s.instantiate(&cb.defs[&h].scheme, level);
    let fits = |name: &'static str, h: Option<Hash>, expected: Ty| -> Result<(), ElementError> {
        match h {
            Some(h) => {
                passes_no_methods(cb, name, h).map_err(|error| ElementError::Contract { name, error })?;
                s.constrain(&ty_of(h), &expected).map_err(|error| ElementError::Contract { name, error })
            }
            None => Ok(()),
        }
    };
    let attrs = || Ty::dict(Ty::str());
    let model = s.fresh_ty(level);
    fits("attributes", defs.attributes, Ty::list(Ty::str()))?;
    fits("styles", defs.styles, Ty::str())?;
    match defs.init {
        Some(_) => fits("init", defs.init, Ty::func(vec![attrs()], model.clone(), &[]))?,
        // With no `init`, the model is an empty record.
        None => s.constrain(&Ty::record([]), &model).map_err(|error| ElementError::Contract { name: "init", error })?,
    }
    fits(
        "update",
        defs.update,
        Ty::func(
            vec![event_type(), model.clone()],
            Ty::record([(MODEL.to_string(), model.clone()), (EFFECTS.to_string(), Ty::list(Ty::Effect))]),
            &["model"],
        ),
    )?;
    fits("view", Some(defs.view), Ty::func(vec![model], Ty::View, &[]))?;
    Ok(defs)
}

/// Visit every term of the definition `root` and of everything it
/// refers to.
pub(crate) fn walk_reachable(cb: &Codebase, root: Hash, f: &mut dyn FnMut(&Term)) {
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(hash) = stack.pop() {
        if !seen.insert(hash) {
            continue;
        }
        let Some(def) = cb.defs.get(&hash) else { continue };
        def.term.walk(&mut |t| f(t));
        def.term.refs(&mut stack);
        stack.extend(def.cycle.iter().copied());
    }
}

/// Whether the definition `root`, or anything it refers to, uses `op`.
fn reaches(cb: &Codebase, root: Hash, op: Op) -> bool {
    let mut found = false;
    walk_reachable(cb, root, &mut |t| {
        if let Term::Prim(o, _) = t
            && *o == op
        {
            found = true;
        }
    });
    found
}

/// The core module of an element, as WebAssembly text.
pub fn core_wat(cb: &Codebase) -> Result<String, ElementError> {
    let defs = check(cb)?;
    let mut b = Builder::new(cb);
    let iface = "demo:element/element";

    let attributes = match defs.attributes {
        Some(h) => b.value_of(h)?,
        None => "(i64.extend_i32_u (call $list_new (i32.const 0)))".to_string(),
    };
    let styles = match defs.styles {
        Some(h) => b.value_of(h)?,
        None => format!("(i64.const {})", b.static_str("")),
    };
    let init = match defs.init {
        Some(h) => b.call(h, &["(call $dup (local.get $attrs))".into()])?,
        None => format!("(i64.const {})", b.static_empty_record()),
    };
    // `update` takes the model out of the table and gives the next one
    // back; the table keeps the attributes, so `update` gets a copy.
    let update = match defs.update {
        Some(h) => b.call(h, &["(local.get $command)".into(), "(local.get $model)".into()])?,
        None => String::new(),
    };
    let view = b.call(defs.view, &["(call $dup (i64.load (call $slot (local.get $id))))".into()])?;
    let key = b.static_str("key");
    let checked = b.static_str("checked");
    let value = b.static_str("value");
    let true_str = b.static_str("true");
    let connected = b.static_str("connected");
    let changed = b.static_str("attribute-changed");
    let connected_action = b.static_keyword("connected");
    let changed_action = b.static_keyword("attribute-changed");

    b.add_import("(import \"demo:element/host\" \"emit\" (func $host_emit (param i32 i32 i32 i32 i32)))");
    for name in ["connected", "attribute-changed", "handle-event"] {
        b.add_import(format!(
            "(import \"[export]{iface}\" \"[task-return]{name}\" (func $return_{} ))",
            name.replace('-', "_")
        ));
    }
    // An element that makes requests gets the `wasi:http` glue, and its
    // tasks keep a waitable set in their context while requests are in
    // flight. Others import nothing of `wasi:http`.
    let fetching = defs.update.is_some_and(|h| reaches(cb, h, Op::Fetch));
    if fetching {
        for line in fetch::IMPORTS.lines() {
            b.add_import(line);
        }
        b.add_import("(import \"$root\" \"[context-get-0]\" (func $ctx_get (result i32)))");
        b.add_import("(import \"$root\" \"[context-set-0]\" (func $ctx_set (param i32)))");
        let zeros = b.static_str(&"\0".repeat(64)) + 8;
        let methods =
            ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH"].map(|m| b.static_str(m));
        let statics = fetch::Statics {
            zeros,
            content_type: b.static_str("content-type"),
            json: b.static_str("application/json"),
            response: b.static_str("response"),
            http: b.static_str("http://"),
            https: b.static_str("https://"),
            slash: b.static_str("/"),
            methods,
        };
        b.add_func(fetch::funcs(&statics));
    }

    // The table of elements: 16 bytes for each id, the model then the
    // attributes, both owned by the table. Ids start at 1.
    b.add_func(
        r#"  (global $els (mut i32) (i32.const 0))
  (global $cap (mut i32) (i32.const 0))
  (global $next (mut i32) (i32.const 1))
  (global $nidx (mut i32) (i32.const 0))
  (func $slot (param $id i32) (result i32)
    (if (i32.ge_u (local.get $id) (global.get $cap)) (then unreachable))
    (i32.add (global.get $els) (i32.shl (local.get $id) (i32.const 4))))
  (func $ensure (param $id i32)
    (local $cap i32) (local $new i32)
    (if (i32.lt_u (local.get $id) (global.get $cap)) (then (return)))
    (local.set $cap (i32.add (i32.shl (local.get $id) (i32.const 1)) (i32.const 16)))
    (local.set $new (call $alloc (i32.shl (local.get $cap) (i32.const 4))))
    (memory.fill (local.get $new) (i32.const 0) (i32.shl (local.get $cap) (i32.const 4)))
    (if (global.get $els)
      (then
        (memory.copy (local.get $new) (global.get $els) (i32.shl (global.get $cap) (i32.const 4)))
        (call $free (global.get $els))))
    (global.set $els (local.get $new))
    (global.set $cap (local.get $cap)))
"#,
    );
    b.add_func(ABI_FUNCS);

    // A view's nodes, flattened in document order: lists among the
    // children are spliced in, and an empty list adds nothing. The lowered
    // node list points into the view's strings, so the view and every
    // block allocated here are released in `post-return`.
    let mut f = String::new();
    let _ = write!(
        f,
        r#"  (func $count_nodes (param $v i64) (result i32)
    (local $n i32) (local $i i32) (local $k i32) (local $children i64)
    (if (i64.eq (local.get $v) (i64.const 2)) (then (return (i32.const 0))))
    (local.set $k (call $kind (local.get $v)))
    (if (call $is_list (local.get $v))
      (then
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (call $len (local.get $v))))
          (local.set $n (i32.add (local.get $n) (call $count_nodes (call $list_get (local.get $v) (local.get $i)))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return (local.get $n))))
    (if (i32.ne (local.get $k) (i32.const 6)) (then (return (i32.const 1))))
    (local.set $children (i64.load offset=24 (call $ptr (local.get $v))))
    (i32.add (i32.const 1) (call $count_nodes (local.get $children))))
  (func $is_named (param $s i64) (param $name i64) (result i32)
    (call $str_eq (local.get $s) (local.get $name)))
  (func $scratch (param $n i32) (result i32)
    ;; A raw block released in `post-return`.
    (local $p i32)
    (local.set $p (call $alloc (local.get $n)))
    (call $defer_raw (local.get $p))
    (local.get $p))
  (func $flatten (param $v i64) (param $parent i32) (param $arr i32)
    (local $k i32) (local $i i32) (local $n i32) (local $e i32) (local $idx i32)
    (local $attrs i64) (local $events i64) (local $name i64) (local $val i64) (local $s i64)
    (local $na i32) (local $np i32) (local $pa i32) (local $pp i32) (local $pe i32) (local $at i32)
    (if (i64.eq (local.get $v) (i64.const 2)) (then (return)))
    (local.set $k (call $kind (local.get $v)))
    (if (call $is_list (local.get $v))
      (then
        (local.set $n (call $len (local.get $v)))
        (block $done (loop $next
          (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
          (call $flatten (call $list_get (local.get $v) (local.get $i)) (local.get $parent) (local.get $arr))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $next)))
        (return)))
    (local.set $idx (global.get $nidx))
    (global.set $nidx (i32.add (local.get $idx) (i32.const 1)))
    (local.set $e (i32.add (local.get $arr) (i32.mul (local.get $idx) (i32.const 56))))
    ;; parent: option<u32> at 0
    (if (i32.ge_s (local.get $parent) (i32.const 0))
      (then (i32.store8 (local.get $e) (i32.const 1)) (i32.store offset=4 (local.get $e) (local.get $parent)))
      (else (i32.store8 (local.get $e) (i32.const 0))))
    (if (i32.ne (local.get $k) (i32.const 6))
      (then
        ;; A text node: key none at 8, kind text (1) at 20, string at 24.
        ;; Its text is a new string unless the child already is one.
        (local.set $s (call $to_str (local.get $v)))
        (call $defer (local.get $s))
        (i32.store8 offset=8 (local.get $e) (i32.const 0))
        (i32.store8 offset=20 (local.get $e) (i32.const 1))
        (call $str_out (i32.add (local.get $e) (i32.const 24)) (local.get $s))
        (return)))
    (local.set $attrs (i64.load offset=16 (call $ptr (local.get $v))))
    (local.set $events (i64.load offset=32 (call $ptr (local.get $v))))
    ;; key: option<string> at 8. The attributes keep the key alive.
    (local.set $s (call $dict_get (local.get $attrs) (i64.const {key})))
    (if (i64.ne (local.get $s) (i64.const 2))
      (then (i32.store8 offset=8 (local.get $e) (i32.const 1)) (call $str_out (i32.add (local.get $e) (i32.const 12)) (local.get $s)))
      (else (i32.store8 offset=8 (local.get $e) (i32.const 0))))
    (call $drop (local.get $s))
    ;; kind: element (0) at 20, element-node at 24
    (i32.store8 offset=20 (local.get $e) (i32.const 0))
    (call $str_out (i32.add (local.get $e) (i32.const 24)) (i64.load offset=8 (call $ptr (local.get $v))))
    ;; Split the attributes: `key` names the node, `checked` and `value`
    ;; become properties, the rest stay attributes.
    (local.set $n (call $len (local.get $attrs)))
    (local.set $pa (call $scratch (i32.shl (local.get $n) (i32.const 4))))
    (local.set $pp (call $scratch (i32.mul (local.get $n) (i32.const 20))))
    (local.set $i (i32.const 0))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $name (call $dict_key (local.get $attrs) (local.get $i)))
      (local.set $val (call $dict_val (local.get $attrs) (local.get $i)))
      (if (i32.eqz (call $is_named (local.get $name) (i64.const {key})))
        (then
          (if (call $is_named (local.get $name) (i64.const {checked}))
            (then
              (local.set $at (i32.add (local.get $pp) (i32.mul (local.get $np) (i32.const 20))))
              (call $str_out (local.get $at) (local.get $name))
              (i32.store8 offset=8 (local.get $at) (i32.const 1))
              (i32.store8 offset=12 (local.get $at) (call $str_eq (local.get $val) (i64.const {true_str})))
              (local.set $np (i32.add (local.get $np) (i32.const 1))))
            (else
              (if (call $is_named (local.get $name) (i64.const {value}))
                (then
                  (local.set $at (i32.add (local.get $pp) (i32.mul (local.get $np) (i32.const 20))))
                  (call $str_out (local.get $at) (local.get $name))
                  (i32.store8 offset=8 (local.get $at) (i32.const 0))
                  (call $str_out (i32.add (local.get $at) (i32.const 12)) (local.get $val))
                  (local.set $np (i32.add (local.get $np) (i32.const 1))))
                (else
                  (local.set $at (i32.add (local.get $pa) (i32.shl (local.get $na) (i32.const 4))))
                  (call $str_out (local.get $at) (local.get $name))
                  (call $str_out (i32.add (local.get $at) (i32.const 8)) (local.get $val))
                  (local.set $na (i32.add (local.get $na) (i32.const 1)))))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i32.store offset=32 (local.get $e) (local.get $pa))
    (i32.store offset=36 (local.get $e) (local.get $na))
    (i32.store offset=40 (local.get $e) (local.get $pp))
    (i32.store offset=44 (local.get $e) (local.get $np))
    ;; events: list<tuple<string, string>> at 48
    (local.set $n (call $len (local.get $events)))
    (local.set $pe (call $scratch (i32.shl (local.get $n) (i32.const 4))))
    (local.set $i (i32.const 0))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $at (i32.add (local.get $pe) (i32.shl (local.get $i) (i32.const 4))))
      (call $str_out (local.get $at) (call $dict_key (local.get $events) (local.get $i)))
      (call $str_out (i32.add (local.get $at) (i32.const 8)) (call $dict_val (local.get $events) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i32.store offset=48 (local.get $e) (local.get $pe))
    (i32.store offset=52 (local.get $e) (local.get $n))
    (call $flatten (i64.load offset=24 (call $ptr (local.get $v))) (local.get $idx) (local.get $arr)))"#
    );
    b.add_func(f);

    // The exports of `demo:element/element`, each lifted synchronously.
    // An export that returns memory releases what it lent at its
    // `post-return`, and every export first releases anything a
    // previous one lent, in case a host never called `post-return`.
    let mut x = String::new();
    let _ = write!(
        x,
        r#"  (func (export "{iface}#observed-attributes") (result i32)
    (local $v i64) (local $n i32) (local $arr i32) (local $i i32)
    (call $release)
    (local.set $v {attributes})
    (call $defer (local.get $v))
    (local.set $n (call $len (local.get $v)))
    (local.set $arr (call $scratch (i32.shl (local.get $n) (i32.const 3))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (call $str_out (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 3))) (call $list_get (local.get $v) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i32.store (i32.const 0) (local.get $arr))
    (i32.store (i32.const 4) (local.get $n))
    (i32.const 0))
  (func (export "cabi_post_{iface}#observed-attributes") (param i32) (call $release))
  (func (export "{iface}#styles") (result i32)
    (local $s i64)
    (call $release)
    (local.set $s {styles})
    (call $defer (local.get $s))
    (call $str_out (i32.const 0) (local.get $s))
    (i32.const 0))
  (func (export "cabi_post_{iface}#styles") (param i32) (call $release))
  (func (export "{iface}#create") (param $p i32) (param $n i32) (result i32)
    (local $attrs i64) (local $key i64) (local $val i64) (local $next_attrs i64) (local $i i32) (local $at i32) (local $id i32)
    (call $release)
    ;; Each attribute is inserted where it sorts, so the dictionary's
    ;; order does not depend on the order the host gives them in.
    (local.set $attrs (i64.extend_i32_u (call $dict_new (i32.const 0))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $at (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))
      (local.set $key (call $arg_str (i32.load (local.get $at)) (i32.load offset=4 (local.get $at))))
      (local.set $val (call $arg_str (i32.load offset=8 (local.get $at)) (i32.load offset=12 (local.get $at))))
      (local.set $next_attrs (call $dict_assoc (local.get $attrs) (local.get $key) (local.get $val)))
      (call $drop (local.get $attrs))
      (call $drop (local.get $key))
      (call $drop (local.get $val))
      (local.set $attrs (local.get $next_attrs))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (local.get $n) (then (call $free (local.get $p))))
    (local.set $id (global.get $next))
    (global.set $next (i32.add (local.get $id) (i32.const 1)))
    (call $ensure (local.get $id))
    (i64.store offset=8 (call $slot (local.get $id)) (local.get $attrs))
    (i64.store (call $slot (local.get $id)) {init})
    (local.get $id))
  (func (export "[async-lift]{iface}#connected") (param $id i32) (result i32)
    (call $release)
    (global.set $which (i32.const 0))
    (call $dispatch (local.get $id)
      (call $event_new (call $lit_owned (i64.const {connected_action})) (call $lit_owned (i64.const {connected}))
        (i64.const 2) (i64.const 2) (i64.const 2) (i64.const 2) (i64.const 2)))
    (call $task_end))
  (func (export "{iface}#disconnected") (param $id i32)
    (call $release)
    (call $drop (i64.load (call $slot (local.get $id))))
    (call $drop (i64.load offset=8 (call $slot (local.get $id))))
    (i64.store (call $slot (local.get $id)) (i64.const 0))
    (i64.store offset=8 (call $slot (local.get $id)) (i64.const 0)))
  (func (export "[async-lift]{iface}#attribute-changed") (param $id i32) (param $np i32) (param $nn i32) (param $disc i32) (param $vp i32) (param $vn i32) (result i32)
    (local $attrs i64) (local $name i64) (local $val i64)
    (call $release)
    (global.set $which (i32.const 1))
    (local.set $attrs (i64.load offset=8 (call $slot (local.get $id))))
    (local.set $name (call $arg_str (local.get $np) (local.get $nn)))
    (local.set $val (call $opt_str (local.get $disc) (local.get $vp) (local.get $vn)))
    (i64.store offset=8 (call $slot (local.get $id))
      (if (result i64) (local.get $disc)
        (then (call $dict_assoc (local.get $attrs) (local.get $name) (local.get $val)))
        (else (call $dict_dissoc (local.get $attrs) (local.get $name)))))
    (call $drop (local.get $attrs))
    ;; Tell `update`: the attribute, by its name as a keyword, and its
    ;; new value, if it still has one.
    (call $dispatch (local.get $id)
      (call $event_new (call $lit_owned (i64.const {changed_action})) (call $lit_owned (i64.const {changed}))
        (local.get $val) (i64.const 2) (i64.const 2) (i64.const 2)
        (call $keyword_of (call $str_new (call $str_data (local.get $name)) (call $len (local.get $name))))))
    (call $drop (local.get $name))
    (call $task_end))
  (func (export "{iface}#render") (param $id i32) (result i32)
    (local $v i64) (local $n i32) (local $arr i32)
    (call $release)
    (local.set $v {view})
    (call $defer (local.get $v))
    (local.set $n (call $count_nodes (local.get $v)))
    (local.set $arr (call $scratch (i32.mul (local.get $n) (i32.const 56))))
    (global.set $nidx (i32.const 0))
    (call $flatten (local.get $v) (i32.const -1) (local.get $arr))
    (i32.store (i32.const 0) (local.get $arr))
    (i32.store (i32.const 4) (local.get $n))
    (i32.const 0))
  (func (export "cabi_post_{iface}#render") (param i32) (call $release))
"#
    );
    // An event record, fields sorted by id as every record is. Each
    // argument is consumed. An attribute the event does not have comes
    // as the glue's internal "absent" (2), and the record leaves it out.
    let mut fields = [
        (field_id(ATTRIBUTE), "attribute"),
        (field_id(ACTION), "action"),
        (field_id(KIND), "kind"),
        (field_id(VALUE), "value"),
        (field_id(CHECKED), "checked"),
        (field_id(KEY), "key"),
        (field_id(STATUS), "status"),
    ];
    fields.sort_by_key(|f| f.0);
    let mut count = String::new();
    let mut stores = String::new();
    for (id, v) in fields.iter() {
        let _ = write!(
            count,
            " (if (i64.ne (local.get ${v}) (i64.const 2)) (then (local.set $n (i32.add (local.get $n) (i32.const 1)))))"
        );
        let _ = write!(
            stores,
            "\n    (if (i64.ne (local.get ${v}) (i64.const 2)) (then (i64.store offset=8 (local.get $at) (i64.const {id})) (i64.store offset=16 (local.get $at) (local.get ${v})) (local.set $at (i32.add (local.get $at) (i32.const 16)))))"
        );
    }
    let keyword = crate::codegen::KIND_KEYWORD;
    let _ = write!(
        x,
        r#"  (func $event_new (param $action i64) (param $kind i64) (param $value i64) (param $checked i64) (param $key i64) (param $status i64) (param $attribute i64) (result i64)
    (local $w i32) (local $n i32) (local $at i32){count}
    (local.set $w (call $obj (i32.const 3) (local.get $n) (i32.shl (local.get $n) (i32.const 4))))
    (local.set $at (local.get $w)){stores}
    (i64.extend_i32_u (local.get $w)))
  (func $lit_owned (param $s i64) (result i64)
    (call $dup (local.get $s)))
  (func $keyword_of (param $s i64) (result i64)
    ;; A string just lifted, which nothing else holds, as the keyword of
    ;; its name.
    (i32.store (call $ptr (local.get $s)) (i32.const {keyword}))
    (local.get $s))
"#
    );
    // Run `update` on one element and perform the effects it returns.
    let dispatch = if update.is_empty() {
        "    (call $drop (local.get $command))".to_string()
    } else {
        format!(
            r#"    ;; An element that left meanwhile hears nothing more.
    (if (i64.eqz (i64.load offset=8 (call $slot (local.get $id))))
      (then (call $drop (local.get $command)) (return)))
    (local.set $model (i64.load (call $slot (local.get $id))))
    (i64.store (call $slot (local.get $id)) (i64.const 2))
    (local.set $r {update})
    (i64.store (call $slot (local.get $id)) (call $rec_get (local.get $r) (i64.const {model})))
    (local.set $effects (call $rec_get (local.get $r) (i64.const {effects})))
    (call $drop (local.get $r))
    (local.set $n (call $len (local.get $effects)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $e (call $ptr (call $list_get (local.get $effects) (local.get $i))))
      ;; A custom event goes out through the host; a request starts in
      ;; the current task.
      (if (i32.eq (i32.load (local.get $e)) (i32.const 7))
        (then
          (call $host_emit (local.get $id)
            (call $str_data (i64.load offset=8 (local.get $e))) (call $len (i64.load offset=8 (local.get $e)))
            (call $str_data (i64.load offset=16 (local.get $e))) (call $len (i64.load offset=16 (local.get $e))))))
{start_fetch}
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (call $drop (local.get $effects))"#,
            model = field_id(MODEL),
            effects = field_id(EFFECTS),
            start_fetch = if fetching {
                "      (if (i32.eq (i32.load (local.get $e)) (i32.const 8))\n        (then (call $fetch_start (local.get $id) (local.get $e) (call $task_set))))"
            } else {
                ""
            },
        )
    };
    let _ = write!(
        x,
        r#"  (func $dispatch (param $id i32) (param $command i64)
    (local $model i64) (local $r i64) (local $effects i64) (local $n i32) (local $i i32) (local $e i32)
{dispatch})
  (func (export "[async-lift]{iface}#handle-event") (param $id i32) (param $hp i32) (param $hn i32)
    (param $kp i32) (param $kn i32) (param $vd i32) (param $vp i32) (param $vn i32)
    (param $cd i32) (param $cv i32) (param $yd i32) (param $yp i32) (param $yn i32) (result i32)
    (call $release)
    (global.set $which (i32.const 2))
    (call $dispatch (local.get $id)
      (call $event_new
        (call $keyword_of (call $arg_str (local.get $hp) (local.get $hn)))
        (call $arg_str (local.get $kp) (local.get $kn))
        (call $opt_str (local.get $vd) (local.get $vp) (local.get $vn))
        (if (result i64) (local.get $cd) (then (call $bool (local.get $cv))) (else (i64.const 2)))
        (call $opt_str (local.get $yd) (local.get $yp) (local.get $yn))
        (i64.const 2) (i64.const 2)))
    (call $task_end))"#
    );
    b.add_func(x);
    b.add_func(task_funcs(fetching, iface));
    // For tests: how many blocks are allocated and not yet freed.
    b.add_func(
        "  (func (export \"conduct_live\") (export \"conduct:debug/memory#live\") (result i32) (global.get $live))",
    );
    Ok(b.finish()?)
}

/// The end of a task that runs `update`, and the callback that carries
/// it on. A task returns once none of its requests waits on anything;
/// until then it waits on its waitable set, which its context holds with
/// which export started it.
fn task_funcs(fetching: bool, iface: &str) -> String {
    let returns = r#"    (if (i32.eqz (local.get $which)) (then (call $return_connected)))
    (if (i32.eq (local.get $which) (i32.const 1)) (then (call $return_attribute_changed)))
    (if (i32.eq (local.get $which) (i32.const 2)) (then (call $return_handle_event)))
    (i32.const 0))"#;
    let mut out = String::from("  (global $which (mut i32) (i32.const 0))\n");
    if fetching {
        out.push_str(
            r#"  (func $task_set (result i32)
    (local $ctx i32)
    (local.set $ctx (call $ctx_get))
    (if (i32.eqz (local.get $ctx))
      (then
        (local.set $ctx (call $alloc (i32.const 8)))
        (i32.store (local.get $ctx) (call $set_new))
        (i32.store offset=4 (local.get $ctx) (global.get $which))
        (call $ctx_set (local.get $ctx))))
    (i32.load (local.get $ctx)))
  (func $task_end (result i32)
    (local $ctx i32) (local $which i32)
    (local.set $ctx (call $ctx_get))
    (local.set $which (global.get $which))
    (if (local.get $ctx)
      (then
        (if (call $fetch_pending (i32.load (local.get $ctx)))
          (then (return (i32.or (i32.const 2) (i32.shl (i32.load (local.get $ctx)) (i32.const 4))))))
        (local.set $which (i32.load offset=4 (local.get $ctx)))
        (call $set_drop (i32.load (local.get $ctx)))
        (call $free (local.get $ctx))
        (call $ctx_set (i32.const 0))))
"#,
        );
        out.push_str(returns);
        out.push_str(
            r#"
  (func $callback (param $event i32) (param $w i32) (param $payload i32) (result i32)
    (call $fetch_event (local.get $event) (local.get $w) (local.get $payload))
    (call $task_end))
"#,
        );
    } else {
        out.push_str(
            "  (func $task_end (result i32)\n    (local $which i32)\n    (local.set $which (global.get $which))\n",
        );
        out.push_str(returns);
        out.push_str("\n  (func $callback (param i32 i32 i32) (result i32) (call $task_end))\n");
    }
    for name in ["connected", "attribute-changed", "handle-event"] {
        out.push_str(&format!("  (export \"[callback][async-lift]{iface}#{name}\" (func $callback))\n"));
    }
    out
}

/// Compile the element in `cb` to a component of the
/// `element-component` world.
pub fn component(cb: &Codebase) -> Result<Vec<u8>, ElementError> {
    encode(cb, false)
}

/// The element as [`component`] compiles it, plus an export
/// `conduct:debug/memory#live` that counts the blocks its memory holds,
/// for tests.
pub fn debug_component(cb: &Codebase) -> Result<Vec<u8>, ElementError> {
    encode(cb, true)
}

fn encode(cb: &Codebase, debug: bool) -> Result<Vec<u8>, ElementError> {
    let wat = core_wat(cb)?;
    let mut core = wat::parse_str(&wat).map_err(|e| ElementError::Encode(format!("{e}")))?;
    let mut resolve = wit_parser::Resolve::default();
    for (name, wit) in WASI_WIT {
        resolve.push_str(name, wit).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
    }
    let mut pkg = resolve.push_str("element.wit", ELEMENT_WIT).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
    if debug {
        resolve.push_str("route.wit", crate::route::ROUTE_WIT).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
        pkg = resolve.push_str("debug.wit", DEBUG_WIT).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
    }
    let world =
        resolve.select_world(&[pkg], Some("element-component")).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
    wit_component::embed_component_metadata(&mut core, &resolve, world, wit_component::StringEncoding::UTF8, false)
        .map_err(|e| ElementError::Encode(format!("{e:#}")))?;
    wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&core)
        .and_then(|e| e.encode())
        .map_err(|e| ElementError::Encode(format!("{e:#}")))
}

/// What the host calls is passed its arguments, and no methods: a
/// definition it calls must not ask for them.
pub fn passes_no_methods(cb: &Codebase, name: &str, h: Hash) -> Result<(), TypeError> {
    match cb.defs[&h].scheme.requires.first() {
        None => Ok(()),
        Some(r) => Err(TypeError(format!(
            "`{name}` asks for `{}` for a type its callers say, and the host passes no methods: call it on values of \
             a known type",
            r.method
        ))),
    }
}
