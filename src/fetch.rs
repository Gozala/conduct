//! The glue that performs an element's `fetch` effects through
//! `wasi:http/client@0.3.0`, as WebAssembly text.
//!
//! An element's exports that run `update` are lifted with the async
//! ABI's callback form, so a request does not block its instance: the
//! export starts each request and returns, and the callback carries the
//! requests on as their events arrive. A request goes through these
//! steps, each of which may finish at once or later:
//!
//! 1. Build the request: headers, a `stream<u8>` for a body, a
//!    `future` for trailers, the method, and the URL. An absolute URL
//!    sets the scheme and authority; any other is a path for the host to
//!    resolve, as the demo's page resolves it against its origin.
//! 2. Write the body and the trailers (none), and call `send`, all
//!    async-lowered, since the host reads the body while it sends.
//! 3. When `send` returns a response, consume its body and read the
//!    stream to its end.
//! 4. Give `update` the body and the status under the handler the
//!    effect named. That may start more requests, which join the same
//!    task.
//!
//! The task returns when none of its requests has anything left to wait
//! for. So the host sees `handle-event` finish after every response it
//! caused has reached `update`, as the Zena element finishes after its
//! `await`s, and renders once.
//!
//! What the glue keeps for a request is a raw block of [`REQ_SIZE`]
//! bytes on a list of requests in flight. A waitable that wakes the
//! callback is found by its handle on that list.

/// The bytes a request in flight keeps.
pub const REQ_SIZE: u32 = 112;

/// The core imports the fetch glue needs.
pub const IMPORTS: &str = r#"(import "wasi:http/types@0.3.0" "[constructor]fields" (func $fields_new (result i32)))
(import "wasi:http/types@0.3.0" "[method]fields.append" (func $fields_append (param i32 i32 i32 i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[static]request.new" (func $request_new (param i32 i32 i32 i32 i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[method]request.set-method" (func $set_method (param i32 i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[method]request.set-scheme" (func $set_scheme (param i32 i32 i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[method]request.set-authority" (func $set_authority (param i32 i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[method]request.set-path-with-query" (func $set_path (param i32 i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[method]response.get-status-code" (func $status_code (param i32) (result i32)))
(import "wasi:http/types@0.3.0" "[static]response.consume-body" (func $consume_body (param i32 i32 i32)))
(import "wasi:http/types@0.3.0" "[stream-new-0][static]request.new" (func $body_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][stream-write-0][static]request.new" (func $body_write (param i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[stream-drop-writable-0][static]request.new" (func $body_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-new-1][static]request.new" (func $trailers_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][future-write-1][static]request.new" (func $trailers_write (param i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[future-drop-writable-1][static]request.new" (func $trailers_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-drop-readable-2][static]request.new" (func $sent_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-new-0][static]response.consume-body" (func $done_new (result i64)))
(import "wasi:http/types@0.3.0" "[async-lower][future-write-0][static]response.consume-body" (func $done_write (param i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[future-drop-writable-0][static]response.consume-body" (func $done_drop (param i32)))
(import "wasi:http/types@0.3.0" "[async-lower][stream-read-1][static]response.consume-body" (func $read (param i32 i32 i32) (result i32)))
(import "wasi:http/types@0.3.0" "[stream-drop-readable-1][static]response.consume-body" (func $read_drop (param i32)))
(import "wasi:http/types@0.3.0" "[future-drop-readable-2][static]response.consume-body" (func $rtrailers_drop (param i32)))
(import "wasi:http/client@0.3.0" "[async-lower]send" (func $send (param i32 i32) (result i32)))
(import "$root" "[waitable-set-new]" (func $set_new (result i32)))
(import "$root" "[waitable-set-drop]" (func $set_drop (param i32)))
(import "$root" "[waitable-join]" (func $join (param i32 i32)))
(import "$root" "[subtask-drop]" (func $subtask_drop (param i32)))"#;

/// Static strings the glue reads, by their addresses.
pub struct Statics {
    /// 64 zero bytes: `ok(none)` for the trailers and `ok` for the
    /// response's result, as the canonical ABI lays both out.
    pub zeros: u32,
    pub content_type: u32,
    pub json: u32,
    pub response: u32,
    pub http: u32,
    pub https: u32,
    pub slash: u32,
    /// The methods with a case of their own in `wasi:http`'s `method`,
    /// in its order.
    pub methods: [u32; 9],
}

/// The functions of the fetch glue. `$fetch_start` performs one effect,
/// `$fetch_event` takes one event of the callback, and `$fetch_pending`
/// says whether a task still waits on a request.
pub fn funcs(s: &Statics) -> String {
    let Statics { zeros, content_type, json, response, http, https, slash, methods } = s;
    let mut method_cases = String::new();
    for (i, m) in methods.iter().enumerate() {
        method_cases.push_str(&format!(
            "    (if (call $str_eq (local.get $m) (i64.const {m})) (then (return (i32.const {i}))))\n"
        ));
    }
    format!(
        r#"  (global $reqs (mut i32) (i32.const 0))
  (func $method_case (param $m i64) (result i32)
{method_cases}    (i32.const 9))
  (func $prefix_len (param $s i64) (param $p i64) (result i32)
    ;; The length of `p` if `s` starts with it, or 0.
    (if (i32.lt_u (call $len (local.get $s)) (call $len (local.get $p))) (then (return (i32.const 0))))
    (if (result i32) (call $bytes_eq (call $str_data (local.get $s)) (call $str_data (local.get $p)) (call $len (local.get $p)))
      (then (call $len (local.get $p)))
      (else (i32.const 0))))
  (func $set_url (param $req i32) (param $url i64)
    ;; An absolute URL sets the scheme and authority; the rest of it, or
    ;; a URL without a scheme, is the path with its query.
    (local $data i32) (local $n i32) (local $at i32) (local $end i32) (local $scheme i32)
    (local.set $data (call $str_data (local.get $url)))
    (local.set $n (call $len (local.get $url)))
    (local.set $at (call $prefix_len (local.get $url) (i64.const {http})))
    (if (local.get $at)
      (then (local.set $scheme (i32.const 0)))
      (else
        (local.set $at (call $prefix_len (local.get $url) (i64.const {https})))
        (local.set $scheme (i32.const 1))))
    (if (local.get $at)
      (then
        (drop (call $set_scheme (local.get $req) (i32.const 1) (local.get $scheme) (i32.const 0) (i32.const 0)))
        (local.set $end (local.get $at))
        (block $found (loop $next
          (br_if $found (i32.ge_u (local.get $end) (local.get $n)))
          (br_if $found (i32.eq (i32.load8_u (i32.add (local.get $data) (local.get $end))) (i32.const 47)))
          (local.set $end (i32.add (local.get $end) (i32.const 1)))
          (br $next)))
        (drop (call $set_authority (local.get $req) (i32.const 1)
          (i32.add (local.get $data) (local.get $at)) (i32.sub (local.get $end) (local.get $at))))
        (local.set $at (local.get $end))))
    (if (i32.lt_u (local.get $at) (local.get $n))
      (then
        (drop (call $set_path (local.get $req) (i32.const 1)
          (i32.add (local.get $data) (local.get $at)) (i32.sub (local.get $n) (local.get $at)))))
      (else
        ;; An absolute URL with no path asks for the root.
        (drop (call $set_path (local.get $req) (i32.const 1) (call $str_data (i64.const {slash})) (i32.const 1))))))
  (func $fetch_start (param $id i32) (param $e i32) (param $set i32)
    ;; Start the request a `fetch` effect describes, in the task whose
    ;; waitable set is `set`. The effect is borrowed.
    (local $r i32) (local $headers i32) (local $body i64) (local $pair i64) (local $req i32)
    (local $contents i32) (local $method i64) (local $case i32) (local $code i32)
    (local.set $r (call $alloc (i32.const {REQ_SIZE})))
    (memory.fill (local.get $r) (i32.const 0) (i32.const {REQ_SIZE}))
    (i32.store (local.get $r) (global.get $reqs))
    (global.set $reqs (local.get $r))
    (i32.store offset=4 (local.get $r) (local.get $id))
    (i64.store offset=8 (local.get $r) (call $dup (i64.load offset=32 (local.get $e))))
    (i32.store offset=16 (local.get $r) (local.get $set))
    (local.set $body (call $dup (i64.load offset=24 (local.get $e))))
    (i64.store offset=96 (local.get $r) (local.get $body))
    (local.set $headers (call $fields_new))
    (if (call $len (local.get $body))
      (then
        (call $fields_append (local.get $headers)
          (call $str_data (i64.const {content_type})) (call $len (i64.const {content_type}))
          (call $str_data (i64.const {json})) (call $len (i64.const {json}))
          (i32.add (local.get $r) (i32.const 56)))
        (local.set $pair (call $body_new))
        (local.set $contents (i32.wrap_i64 (local.get $pair)))
        (i32.store offset=20 (local.get $r) (i32.wrap_i64 (i64.shr_u (local.get $pair) (i64.const 32))))))
    (local.set $pair (call $trailers_new))
    (i32.store offset=24 (local.get $r) (i32.wrap_i64 (i64.shr_u (local.get $pair) (i64.const 32))))
    (call $request_new (local.get $headers)
      (i32.ne (local.get $contents) (i32.const 0)) (local.get $contents)
      (i32.wrap_i64 (local.get $pair))
      (i32.const 0) (i32.const 0)
      (i32.add (local.get $r) (i32.const 56)))
    (local.set $req (i32.load offset=56 (local.get $r)))
    (call $sent_drop (i32.load offset=60 (local.get $r)))
    (local.set $method (i64.load offset=8 (local.get $e)))
    (local.set $case (call $method_case (local.get $method)))
    (drop (call $set_method (local.get $req) (local.get $case)
      (call $str_data (local.get $method)) (call $len (local.get $method))))
    (call $set_url (local.get $req) (i64.load offset=16 (local.get $e)))
    (if (i32.load offset=20 (local.get $r)) (then (call $write_body (local.get $r))))
    (local.set $code (call $trailers_write (i32.load offset=24 (local.get $r)) (i32.const {zeros})))
    (call $wrote_future (local.get $r) (i32.const 24) (local.get $code))
    (local.set $code (call $send (local.get $req) (i32.add (local.get $r) (i32.const 56))))
    (if (i32.eq (i32.and (local.get $code) (i32.const 15)) (i32.const 2))
      (then (call $responded (local.get $r)))
      (else
        (i32.store offset=28 (local.get $r) (i32.shr_u (local.get $code) (i32.const 4)))
        (call $join (i32.load offset=28 (local.get $r)) (local.get $set))))
    (call $settle (local.get $r)))
  (func $write_body (param $r i32)
    ;; Write what is left of the body, until a write waits or the body
    ;; is all written, which ends the stream.
    (local $w i32) (local $body i64) (local $code i32)
    (local.set $w (i32.load offset=20 (local.get $r)))
    (local.set $body (i64.load offset=96 (local.get $r)))
    (block $done (loop $next
      (if (i32.ge_u (i32.load offset=104 (local.get $r)) (call $len (local.get $body)))
        (then
          (call $join (local.get $w) (i32.const 0))
          (call $body_drop (local.get $w))
          (i32.store offset=20 (local.get $r) (i32.const 0))
          (br $done)))
      (local.set $code (call $body_write (local.get $w)
        (i32.add (call $str_data (local.get $body)) (i32.load offset=104 (local.get $r)))
        (i32.sub (call $len (local.get $body)) (i32.load offset=104 (local.get $r)))))
      (if (i32.eq (local.get $code) (i32.const -1))
        (then (call $join (local.get $w) (i32.load offset=16 (local.get $r))) (br $done)))
      (br_if $next (call $wrote (local.get $r) (local.get $code))))))
  (func $wrote (param $r i32) (param $code i32) (result i32)
    ;; Count what a body write wrote; 1 if the stream takes more.
    (i32.store offset=104 (local.get $r)
      (i32.add (i32.load offset=104 (local.get $r)) (i32.shr_u (local.get $code) (i32.const 4))))
    (if (i32.and (local.get $code) (i32.const 15))
      (then
        ;; The host dropped the stream: nothing more goes.
        (i32.store offset=104 (local.get $r) (call $len (i64.load offset=96 (local.get $r))))))
    (i32.const 1))
  (func $wrote_future (param $r i32) (param $at i32) (param $code i32)
    ;; A future write at `at` in the request finished at once or waits.
    (local $w i32)
    (local.set $w (i32.load (i32.add (local.get $r) (local.get $at))))
    (if (i32.eq (local.get $code) (i32.const -1))
      (then (call $join (local.get $w) (i32.load offset=16 (local.get $r))))
      (else (call $close_future (local.get $r) (local.get $at)))))
  (func $close_future (param $r i32) (param $at i32)
    (local $w i32)
    (local.set $w (i32.load (i32.add (local.get $r) (local.get $at))))
    (call $join (local.get $w) (i32.const 0))
    (if (i32.eq (local.get $at) (i32.const 24))
      (then (call $trailers_drop (local.get $w)))
      (else (call $done_drop (local.get $w))))
    (i32.store (i32.add (local.get $r) (local.get $at)) (i32.const 0)))
  (func $responded (param $r i32)
    ;; `send` returned a `result<response, error-code>` at offset 56.
    (local $resp i32) (local $pair i64) (local $code i32)
    (if (i32.load8_u offset=56 (local.get $r))
      (then
        ;; No response: the handler hears status 0 and no body.
        (i32.store offset=40 (local.get $r) (i32.const 0))
        (call $finish (local.get $r))
        (return)))
    (local.set $resp (i32.load offset=64 (local.get $r)))
    (i32.store offset=40 (local.get $r) (call $status_code (local.get $resp)))
    (local.set $pair (call $done_new))
    (i32.store offset=32 (local.get $r) (i32.wrap_i64 (i64.shr_u (local.get $pair) (i64.const 32))))
    (call $consume_body (local.get $resp) (i32.wrap_i64 (local.get $pair)) (i32.add (local.get $r) (i32.const 56)))
    (i32.store offset=36 (local.get $r) (i32.load offset=56 (local.get $r)))
    (call $rtrailers_drop (i32.load offset=60 (local.get $r)))
    (local.set $code (call $done_write (i32.load offset=32 (local.get $r)) (i32.const {zeros})))
    (call $wrote_future (local.get $r) (i32.const 32) (local.get $code))
    (i32.store offset=52 (local.get $r) (i32.const 1024))
    (i32.store offset=44 (local.get $r) (call $alloc (i32.const 1024)))
    (call $read_body (local.get $r)))
  (func $read_body (param $r i32)
    ;; Read the response's body until a read waits or the stream ends.
    (local $code i32) (local $grown i32)
    (block $done (loop $next
      (if (i32.eq (i32.load offset=48 (local.get $r)) (i32.load offset=52 (local.get $r)))
        (then
          (local.set $grown (call $alloc (i32.shl (i32.load offset=52 (local.get $r)) (i32.const 1))))
          (memory.copy (local.get $grown) (i32.load offset=44 (local.get $r)) (i32.load offset=48 (local.get $r)))
          (call $free (i32.load offset=44 (local.get $r)))
          (i32.store offset=44 (local.get $r) (local.get $grown))
          (i32.store offset=52 (local.get $r) (i32.shl (i32.load offset=52 (local.get $r)) (i32.const 1)))))
      (local.set $code (call $read (i32.load offset=36 (local.get $r))
        (i32.add (i32.load offset=44 (local.get $r)) (i32.load offset=48 (local.get $r)))
        (i32.sub (i32.load offset=52 (local.get $r)) (i32.load offset=48 (local.get $r)))))
      (if (i32.eq (local.get $code) (i32.const -1))
        (then (call $join (i32.load offset=36 (local.get $r)) (i32.load offset=16 (local.get $r))) (br $done)))
      (br_if $next (call $got (local.get $r) (local.get $code))))))
  (func $got (param $r i32) (param $code i32) (result i32)
    ;; Count what a read read; at the end of the stream, finish and
    ;; answer 0, else 1.
    (i32.store offset=48 (local.get $r)
      (i32.add (i32.load offset=48 (local.get $r)) (i32.shr_u (local.get $code) (i32.const 4))))
    (if (i32.eqz (i32.and (local.get $code) (i32.const 15))) (then (return (i32.const 1))))
    (call $join (i32.load offset=36 (local.get $r)) (i32.const 0))
    (call $read_drop (i32.load offset=36 (local.get $r)))
    (i32.store offset=36 (local.get $r) (i32.const 0))
    (call $finish (local.get $r))
    (i32.const 0))
  (func $finish (param $r i32)
    ;; Give `update` the response, under the effect's handler.
    (local $body i64) (local $handler i64)
    (local.set $body (call $str_new (i32.load offset=44 (local.get $r)) (i32.load offset=48 (local.get $r))))
    (if (i32.load offset=44 (local.get $r)) (then (call $free (i32.load offset=44 (local.get $r)))))
    (i32.store offset=44 (local.get $r) (i32.const 0))
    (local.set $handler (i64.load offset=8 (local.get $r)))
    (i64.store offset=8 (local.get $r) (i64.const 2))
    (i32.store offset=108 (local.get $r) (i32.const 1))
    (call $dispatch (i32.load offset=4 (local.get $r))
      (call $event_new (local.get $handler) (call $lit_owned (i64.const {response})) (local.get $body) (i64.const 2) (i64.const 2)
        (i64.or (i64.shl (i64.extend_i32_u (i32.load offset=40 (local.get $r))) (i64.const 1)) (i64.const 1)))))
  (func $settle (param $r i32)
    ;; Free a request once it has answered and holds no handles.
    (local $at i32)
    (if (i32.eqz (i32.load offset=108 (local.get $r))) (then (return)))
    (if (i32.or (i32.or (i32.load offset=20 (local.get $r)) (i32.load offset=24 (local.get $r)))
                (i32.or (i32.or (i32.load offset=28 (local.get $r)) (i32.load offset=32 (local.get $r)))
                        (i32.load offset=36 (local.get $r))))
      (then (return)))
    (if (i32.eq (global.get $reqs) (local.get $r))
      (then (global.set $reqs (i32.load (local.get $r))))
      (else
        (local.set $at (global.get $reqs))
        (block $done (loop $next
          (br_if $done (i32.eqz (local.get $at)))
          (if (i32.eq (i32.load (local.get $at)) (local.get $r))
            (then (i32.store (local.get $at) (i32.load (local.get $r))) (br $done)))
          (local.set $at (i32.load (local.get $at)))
          (br $next)))))
    (call $drop (i64.load offset=96 (local.get $r)))
    (call $free (local.get $r)))
  (func $fetch_event (param $event i32) (param $w i32) (param $payload i32)
    ;; Carry on the request that `w` belongs to.
    (local $r i32)
    (local.set $r (global.get $reqs))
    (block $found (loop $next
      (if (i32.eqz (local.get $r)) (then (return)))
      (br_if $found (i32.and (i32.eq (local.get $event) (i32.const 1)) (i32.eq (i32.load offset=28 (local.get $r)) (local.get $w))))
      (br_if $found (i32.and (i32.eq (local.get $event) (i32.const 2)) (i32.eq (i32.load offset=36 (local.get $r)) (local.get $w))))
      (br_if $found (i32.and (i32.eq (local.get $event) (i32.const 3)) (i32.eq (i32.load offset=20 (local.get $r)) (local.get $w))))
      (br_if $found (i32.and (i32.eq (local.get $event) (i32.const 5))
        (i32.or (i32.eq (i32.load offset=24 (local.get $r)) (local.get $w)) (i32.eq (i32.load offset=32 (local.get $r)) (local.get $w)))))
      (local.set $r (i32.load (local.get $r)))
      (br $next)))
    (if (i32.eq (local.get $event) (i32.const 1))
      (then
        ;; `send`'s subtask: only its return matters.
        (if (i32.ne (local.get $payload) (i32.const 2)) (then (return)))
        (call $join (local.get $w) (i32.const 0))
        (call $subtask_drop (local.get $w))
        (i32.store offset=28 (local.get $r) (i32.const 0))
        (call $responded (local.get $r))))
    (if (i32.eq (local.get $event) (i32.const 2))
      (then (if (call $got (local.get $r) (local.get $payload)) (then (call $read_body (local.get $r))))))
    (if (i32.eq (local.get $event) (i32.const 3))
      (then (drop (call $wrote (local.get $r) (local.get $payload))) (call $write_body (local.get $r))))
    (if (i32.eq (local.get $event) (i32.const 5))
      (then
        (call $close_future (local.get $r)
          (select (i32.const 24) (i32.const 32) (i32.eq (i32.load offset=24 (local.get $r)) (local.get $w))))))
    (call $settle (local.get $r)))
  (func $fetch_pending (param $set i32) (result i32)
    (local $r i32)
    (local.set $r (global.get $reqs))
    (block $done (loop $next
      (br_if $done (i32.eqz (local.get $r)))
      (if (i32.eq (i32.load offset=16 (local.get $r)) (local.get $set)) (then (return (i32.const 1))))
      (local.set $r (i32.load (local.get $r)))
      (br $next)))
    (i32.const 0))
"#
    )
}
