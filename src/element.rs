//! Elements of the TodoMVC demo's element world, written in conduct.
//!
//! An element is four pure definitions, in the Elm architecture's
//! shape, plus two constants:
//!
//! ```text
//! attributes : List[String]          the attributes it observes
//! styles     : String                the CSS of its shadow root
//! init       : (Dict[String]) -> m   its first model, from its attributes
//! update     : (m, String, Event, Dict[String]) -> {model: m, emit: List[Emit]}
//! view       : (m, Dict[String]) -> View
//! ```
//!
//! where `Event` is `{kind: String, value: String | nil,
//! checked: Bool | nil, key: String | nil}`. The handler name `update`
//! receives is the one the view's events named. Custom events are not
//! sent by `update`: it returns them, as `(emit name detail)` values,
//! and the glue sends them. So element code stays pure, and the glue is
//! the only code with effects.
//!
//! The glue, generated here, keeps each element's model and attributes
//! between calls and speaks the canonical ABI of the `demo:element`
//! world. Every export is lifted synchronously, though the world
//! declares them `async`: the component model lets a synchronous lift
//! implement an `async` function type, and it keeps the async ABI out
//! of the compiler for now.

use crate::codebase::Codebase;
use crate::codegen::{Builder, CodegenError, NIL, field_id};
use crate::term::Hash;
use crate::types::{Ty, TypeError};
use std::fmt::Write as _;

/// The element world, as the demo defines it, without its WASI imports.
pub const ELEMENT_WIT: &str = include_str!("../wit/element.wit");

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

/// The record a handler receives.
pub fn event_type(cb: &Codebase, level: u32) -> Ty {
    let optional = |t: Ty| {
        let v = cb.solver.fresh(level);
        v.lower.borrow_mut().push(t);
        v.lower.borrow_mut().push(Ty::nil());
        Ty::Var(v)
    };
    Ty::record([
        ("kind".to_string(), Ty::str()),
        ("value".to_string(), optional(Ty::str())),
        ("checked".to_string(), optional(Ty::bool())),
        ("key".to_string(), optional(Ty::str())),
    ])
}

/// Find the element's definitions in the codebase's names, and check
/// their types against the contract. The model type is whatever `init`
/// returns; `update` and `view` must accept it, and `update` must
/// return it again.
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
    let ty_of = |h: Hash| s.instantiate(&cb.defs[&h].scheme, level);
    let fits = |name: &'static str, h: Option<Hash>, expected: Ty| -> Result<(), ElementError> {
        match h {
            Some(h) => s.constrain(&ty_of(h), &expected).map_err(|error| ElementError::Contract { name, error }),
            None => Ok(()),
        }
    };
    let attrs = || Ty::dict(Ty::str());
    let model = s.fresh_ty(level);
    fits("attributes", defs.attributes, Ty::list(Ty::str()))?;
    fits("styles", defs.styles, Ty::str())?;
    match defs.init {
        Some(_) => fits("init", defs.init, Ty::func(vec![attrs()], model.clone()))?,
        // With no `init`, the model is nil.
        None => s.constrain(&Ty::nil(), &model).map_err(|error| ElementError::Contract { name: "init", error })?,
    }
    fits(
        "update",
        defs.update,
        Ty::func(
            vec![model.clone(), Ty::str(), event_type(cb, level), attrs()],
            Ty::record([("model".to_string(), model.clone()), ("emit".to_string(), Ty::list(Ty::Emit))]),
        ),
    )?;
    fits("view", Some(defs.view), Ty::func(vec![model, attrs()], Ty::View))?;
    Ok(defs)
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
        None => format!("(i64.const {NIL})"),
    };
    // `update` takes the model out of the table and gives the next one
    // back; the table keeps the attributes, so `update` gets a copy.
    let update = match defs.update {
        Some(h) => b.call(
            h,
            &[
                "(local.get $model)".into(),
                "(local.get $handler)".into(),
                "(local.get $event)".into(),
                "(call $dup (i64.load offset=8 (call $slot (local.get $id))))".into(),
            ],
        )?,
        None => String::new(),
    };
    let view = b.call(
        defs.view,
        &[
            "(call $dup (i64.load (call $slot (local.get $id))))".into(),
            "(call $dup (i64.load offset=8 (call $slot (local.get $id))))".into(),
        ],
    )?;
    let key = b.static_str("key");
    let checked = b.static_str("checked");
    let value = b.static_str("value");
    let true_str = b.static_str("true");

    b.add_import("(import \"demo:element/host\" \"emit\" (func $host_emit (param i32 i32 i32 i32 i32)))");

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
  (func $str_out (param $at i32) (param $s i64)
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
    (local.set $p (call $alloc (local.get $size)))
    (if (local.get $old)
      (then
        (memory.copy (local.get $p) (local.get $old)
          (select (local.get $old_size) (local.get $size) (i32.lt_u (local.get $old_size) (local.get $size))))
        (call $free (local.get $old))))
    (local.get $p))"#,
    );

    // A view's nodes, flattened in document order: lists among the
    // children are spliced in, and nil children are skipped. The lowered
    // node list points into the view's strings, so the view and every
    // block allocated here are released in `post-return`.
    let mut f = String::new();
    let _ = write!(
        f,
        r#"  (func $count_nodes (param $v i64) (result i32)
    (local $n i32) (local $i i32) (local $k i32) (local $children i64)
    (if (i64.eq (local.get $v) (i64.const 2)) (then (return (i32.const 0))))
    (local.set $k (call $kind (local.get $v)))
    (if (i32.eq (local.get $k) (i32.const 2))
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
    (if (i32.eq (local.get $k) (i32.const 2))
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
    (local $attrs i64) (local $d i32) (local $i i32) (local $at i32) (local $id i32)
    (call $release)
    (local.set $d (call $dict_new (local.get $n)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $at (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))))
      (call $dict_put (local.get $d) (local.get $i)
        (call $arg_str (i32.load (local.get $at)) (i32.load offset=4 (local.get $at)))
        (call $arg_str (i32.load offset=8 (local.get $at)) (i32.load offset=12 (local.get $at))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (local.get $n) (then (call $free (local.get $p))))
    (local.set $attrs (i64.extend_i32_u (local.get $d)))
    (local.set $id (global.get $next))
    (global.set $next (i32.add (local.get $id) (i32.const 1)))
    (call $ensure (local.get $id))
    (i64.store offset=8 (call $slot (local.get $id)) (local.get $attrs))
    (i64.store (call $slot (local.get $id)) {init})
    (local.get $id))
  (func (export "{iface}#connected") (param $id i32)
    (call $release))
  (func (export "{iface}#disconnected") (param $id i32)
    (call $release)
    (call $drop (i64.load (call $slot (local.get $id))))
    (call $drop (i64.load offset=8 (call $slot (local.get $id))))
    (i64.store (call $slot (local.get $id)) (i64.const 0))
    (i64.store offset=8 (call $slot (local.get $id)) (i64.const 0)))
  (func (export "{iface}#attribute-changed") (param $id i32) (param $np i32) (param $nn i32) (param $disc i32) (param $vp i32) (param $vn i32)
    (local $attrs i64) (local $name i64) (local $val i64)
    (call $release)
    (local.set $attrs (i64.load offset=8 (call $slot (local.get $id))))
    (local.set $name (call $arg_str (local.get $np) (local.get $nn)))
    (local.set $val (call $opt_str (local.get $disc) (local.get $vp) (local.get $vn)))
    (i64.store offset=8 (call $slot (local.get $id))
      (if (result i64) (local.get $disc)
        (then (call $dict_assoc (local.get $attrs) (local.get $name) (local.get $val)))
        (else (call $dict_dissoc (local.get $attrs) (local.get $name)))))
    (call $drop (local.get $attrs))
    (call $drop (local.get $name))
    (call $drop (local.get $val)))
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
    // handle-event: (id, handler, event{kind, value?, checked?, key?})
    // flattens to 13 parameters.
    let mut fields = [
        (field_id("kind"), "(call $arg_str (local.get $kp) (local.get $kn))".to_string()),
        (field_id("value"), "(call $opt_str (local.get $vd) (local.get $vp) (local.get $vn))".to_string()),
        (
            field_id("checked"),
            "(if (result i64) (local.get $cd) (then (call $bool (local.get $cv))) (else (i64.const 2)))".to_string(),
        ),
        (field_id("key"), "(call $opt_str (local.get $yd) (local.get $yp) (local.get $yn))".to_string()),
    ];
    fields.sort_by_key(|f| f.0);
    let mut event = String::from("(local.set $w (call $obj (i32.const 3) (i32.const 4) (i32.const 64)))");
    for (i, (id, v)) in fields.iter().enumerate() {
        let _ = write!(
            event,
            " (i64.store offset={} (local.get $w) (i64.const {id})) (i64.store offset={} (local.get $w) {v})",
            8 + 16 * i,
            16 + 16 * i
        );
    }
    let handle = if update.is_empty() {
        // No `update`: the event is lifted, so its buffers are freed, and
        // dropped.
        "    (call $drop (local.get $handler))\n    (call $drop (local.get $event))".to_string()
    } else {
        format!(
            r#"    (local.set $model (i64.load (call $slot (local.get $id))))
    (i64.store (call $slot (local.get $id)) (i64.const 2))
    (local.set $r {update})
    (i64.store (call $slot (local.get $id)) (call $rec_get (local.get $r) (i64.const {model})))
    (local.set $emits (call $rec_get (local.get $r) (i64.const {emit})))
    (call $drop (local.get $r))
    (local.set $n (call $len (local.get $emits)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $e (call $ptr (call $list_get (local.get $emits) (local.get $i))))
      (call $host_emit (local.get $id)
        (call $str_data (i64.load offset=8 (local.get $e))) (call $len (i64.load offset=8 (local.get $e)))
        (call $str_data (i64.load offset=16 (local.get $e))) (call $len (i64.load offset=16 (local.get $e))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (call $drop (local.get $emits))"#,
            model = field_id("model"),
            emit = field_id("emit"),
        )
    };
    let _ = write!(
        x,
        r#"  (func (export "{iface}#handle-event") (param $id i32) (param $hp i32) (param $hn i32)
    (param $kp i32) (param $kn i32) (param $vd i32) (param $vp i32) (param $vn i32)
    (param $cd i32) (param $cv i32) (param $yd i32) (param $yp i32) (param $yn i32)
    (local $handler i64) (local $event i64) (local $model i64) (local $r i64) (local $emits i64) (local $w i32)
    (local $n i32) (local $i i32) (local $e i32)
    (call $release)
    (local.set $handler (call $arg_str (local.get $hp) (local.get $hn)))
    {event}
    (local.set $event (i64.extend_i32_u (local.get $w)))
{handle})"#
    );
    b.add_func(x);
    // For tests: how many blocks are allocated and not yet freed.
    b.add_func("  (func (export \"conduct_live\") (result i32) (global.get $live))");
    Ok(b.finish()?)
}

/// Compile the element in `cb` to a component of the
/// `element-component` world.
pub fn component(cb: &Codebase) -> Result<Vec<u8>, ElementError> {
    let wat = core_wat(cb)?;
    let mut core = wat::parse_str(&wat).map_err(|e| ElementError::Encode(format!("{e}")))?;
    let mut resolve = wit_parser::Resolve::default();
    let pkg = resolve.push_str("element.wit", ELEMENT_WIT).map_err(|e| ElementError::Encode(format!("{e:#}")))?;
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
