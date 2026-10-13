//! The fetch glue against Wasmtime's own `wasi:http@0.3.0`, the
//! implementation conduct's components target outside the browser.
//! Wasmtime builds a request's URI from its scheme and authority, so
//! these requests use absolute URLs; the hook that sends them answers in
//! process.

use bytes::Bytes;
use conduct::element;
use http_body_util::{BodyExt, Full};
use std::future::Future;
use std::sync::{Arc, Mutex};
use wasmtime::component::{Component, Linker, ResourceTable, Val};
use wasmtime::{Config, Engine, Store, StoreContextMut};
use wasmtime_wasi_http::{WasiBody, WasiHttpCtx, WasiHttpCtxView, WasiHttpHooks, WasiHttpView};

type Seen = Arc<Mutex<Vec<String>>>;

/// Answers each request with its method, URI, and body, or with 5000
/// bytes for `/big`, and keeps a line for each.
struct Echo {
    seen: Seen,
}

type HookFuture = Box<dyn Future<Output = wasmtime_wasi_http::Result<()>> + Send>;

impl WasiHttpHooks for Echo {
    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        _options: Option<wasmtime_wasi_http::RequestOptions>,
        _fut: HookFuture,
    ) -> Box<dyn Future<Output = wasmtime_wasi_http::Result<(http::Response<WasiBody>, HookFuture)>> + Send> {
        let seen = self.seen.clone();
        Box::new(async move {
            let (parts, body) = request.into_parts();
            let body = body.collect().await?.to_bytes();
            let line = format!("{} {} {}", parts.method, parts.uri, String::from_utf8_lossy(&body));
            seen.lock().unwrap().push(line.trim_end().to_string());
            let answer = if parts.uri.path() == "/big" { "x".repeat(5000) } else { line };
            let body = Full::new(Bytes::from(answer)).map_err(|never| match never {}).boxed_unsync();
            let response = http::Response::builder().status(201).body(body).unwrap();
            Ok((response, Box::new(async { Ok(()) }) as HookFuture))
        })
    }
}

struct State {
    table: ResourceTable,
    http: WasiHttpCtx,
    hooks: Echo,
    emitted: Vec<String>,
}

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView { ctx: &mut self.http, table: &mut self.table, hooks: &mut self.hooks }
    }
}

/// An element that asks for each URL in its `urls` attribute, one per
/// line, when it connects, and reports each response as a custom event.
/// It posts a body to each URL but `/big`, which it gets with none.
const FETCHER: &str = r#"
(type n : (t/n integer))
(define attributes ["urls"])
(define (init attrs) {n: 0})
(define (ask url)
  (if (= (count (split url "/big")) 1)
    (fetch "POST" url (str "{\"url\":\"" url "\"}") "answer")
    (fetch "GET" url "" "answer")))
(define (update m handler event attrs)
  (cond
    (= handler "connected")
      {model: m
       effects: (map (filter (split (get :urls of: attrs or: "") "\n") where: (fn [u] (not (= u "")))) each: ask)}
    (= handler "answer")
      {model: {n: (+ (n m) 1)}
       effects: [(emit "answered" (str (status event 0) " " (value event "")))]}
    :else {model: m, effects: []}))
(define (view m attrs) (h "p" {} [(str (n m))] {}))
"#;

async fn run(urls: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut cb = element::codebase();
    cb.add_source(FETCHER).unwrap_or_else(|e| panic!("{e}"));
    let bytes = element::component(&cb).unwrap_or_else(|e| panic!("{e}"));

    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config).unwrap();
    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi_http::p3::add_to_linker(&mut linker).unwrap();
    linker
        .instance("demo:element/host")
        .unwrap()
        .func_wrap("emit", |mut cx: StoreContextMut<'_, State>, (_id, name, detail): (u32, String, String)| {
            cx.data_mut().emitted.push(format!("{name} {detail}"));
            Ok(())
        })
        .unwrap();
    let component = Component::new(&engine, &bytes).unwrap();
    let seen = Seen::default();
    let state = State {
        table: ResourceTable::new(),
        http: WasiHttpCtx::new(),
        hooks: Echo { seen: seen.clone() },
        emitted: Vec::new(),
    };
    let mut store = Store::new(&engine, state);
    let instance = linker.instantiate_async(&mut store, &component).await.unwrap();
    let iface = instance.get_export_index(&mut store, None, "demo:element/element").unwrap();
    let func = |store: &mut Store<State>, name: &str| {
        let index = instance.get_export_index(&mut *store, Some(&iface), name).unwrap();
        instance.get_func(&mut *store, index).unwrap()
    };

    let attrs = Val::List(vec![Val::Tuple(vec![Val::String("urls".into()), Val::String(urls.join("\n"))])]);
    let mut results = [Val::U32(0)];
    func(&mut store, "create").call_async(&mut store, &[attrs], &mut results).await.unwrap();
    let id = results[0].clone();
    func(&mut store, "connected").call_async(&mut store, &[id], &mut []).await.unwrap();
    let mut seen = seen.lock().unwrap().clone();
    seen.sort();
    let mut emitted = std::mem::take(&mut store.data_mut().emitted);
    emitted.sort();
    (seen, emitted)
}

#[tokio::test(flavor = "current_thread")]
async fn requests_go_through_wasmtime() {
    let (seen, emitted) = run(&["http://api.test/echo?x=1"]).await;
    assert_eq!(seen, [r#"POST http://api.test/echo?x=1 {"url":"http://api.test/echo?x=1"}"#]);
    let echoed = r#"POST http://api.test/echo?x=1 {"url":"http://api.test/echo?x=1"}"#;
    assert_eq!(emitted, [format!("answered 201 {echoed}")]);
}

#[tokio::test(flavor = "current_thread")]
async fn bodies_longer_than_a_read_and_requests_at_once() {
    let (seen, emitted) = run(&["https://api.test/big", "http://other.test"]).await;
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0], "GET https://api.test/big");
    assert!(seen[1].starts_with("POST http://other.test/ "), "{seen:?}");
    assert_eq!(emitted.len(), 2, "{emitted:?}");
    // Sorted, the echo comes before the long body.
    assert!(emitted[0].starts_with("answered 201 POST http://other.test/ "), "{emitted:?}");
    assert_eq!(emitted[1], format!("answered 201 {}", "x".repeat(5000)));
}
