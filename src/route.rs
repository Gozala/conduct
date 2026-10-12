//! Routes of the TodoMVC demo's route world, written in conduct.
//!
//! A route is one function with effects, in Roc's style, and an
//! optional constant:
//!
//! ```text
//! pattern : String                the path it serves, such as "/api/todos/:id"
//! handle! : (Request) => Response ! {model}
//! ```
//!
//! where `Request` is `{method: String, path: String, params:
//! Dict[String], query: Dict[String], headers: Dict[String], body:
//! String}` and `Response` is `{status: Int, headers: Dict[String],
//! body: String}`. `params` holds the parts of the path that the
//! pattern's `:name` segments match.
//!
//! The route calls the demo's todo model directly, through host
//! functions that use the `model` capability: `query!`, `add!`,
//! `update!`, `remove!`, `toggle-all!` and `clear-completed!`. A model
//! `result` is a union of variants: `{:ok todo}` or `{:error why}`,
//! where `why` is `:not-found` or `:empty-title`, and a `result` with no
//! value is `:ok` itself.
//!
//! The glue exports `wasi:http/handler@0.3.0#handle`, lifted with the
//! async ABI's callback form. It reads the request, body and all, runs
//! `handle!`, returns the response with `task.return`, and then writes
//! the response's body. The model's functions are `async` in the WIT,
//! and the glue calls them with the synchronous ABI: each call blocks
//! the task until the model answers. Bodies are read and written with
//! async-lowered stream operations, and the glue waits on each one that
//! blocks with `waitable-set.wait`; their synchronous forms need a
//! feature beyond the async ABI. An `async` function's task may block,
//! so `handle!` runs as ordinary code, from start to finish, however
//! long the model takes.

use crate::codebase::{Codebase, Error};
use crate::codegen::{Builder, CodegenError, NIL, field_id};
use crate::element::{ABI_FUNCS, DEBUG_WIT, ELEMENT_WIT, WASI_WIT, walk_reachable};
use crate::term::{Hash, Term};
use crate::types::{Ty, TypeError};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The route world, as the demo defines it, without its WASI imports
/// other than `wasi:http`.
pub const ROUTE_WIT: &str = include_str!("../wit/route.wit");
/// The helpers every route's codebase starts with.
pub const PRELUDE: &str = include_str!("route.cdt");

/// The capability the model's functions use.
pub const MODEL: &str = "model";

#[derive(Debug)]
pub enum RouteError {
    Missing(&'static str),
    Contract { name: &'static str, error: TypeError },
    Pattern,
    Codegen(CodegenError),
    Encode(String),
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::Missing(name) => write!(f, "a route needs a definition named `{name}`"),
            RouteError::Contract { name, error } => write!(f, "`{name}` does not fit the route contract: {error}"),
            RouteError::Pattern => write!(f, "a route's `pattern` must be a string literal"),
            RouteError::Codegen(e) => write!(f, "{e}"),
            RouteError::Encode(e) => write!(f, "component encoding failed: {e}"),
        }
    }
}

impl std::error::Error for RouteError {}

impl From<CodegenError> for RouteError {
    fn from(e: CodegenError) -> Self {
        RouteError::Codegen(e)
    }
}

/// The global names of the attributes the contract and the model use,
/// as the prelude claims them.
mod key {
    pub const METHOD: &str = "conduct.http/method";
    pub const PATH: &str = "conduct.http/path";
    pub const PARAMS: &str = "conduct.http/params";
    pub const QUERY: &str = "conduct.http/query";
    pub const HEADERS: &str = "conduct.http/headers";
    pub const BODY: &str = "conduct.http/body";
    pub const STATUS: &str = "conduct.http/status";
    pub const ID: &str = "demo.todo/id";
    pub const TITLE: &str = "demo.todo/title";
    pub const COMPLETED: &str = "demo.todo/completed";
    pub const TODOS: &str = "demo.todo/todos";
    pub const COUNTS: &str = "demo.todo/counts";
    pub const ACTIVE_COUNT: &str = "demo.counts/active";
    pub const COMPLETED_COUNT: &str = "demo.counts/completed";
    pub const OK: &str = "conduct.result/ok";
    pub const ERROR: &str = "conduct.result/error";
}

fn field(name: &str, t: Ty) -> (String, Ty) {
    (name.to_string(), t)
}

/// A todo, as the model gives it.
pub fn todo_type() -> Ty {
    Ty::record([field(key::ID, Ty::str()), field(key::TITLE, Ty::str()), field(key::COMPLETED, Ty::bool())])
}

fn model_error() -> Ty {
    Ty::keys(["not-found".to_string(), "empty-title".to_string()])
}

/// What `add!` and `update!` answer: `{ok: todo}`, or `{error: why}`.
fn todo_result() -> Ty {
    Ty::tags([(key::OK.to_string(), Some(todo_type())), (key::ERROR.to_string(), Some(model_error()))])
}

/// The model's functions, as host functions: each name, the WIT
/// function it calls, its parameters, and its result.
fn model_functions() -> Vec<(&'static str, &'static str, Vec<Ty>, Ty)> {
    let filter = Ty::keys(["all".to_string(), "active".to_string(), "completed".to_string()]);
    let counts = Ty::record([field(key::ACTIVE_COUNT, Ty::int()), field(key::COMPLETED_COUNT, Ty::int())]);
    vec![
        (
            "query!",
            "query",
            vec![filter],
            Ty::record([field(key::TODOS, Ty::list(todo_type())), field(key::COUNTS, counts)]),
        ),
        ("add!", "add", vec![Ty::str()], todo_result()),
        // A WIT function with `option` parameters takes one record of its
        // named parameters, where an absent attribute is `none`.
        (
            "update!",
            "update",
            vec![Ty::record([
                field(key::ID, Ty::str()),
                field(key::TITLE, Ty::opt(Ty::str())),
                field(key::COMPLETED, Ty::opt(Ty::bool())),
            ])],
            todo_result(),
        ),
        (
            "remove!",
            "remove",
            vec![Ty::str()],
            Ty::tags([("ok".to_string(), None), (key::ERROR.to_string(), Some(model_error()))]),
        ),
        ("toggle-all!", "toggle-all", vec![Ty::bool()], Ty::record([])),
        ("clear-completed!", "clear-completed", vec![], Ty::record([])),
    ]
}

/// A codebase for a route: the model's functions and the prelude.
pub fn codebase() -> Result<Codebase, Error> {
    let mut cb = Codebase::new();
    for (name, _, params, ret) in model_functions() {
        cb.add_host(name, MODEL, params, ret)?;
    }
    cb.add_source(PRELUDE)?;
    Ok(cb)
}

/// The record `handle!` receives.
pub fn request_type() -> Ty {
    Ty::record([
        field(key::METHOD, Ty::str()),
        field(key::PATH, Ty::str()),
        field(key::PARAMS, Ty::dict(Ty::str())),
        field(key::QUERY, Ty::dict(Ty::str())),
        field(key::HEADERS, Ty::dict(Ty::str())),
        field(key::BODY, Ty::str()),
    ])
}

/// The record `handle!` answers.
pub fn response_type() -> Ty {
    Ty::record([field(key::STATUS, Ty::int()), field(key::HEADERS, Ty::dict(Ty::str())), field(key::BODY, Ty::str())])
}

/// The route's definitions.
pub struct RouteDefs {
    pub handle: Hash,
    pub pattern: Option<String>,
}

/// Find the route's definitions and check them against the contract.
pub fn check(cb: &Codebase) -> Result<RouteDefs, RouteError> {
    let handle = *cb.names.get("handle!").ok_or(RouteError::Missing("handle!"))?;
    let s = &cb.solver;
    let ty = s.instantiate(&cb.defs[&handle].scheme, 1);
    let expected = Ty::func_eff(vec![request_type()], response_type(), Ty::caps([MODEL.to_string()]), vec![]);
    s.constrain(&ty, &expected).map_err(|error| RouteError::Contract { name: "handle!", error })?;
    let pattern = match cb.names.get("pattern") {
        None => None,
        Some(h) => match &cb.defs[h].term {
            Term::Str(p) => Some(p.clone()),
            _ => return Err(RouteError::Pattern),
        },
    };
    Ok(RouteDefs { handle, pattern })
}

/// The host functions `root` reaches, by name.
fn hosts(cb: &Codebase, root: Hash) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk_reachable(cb, root, &mut |t| {
        if let Term::Host(name, _) = t {
            out.insert(name.clone());
        }
    });
    out
}

/// A function that builds a record from its fields, by global name,
/// given as parameters in `fields`' order, each named after its global
/// name's last part. Each argument is consumed.
fn record_func(func: &str, fields: &[&str]) -> String {
    let local = |f: &str| f.rsplit('/').next().unwrap().replace('-', "_");
    let params: String = fields.iter().map(|f| format!(" (param ${} i64)", local(f))).collect();
    let mut sorted: Vec<(i64, String)> = fields.iter().map(|f| (field_id(f), local(f))).collect();
    sorted.sort_by_key(|f| f.0);
    let mut stores = String::new();
    for (i, (id, name)) in sorted.iter().enumerate() {
        let _ = write!(
            stores,
            "\n    (i64.store offset={} (local.get $w) (i64.const {id})) (i64.store offset={} (local.get $w) (local.get ${}))",
            8 + 16 * i,
            16 + 16 * i,
            name
        );
    }
    format!(
        "  (func {func}{params} (result i64)\n    (local $w i32)\n    (local.set $w (call $obj (i32.const 3) (i32.const {n}) (i32.const {bytes}))){stores}\n    (i64.extend_i32_u (local.get $w)))\n",
        n = fields.len(),
        bytes = 16 * fields.len(),
    )
}

/// The glue for the model's functions a route calls: each lowers its
/// arguments, calls the model synchronously, and lifts the result. Each
/// borrows its arguments and answers an owned value.
fn model_glue(b: &mut Builder, used: &BTreeSet<String>) -> String {
    let all = b.static_keyword("all");
    let active = b.static_keyword("active");
    let not_found = b.static_keyword("not-found");
    let empty_title = b.static_keyword("empty-title");
    let ok = b.static_keyword("ok");
    let nothing = b.static_empty_record();
    let model = "demo:todo/model";
    let mut out = String::new();
    out.push_str(&record_func("$todo_new", &[key::ID, key::TITLE, key::COMPLETED]));
    out.push_str(&record_func("$counts_new", &[key::ACTIVE_COUNT, key::COMPLETED_COUNT]));
    out.push_str(&record_func("$query_new", &[key::TODOS, key::COUNTS]));
    out.push_str(&record_func("$ok_new", &[key::OK]));
    out.push_str(&record_func("$error_new", &[key::ERROR]));
    let _ = write!(
        out,
        r#"  (func $int_of (param $n i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $n)) (i64.const 1)) (i64.const 1)))
  (func $todo_at (param $at i32) (result i64)
    ;; Lift the todo at `at`, taking its strings.
    (call $todo_new
      (call $arg_str (i32.load (local.get $at)) (i32.load offset=4 (local.get $at)))
      (call $arg_str (i32.load offset=8 (local.get $at)) (i32.load offset=12 (local.get $at)))
      (call $bool (i32.load8_u offset=16 (local.get $at)))))
  (func $error_at (param $at i32) (result i64)
    (select (i64.const {not_found}) (i64.const {empty_title}) (i32.eqz (i32.load8_u (local.get $at)))))
  (func $todo_result (param $at i32) (result i64)
    ;; Lift a `result<todo, model-error>` at `at`.
    (if (result i64) (i32.load8_u (local.get $at))
      (then (call $error_new (call $error_at (i32.add (local.get $at) (i32.const 4)))))
      (else (call $ok_new (call $todo_at (i32.add (local.get $at) (i32.const 4)))))))
  (func $field_at (param $r i64) (param $id i64) (result i64)
    ;; The record's attribute `id`, borrowed, or 2 where it has none.
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $p (call $ptr (local.get $r)))
    (local.set $n (call $len (local.get $r)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (if (i64.eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
        (then (return (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i64.const 2))
  (func $ret_area (result i32)
    ;; Where a model call writes its result: freed by the caller.
    (local $p i32)
    (local.set $p (call $alloc (i32.const 24)))
    (memory.fill (local.get $p) (i32.const 0) (i32.const 24))
    (local.get $p))
"#
    );
    for name in used {
        let Some((_, wit, params, _)) = model_functions().into_iter().find(|(n, ..)| n == name) else { continue };
        let func = format!("$model_{}", wit.replace('-', "_"));
        b.provide_host(name, &func);
        let (import, body) = match wit {
            "query" => (
                "(param i32 i32)",
                format!(
                    r#"    (local $ret i32) (local $arr i32) (local $n i32) (local $l i32) (local $i i32) (local $v i64)
    (local.set $ret (call $ret_area))
    (call $m_query
      (if (result i32) (call $eq (local.get $a0) (i64.const {all}))
        (then (i32.const 0))
        (else (select (i32.const 1) (i32.const 2) (call $eq (local.get $a0) (i64.const {active})))))
      (local.get $ret))
    (local.set $arr (i32.load (local.get $ret)))
    (local.set $n (i32.load offset=4 (local.get $ret)))
    (local.set $l (call $list_new (local.get $n)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (call $list_set (local.get $l) (local.get $i)
        (call $todo_at (i32.add (local.get $arr) (i32.mul (local.get $i) (i32.const 20)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (local.get $n) (then (call $free (local.get $arr))))
    (local.set $v (call $query_new (i64.extend_i32_u (local.get $l))
      (call $counts_new (call $int_of (i32.load offset=8 (local.get $ret))) (call $int_of (i32.load offset=12 (local.get $ret))))))
    (call $free (local.get $ret))
    (local.get $v)"#
                ),
            ),
            "add" => (
                "(param i32 i32 i32)",
                r#"    (local $ret i32) (local $v i64)
    (local.set $ret (call $ret_area))
    (call $m_add (call $str_data (local.get $a0)) (call $len (local.get $a0)) (local.get $ret))
    (local.set $v (call $todo_result (local.get $ret)))
    (call $free (local.get $ret))
    (local.get $v)"#
                    .to_string(),
            ),
            "update" => (
                "(param i32 i32 i32 i32 i32 i32 i32 i32)",
                format!(
                    r#"    (local $ret i32) (local $v i64) (local $id i64) (local $title i64) (local $done i64)
    (local.set $id (call $field_at (local.get $a0) (i64.const {id})))
    (local.set $title (call $field_at (local.get $a0) (i64.const {title})))
    (local.set $done (call $field_at (local.get $a0) (i64.const {completed})))
    (local.set $ret (call $ret_area))
    (call $m_update
      (call $str_data (local.get $id)) (call $len (local.get $id))
      (i64.ne (local.get $title) (i64.const {ABSENT}))
      (if (result i32) (i64.ne (local.get $title) (i64.const {ABSENT})) (then (call $str_data (local.get $title))) (else (i32.const 0)))
      (if (result i32) (i64.ne (local.get $title) (i64.const {ABSENT})) (then (call $len (local.get $title))) (else (i32.const 0)))
      (i64.ne (local.get $done) (i64.const {ABSENT}))
      (i64.eq (local.get $done) (i64.const {TRUE}))
      (local.get $ret))
    (local.set $v (call $todo_result (local.get $ret)))
    (call $free (local.get $ret))
    (local.get $v)"#,
                    TRUE = crate::codegen::TRUE,
                    ABSENT = NIL,
                    id = field_id(key::ID),
                    title = field_id(key::TITLE),
                    completed = field_id(key::COMPLETED),
                ),
            ),
            "remove" => (
                "(param i32 i32 i32)",
                format!(
                    r#"    (local $ret i32) (local $v i64)
    (local.set $ret (call $ret_area))
    (call $m_remove (call $str_data (local.get $a0)) (call $len (local.get $a0)) (local.get $ret))
    (local.set $v
      (if (result i64) (i32.load8_u (local.get $ret))
        (then (call $error_new (call $error_at (i32.add (local.get $ret) (i32.const 1)))))
        (else (i64.const {ok}))))
    (call $free (local.get $ret))
    (local.get $v)"#
                ),
            ),
            "toggle-all" => (
                "(param i32)",
                format!(
                    "    (call $m_toggle_all (i64.eq (local.get $a0) (i64.const {}))) (i64.const {nothing})",
                    crate::codegen::TRUE
                ),
            ),
            _ => ("", format!("    (call $m_clear_completed) (i64.const {nothing})")),
        };
        b.add_import(format!("(import \"{model}\" \"{wit}\" (func $m_{} {import}))", wit.replace('-', "_")));
        let params: String = (0..params.len()).map(|i| format!(" (param $a{i} i64)")).collect();
        let _ = writeln!(out, "  (func {func}{params} (result i64)\n{body})");
    }
    out
}

/// The core imports of the handler glue.
const HTTP_IMPORTS: &str = r#"(import "wasi:http/types@0.3.0" "[method]request.get-method" (func $req_method (param i32 i32)))
(import "wasi:http/types@0.3.0" "[method]request.get-path-with-query" (func $req_target (param i32 i32)))
(import "wasi:http/types@0.3.0" "[method]request.get-headers" (func $req_headers (param i32) (result i32)))
(import "wasi:http/types@0.3.0" "[method]fields.copy-all" (func $fields_copy_all (param i32 i32)))
(import "wasi:http/types@0.3.0" "[resource-drop]fields" (func $fields_drop (param i32)))
(import "wasi:http/types@0.3.0" "[static]request.consume-body" (func $req_consume (param i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[future-new-0][static]request.consume-body" (func $received_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][future-write-0][static]request.consume-body" (func $received_write (param i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[future-drop-writable-0][static]request.consume-body" (func $received_drop (param i32)))
(import "wasi:http/types@0.3.0" "[async-lower][stream-read-1][static]request.consume-body" (func $in_read (param i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[stream-drop-readable-1][static]request.consume-body" (func $in_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-drop-readable-2][static]request.consume-body" (func $in_trailers_drop (param i32)))
(import "wasi:http/types@0.3.0" "[constructor]fields" (func $fields_new (result i32)))
(import "wasi:http/types@0.3.0" "[method]fields.append" (func $fields_append (param i32 i32 i32 i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[static]response.new" (func $resp_new (param i32 i32 i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[method]response.set-status-code" (func $resp_status (param i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[stream-new-0][static]response.new" (func $out_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][stream-write-0][static]response.new" (func $out_write (param i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[stream-drop-writable-0][static]response.new" (func $out_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-new-1][static]response.new" (func $out_trailers_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][future-write-1][static]response.new" (func $out_trailers_write (param i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[future-drop-writable-1][static]response.new" (func $out_trailers_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-drop-readable-2][static]response.new" (func $sent_drop (param i32)))
(import "$root" "[waitable-set-new]" (func $set_new (result i32)))
(import "$root" "[waitable-set-drop]" (func $set_drop (param i32)))
(import "$root" "[waitable-join]" (func $join (param i32 i32)))
(import "$root" "[waitable-set-wait]" (func $wait (param i32 i32) (result i32)))
(import "[export]wasi:http/handler@0.3.0" "[task-return]handle" (func $handle_return (param i32 i32 i32 i64 i32 i32 i32 i32)))"#;

/// The core module of a route, as WebAssembly text.
pub fn core_wat(cb: &Codebase) -> Result<String, RouteError> {
    let defs = check(cb)?;
    let mut b = Builder::new(cb);
    let used = hosts(cb, defs.handle);
    let model = model_glue(&mut b, &used);
    b.add_func(model);
    b.add_func(ABI_FUNCS);
    for line in HTTP_IMPORTS.lines() {
        b.add_import(line);
    }

    let methods =
        ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH"].map(|m| b.static_str(m));
    let mut method_cases = String::new();
    for (i, m) in methods.iter().enumerate() {
        let _ = writeln!(
            method_cases,
            "    (if (i32.eq (local.get $case) (i32.const {i})) (then (return (i64.const {m}))))"
        );
    }
    let slash = b.static_str("/");
    let zeros = b.static_str(&"\0".repeat(64)) + 8;
    let route_header = b.static_str("x-demo-route");

    // The parameters the pattern's `:name` segments match, when the
    // path has the pattern's shape.
    let mut params = String::from("    (local.set $params (i64.extend_i32_u (call $dict_new (i32.const 0))))\n");
    let mut route_value = None;
    if let Some(pattern) = &defs.pattern {
        let segments: Vec<&str> = pattern.split('/').collect();
        let mut test = format!("(i32.eq (call $len (local.get $segs)) (i32.const {}))", segments.len());
        let mut fill = String::new();
        for (i, seg) in segments.iter().enumerate() {
            match seg.strip_prefix(':') {
                Some(name) => {
                    let key = b.static_str(name);
                    let _ = write!(
                        fill,
                        "\n        (call $assoc_into (local.get $params) (i64.const {key}) (call $decode_str (call $list_get (local.get $segs) (i32.const {i})) (i32.const 0)))"
                    );
                    let _ = write!(fill, "\n        (local.set $params (global.get $assoc_out))");
                }
                None => {
                    let lit = b.static_str(seg);
                    test = format!(
                        "(i32.and {test} (call $str_eq (call $list_get (local.get $segs) (i32.const {i})) (i64.const {lit})))"
                    );
                }
            }
        }
        // `$str_eq` reads past a short list, so test the length first.
        let _ = write!(
            params,
            "    (local.set $segs (call $str_split (local.get $path) (i64.const {slash})))\n    (if (i32.eq (call $len (local.get $segs)) (i32.const {n}))\n      (then (if {test}\n        (then{fill}))))\n    (call $drop (local.get $segs))\n",
            n = segments.len(),
        );
        route_value = Some(b.static_str(pattern));
    }
    let route_header_append = match route_value {
        Some(v) => format!(
            "    (drop (call $fields_append_str (local.get $h) (i64.const {route_header}) (i64.const {v}) (local.get $ret)))\n"
        ),
        None => String::new(),
    };

    let handle = b.call(defs.handle, &["(local.get $request)".into()])?;
    let mut x = String::new();
    x.push_str(&record_func(
        "$request_new",
        &[key::METHOD, key::PATH, key::PARAMS, key::QUERY, key::HEADERS, key::BODY],
    ));
    let _ = write!(
        x,
        r#"  (global $assoc_out (mut i64) (i64.const 0))
  (func $assoc_into (param $d i64) (param $k i64) (param $v i64)
    ;; Set `k` to `v` in the dictionary `d`, consuming `d` and `v`; the
    ;; new dictionary is in `$assoc_out`.
    (global.set $assoc_out (call $dict_assoc (local.get $d) (local.get $k) (local.get $v)))
    (call $drop (local.get $d))
    (call $drop (local.get $v)))
  (func $hex (param $c i32) (result i32)
    (if (i32.and (i32.ge_u (local.get $c) (i32.const 48)) (i32.le_u (local.get $c) (i32.const 57)))
      (then (return (i32.sub (local.get $c) (i32.const 48)))))
    (local.set $c (i32.or (local.get $c) (i32.const 32)))
    (if (i32.and (i32.ge_u (local.get $c) (i32.const 97)) (i32.le_u (local.get $c) (i32.const 102)))
      (then (return (i32.sub (local.get $c) (i32.const 87)))))
    (i32.const -1))
  (func $decode (param $p i32) (param $n i32) (param $plus i32) (result i64)
    ;; The bytes at `p`, percent-decoded, and with `+` as a space if
    ;; `plus`, as a new string.
    (local $buf i32) (local $i i32) (local $o i32) (local $c i32) (local $h i32) (local $l i32) (local $s i64)
    (local.set $buf (call $alloc (i32.add (local.get $n) (i32.const 1))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $c (i32.load8_u (i32.add (local.get $p) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (if (i32.and (i32.eq (local.get $c) (i32.const 37)) (i32.le_u (i32.add (local.get $i) (i32.const 2)) (local.get $n)))
        (then
          (local.set $h (call $hex (i32.load8_u (i32.add (local.get $p) (local.get $i)))))
          (local.set $l (call $hex (i32.load8_u offset=1 (i32.add (local.get $p) (local.get $i)))))
          (if (i32.and (i32.ge_s (local.get $h) (i32.const 0)) (i32.ge_s (local.get $l) (i32.const 0)))
            (then
              (local.set $c (i32.or (i32.shl (local.get $h) (i32.const 4)) (local.get $l)))
              (local.set $i (i32.add (local.get $i) (i32.const 2)))))))
      (if (i32.and (local.get $plus) (i32.eq (local.get $c) (i32.const 43)))
        (then (local.set $c (i32.const 32))))
      (i32.store8 (i32.add (local.get $buf) (local.get $o)) (local.get $c))
      (local.set $o (i32.add (local.get $o) (i32.const 1)))
      (br $next)))
    (local.set $s (call $str_new (local.get $buf) (local.get $o)))
    (call $free (local.get $buf))
    (local.get $s))
  (func $decode_str (param $s i64) (param $plus i32) (result i64)
    (call $decode (call $str_data (local.get $s)) (call $len (local.get $s)) (local.get $plus)))
  (func $find (param $p i32) (param $n i32) (param $c i32) (result i32)
    ;; Where the byte `c` first is in the `n` bytes at `p`, or `n`.
    (local $i i32)
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (br_if $done (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (local.get $c)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (local.get $i))
  (func $parse_query (param $p i32) (param $n i32) (result i64)
    ;; `a=1&b=2` as a dictionary, its names and values form-decoded.
    (local $d i64) (local $k i64) (local $end i32) (local $eq i32)
    (local.set $d (i64.extend_i32_u (call $dict_new (i32.const 0))))
    (block $done (loop $next
      (br_if $done (i32.eqz (local.get $n)))
      (local.set $end (call $find (local.get $p) (local.get $n) (i32.const 38)))
      (if (local.get $end)
        (then
          (local.set $eq (call $find (local.get $p) (local.get $end) (i32.const 61)))
          (local.set $k (call $decode (local.get $p) (local.get $eq) (i32.const 1)))
          (call $assoc_into (local.get $d) (local.get $k)
            (if (result i64) (i32.lt_u (local.get $eq) (local.get $end))
              (then (call $decode (i32.add (local.get $p) (i32.add (local.get $eq) (i32.const 1)))
                (i32.sub (local.get $end) (i32.add (local.get $eq) (i32.const 1))) (i32.const 1)))
              (else (call $str_new (local.get $p) (i32.const 0)))))
          (call $drop (local.get $k))
          (local.set $d (global.get $assoc_out))))
      (br_if $done (i32.ge_u (local.get $end) (local.get $n)))
      (local.set $p (i32.add (local.get $p) (i32.add (local.get $end) (i32.const 1))))
      (local.set $n (i32.sub (local.get $n) (i32.add (local.get $end) (i32.const 1))))
      (br $next)))
    (local.get $d))
  (func $method_name (param $at i32) (result i64)
    ;; The `method` at `at`, as its name.
    (local $case i32)
    (local.set $case (i32.load8_u (local.get $at)))
{method_cases}    (call $arg_str (i32.load offset=4 (local.get $at)) (i32.load offset=8 (local.get $at))))
  (func $read_headers (param $req i32) (param $ret i32) (result i64)
    (local $f i32) (local $arr i32) (local $n i32) (local $i i32) (local $e i32) (local $d i64) (local $k i64)
    (local.set $f (call $req_headers (local.get $req)))
    (call $fields_copy_all (local.get $f) (local.get $ret))
    (call $fields_drop (local.get $f))
    (local.set $arr (i32.load (local.get $ret)))
    (local.set $n (i32.load offset=4 (local.get $ret)))
    (local.set $d (i64.extend_i32_u (call $dict_new (i32.const 0))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $e (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 4))))
      (local.set $k (call $arg_str (i32.load (local.get $e)) (i32.load offset=4 (local.get $e))))
      (call $assoc_into (local.get $d) (local.get $k)
        (call $arg_str (i32.load offset=8 (local.get $e)) (i32.load offset=12 (local.get $e))))
      (call $drop (local.get $k))
      (local.set $d (global.get $assoc_out))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (if (local.get $n) (then (call $free (local.get $arr))))
    (local.get $d))
  (func $await (param $w i32) (param $code i32) (result i32)
    ;; What an async stream or future operation on `w` that answered
    ;; `code` did, waiting for it if it blocked. The task waits alone, on
    ;; a set of its own, so a task serving another request never takes
    ;; its event.
    (local $set i32) (local $at i32)
    (if (i32.ne (local.get $code) (i32.const -1)) (then (return (local.get $code))))
    (local.set $set (call $set_new))
    (call $join (local.get $w) (local.get $set))
    (local.set $at (call $alloc (i32.const 8)))
    (drop (call $wait (local.get $set) (local.get $at)))
    (local.set $code (i32.load offset=4 (local.get $at)))
    (call $free (local.get $at))
    (call $join (local.get $w) (i32.const 0))
    (call $set_drop (local.get $set))
    (local.get $code))
  (func $read_all (param $s i32) (result i64)
    ;; Read the stream `s` to its end, and drop it.
    (local $buf i32) (local $cap i32) (local $n i32) (local $code i32) (local $grown i32) (local $out i64)
    (local.set $cap (i32.const 1024))
    (local.set $buf (call $alloc (local.get $cap)))
    (block $done (loop $next
      (if (i32.eq (local.get $n) (local.get $cap))
        (then
          (local.set $grown (call $alloc (i32.shl (local.get $cap) (i32.const 1))))
          (memory.copy (local.get $grown) (local.get $buf) (local.get $n))
          (call $free (local.get $buf))
          (local.set $buf (local.get $grown))
          (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))))
      (local.set $code (call $await (local.get $s)
        (call $in_read (local.get $s) (i32.add (local.get $buf) (local.get $n)) (i32.sub (local.get $cap) (local.get $n)))))
      (local.set $n (i32.add (local.get $n) (i32.shr_u (local.get $code) (i32.const 4))))
      (br_if $next (i32.eqz (i32.and (local.get $code) (i32.const 15))))))
    (call $in_drop (local.get $s))
    (local.set $out (call $str_new (local.get $buf) (local.get $n)))
    (call $free (local.get $buf))
    (local.get $out))
  (func $fields_append_str (param $h i32) (param $k i64) (param $v i64) (param $ret i32) (result i32)
    (call $fields_append (local.get $h)
      (call $str_data (local.get $k)) (call $len (local.get $k))
      (call $str_data (local.get $v)) (call $len (local.get $v))
      (local.get $ret))
    (i32.load8_u (local.get $ret)))
  (func $respond (param $r i64)
    ;; Answer with the response `r`, which this consumes: return it, then
    ;; write its body.
    (local $status i64) (local $headers i64) (local $body i64) (local $ret i32) (local $h i32)
    (local $i i32) (local $n i32) (local $pair i64) (local $trailers i64) (local $resp i32)
    (local $w i32) (local $off i32) (local $code i32)
    (local.set $ret (call $ret_area))
    (local.set $status (call $rec_get (local.get $r) (i64.const {status_id})))
    (local.set $headers (call $rec_get (local.get $r) (i64.const {headers_id})))
    (local.set $body (call $rec_get (local.get $r) (i64.const {body_id})))
    (call $drop (local.get $r))
    (local.set $h (call $fields_new))
    (local.set $n (call $len (local.get $headers)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (drop (call $fields_append_str (local.get $h)
        (call $dict_key (local.get $headers) (local.get $i)) (call $dict_val (local.get $headers) (local.get $i))
        (local.get $ret)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
{route_header_append}    (local.set $pair (call $out_new))
    (local.set $trailers (call $out_trailers_new))
    (call $resp_new (local.get $h) (i32.const 1) (i32.wrap_i64 (local.get $pair)) (i32.wrap_i64 (local.get $trailers))
      (local.get $ret))
    (local.set $resp (i32.load (local.get $ret)))
    (call $sent_drop (i32.load offset=4 (local.get $ret)))
    (drop (call $resp_status (local.get $resp) (i32.wrap_i64 (i64.shr_s (local.get $status) (i64.const 1)))))
    (call $handle_return (i32.const 0) (local.get $resp) (i32.const 0) (i64.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0))
    ;; The response is the host's now: write its body as the host reads.
    (local.set $w (i32.wrap_i64 (i64.shr_u (local.get $pair) (i64.const 32))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $off) (call $len (local.get $body))))
      (local.set $code (call $await (local.get $w) (call $out_write (local.get $w)
        (i32.add (call $str_data (local.get $body)) (local.get $off))
        (i32.sub (call $len (local.get $body)) (local.get $off)))))
      (local.set $off (i32.add (local.get $off) (i32.shr_u (local.get $code) (i32.const 4))))
      ;; The host dropped the stream: nothing more goes.
      (br_if $done (i32.and (local.get $code) (i32.const 15)))
      (br $next)))
    (call $out_drop (local.get $w))
    (local.set $w (i32.wrap_i64 (i64.shr_u (local.get $trailers) (i64.const 32))))
    (drop (call $await (local.get $w) (call $out_trailers_write (local.get $w) (i32.const {zeros}))))
    (call $out_trailers_drop (local.get $w))
    (call $drop (local.get $headers))
    (call $drop (local.get $body))
    (call $free (local.get $ret)))
  (func (export "[async-lift]wasi:http/handler@0.3.0#handle") (param $req i32) (result i32)
    (local $ret i32) (local $method i64) (local $target i64) (local $path i64) (local $query i64)
    (local $params i64) (local $segs i64) (local $headers i64) (local $body i64) (local $received i64)
    (local $request i64) (local $q i32)
    (local.set $ret (call $ret_area))
    (call $req_method (local.get $req) (local.get $ret))
    (local.set $method (call $method_name (local.get $ret)))
    (call $req_target (local.get $req) (local.get $ret))
    (local.set $target
      (if (result i64) (i32.load8_u (local.get $ret))
        (then (call $arg_str (i32.load offset=4 (local.get $ret)) (i32.load offset=8 (local.get $ret))))
        (else (i64.const {slash}))))
    (local.set $headers (call $read_headers (local.get $req) (local.get $ret)))
    ;; The body: read it all, then tell the host it was received.
    (local.set $received (call $received_new))
    (call $req_consume (local.get $req) (i32.wrap_i64 (local.get $received)) (local.get $ret))
    (call $in_trailers_drop (i32.load offset=4 (local.get $ret)))
    (local.set $body (call $read_all (i32.load (local.get $ret))))
    (local.set $q (i32.wrap_i64 (i64.shr_u (local.get $received) (i64.const 32))))
    (drop (call $await (local.get $q) (call $received_write (local.get $q) (i32.const {zeros}))))
    (call $received_drop (local.get $q))
    ;; The path, and the query after its `?`.
    (local.set $q (call $find (call $str_data (local.get $target)) (call $len (local.get $target)) (i32.const 63)))
    (local.set $path (call $str_new (call $str_data (local.get $target)) (local.get $q)))
    (local.set $query
      (if (result i64) (i32.lt_u (local.get $q) (call $len (local.get $target)))
        (then (call $parse_query (i32.add (call $str_data (local.get $target)) (i32.add (local.get $q) (i32.const 1)))
          (i32.sub (call $len (local.get $target)) (i32.add (local.get $q) (i32.const 1)))))
        (else (i64.extend_i32_u (call $dict_new (i32.const 0))))))
    (call $drop (local.get $target))
{params}    (call $free (local.get $ret))
    (local.set $request (call $request_new (local.get $method) (local.get $path) (local.get $params)
      (local.get $query) (local.get $headers) (local.get $body)))
    (call $respond {handle})
    (i32.const 0))
  (func (export "[callback][async-lift]wasi:http/handler@0.3.0#handle") (param i32 i32 i32) (result i32)
    ;; The task never waits, so nothing calls back.
    unreachable)
  (func (export "conduct_live") (export "conduct:debug/memory#live") (result i32) (global.get $live))
"#,
        status_id = field_id(key::STATUS),
        headers_id = field_id(key::HEADERS),
        body_id = field_id(key::BODY),
    );
    b.add_func(x);
    Ok(b.finish()?)
}

/// Compile the route in `cb` to a component of the `route-component`
/// world.
pub fn component(cb: &Codebase) -> Result<Vec<u8>, RouteError> {
    encode(cb, false)
}

/// The route as [`component`] compiles it, plus an export
/// `conduct:debug/memory#live` that counts the blocks its memory holds,
/// for tests.
pub fn debug_component(cb: &Codebase) -> Result<Vec<u8>, RouteError> {
    encode(cb, true)
}

fn encode(cb: &Codebase, debug: bool) -> Result<Vec<u8>, RouteError> {
    let wat = core_wat(cb)?;
    let fail = |e: anyhow::Error| RouteError::Encode(format!("{e:#}"));
    let mut core = wat::parse_str(&wat).map_err(|e| RouteError::Encode(format!("{e}")))?;
    let mut resolve = wit_parser::Resolve::default();
    for (name, wit) in WASI_WIT {
        resolve.push_str(name, wit).map_err(fail)?;
    }
    let mut pkg = resolve.push_str("route.wit", ROUTE_WIT).map_err(fail)?;
    if debug {
        resolve.push_str("element.wit", ELEMENT_WIT).map_err(fail)?;
        pkg = resolve.push_str("debug.wit", DEBUG_WIT).map_err(fail)?;
    }
    let world = resolve.select_world(&[pkg], Some("route-component")).map_err(fail)?;
    wit_component::embed_component_metadata(&mut core, &resolve, world, wit_component::StringEncoding::UTF8, false)
        .map_err(fail)?;
    wit_component::ComponentEncoder::default().validate(true).module(&core).and_then(|e| e.encode()).map_err(fail)
}
