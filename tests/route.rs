//! Routes of the TodoMVC demo, written in conduct and served by
//! Wasmtime's own `wasi:http@0.3.0` handler, with the todo model a host
//! of `async` functions that suspend before they answer.

use bytes::Bytes;
use conduct::route;
use http_body_util::{BodyExt, Full};
use std::future::Future;
use std::sync::{Arc, Mutex};
use wasmtime::component::{Accessor, Component, Linker, ResourceTable, Val};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi_http::p3::Request;
use wasmtime_wasi_http::p3::bindings::Service;
use wasmtime_wasi_http::{WasiBody, WasiHttpCtx, WasiHttpCtxView, WasiHttpHooks, WasiHttpView};

fn load(src: &str) -> conduct::Codebase {
    let mut cb = route::codebase().unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

const TODOS: &str = include_str!("../examples/todos-route.cdt");
const TODO: &str = include_str!("../examples/todo-route.cdt");

#[derive(Clone, Debug, PartialEq)]
struct Todo {
    id: String,
    title: String,
    completed: bool,
}

/// The demo's todo model, in memory, with every call it hears.
#[derive(Default)]
struct Model {
    todos: Vec<Todo>,
    next: u32,
    calls: Vec<String>,
}

struct State {
    table: ResourceTable,
    http: WasiHttpCtx,
    hooks: Hooks,
    model: Arc<Mutex<Model>>,
}

type HookFuture = Box<dyn Future<Output = wasmtime_wasi_http::Result<()>> + Send>;

/// A route sends no requests of its own.
struct Hooks;

impl WasiHttpHooks for Hooks {
    fn send_request(
        &mut self,
        _request: http::Request<WasiBody>,
        _options: Option<wasmtime_wasi_http::RequestOptions>,
        _fut: HookFuture,
    ) -> Box<dyn Future<Output = wasmtime_wasi_http::Result<(http::Response<WasiBody>, HookFuture)>> + Send> {
        unreachable!("a route sent a request")
    }
}

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView { ctx: &mut self.http, table: &mut self.table, hooks: &mut self.hooks }
    }
}

fn todo_val(t: &Todo) -> Val {
    Val::Record(vec![
        ("id".into(), Val::String(t.id.clone())),
        ("title".into(), Val::String(t.title.clone())),
        ("completed".into(), Val::Bool(t.completed)),
    ])
}

fn text(v: &Val) -> String {
    match v {
        Val::String(s) => s.clone(),
        other => panic!("not a string: {other:?}"),
    }
}

/// Answer one call of the model, as the demo's service worker does.
fn call(model: &mut Model, name: &str, args: &[Val]) -> Val {
    model.calls.push(format!("{name}{args:?}"));
    let missing = || Val::Result(Err(Some(Box::new(Val::Variant("not-found".into(), None)))));
    match name {
        "query" => {
            let Val::Enum(filter) = &args[0] else { panic!("{args:?}") };
            let shown: Vec<Val> = model
                .todos
                .iter()
                .filter(|t| match filter.as_str() {
                    "active" => !t.completed,
                    "completed" => t.completed,
                    _ => true,
                })
                .map(todo_val)
                .collect();
            let active = model.todos.iter().filter(|t| !t.completed).count() as u32;
            let completed = model.todos.len() as u32 - active;
            Val::Tuple(vec![
                Val::List(shown),
                Val::Record(vec![("active".into(), Val::U32(active)), ("completed".into(), Val::U32(completed))]),
            ])
        }
        "add" => {
            let title = text(&args[0]).trim().to_string();
            if title.is_empty() {
                return Val::Result(Err(Some(Box::new(Val::Variant("empty-title".into(), None)))));
            }
            model.next += 1;
            let todo = Todo { id: format!("t{}", model.next), title, completed: false };
            model.todos.push(todo.clone());
            Val::Result(Ok(Some(Box::new(todo_val(&todo)))))
        }
        "update" => {
            let id = text(&args[0]);
            let Some(todo) = model.todos.iter_mut().find(|t| t.id == id) else { return missing() };
            if let Val::Option(Some(title)) = &args[1] {
                let title = text(title).trim().to_string();
                if title.is_empty() {
                    return Val::Result(Err(Some(Box::new(Val::Variant("empty-title".into(), None)))));
                }
                todo.title = title;
            }
            if let Val::Option(Some(done)) = &args[2] {
                todo.completed = matches!(**done, Val::Bool(true));
            }
            Val::Result(Ok(Some(Box::new(todo_val(todo)))))
        }
        "remove" => {
            let id = text(&args[0]);
            let Some(at) = model.todos.iter().position(|t| t.id == id) else { return missing() };
            model.todos.remove(at);
            Val::Result(Ok(None))
        }
        "toggle-all" => {
            let done = matches!(args[0], Val::Bool(true));
            model.todos.iter_mut().for_each(|t| t.completed = done);
            Val::Tuple(vec![])
        }
        "clear-completed" => {
            model.todos.retain(|t| !t.completed);
            Val::Tuple(vec![])
        }
        other => panic!("no model function {other}"),
    }
}

/// A response: status, headers, body.
type Answer = (u16, Vec<(String, String)>, String);

/// Serve each request with the route in `src`, in one instance, and
/// answer the responses. After each, the route's memory holds what it
/// held before the first.
async fn serve(src: &str, model: Arc<Mutex<Model>>, requests: &[(&str, &str, &str)]) -> Vec<Answer> {
    let cb = load(src);
    let bytes = route::debug_component(&cb).unwrap_or_else(|e| panic!("{e}"));
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, &bytes).unwrap();
    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi_http::p3::add_to_linker(&mut linker).unwrap();
    let mut model_instance = linker.instance("demo:todo/model").unwrap();
    for name in ["query", "add", "update", "remove", "toggle-all", "clear-completed"] {
        model_instance
            .func_new_concurrent(name, move |accessor: &Accessor<State>, _ty, args, results| {
                Box::pin(async move {
                    // Suspend first, so that the route's call blocks.
                    tokio::task::yield_now().await;
                    let answer = accessor.with(|mut store| {
                        let model = store.data_mut().model.clone();
                        let mut model = model.lock().unwrap();
                        call(&mut model, name, args)
                    });
                    match answer {
                        Val::Tuple(parts) if parts.is_empty() => {}
                        Val::Tuple(parts) if name == "query" => results[0] = Val::Tuple(parts),
                        other => results[0] = other,
                    }
                    Ok(())
                })
            })
            .unwrap();
    }
    let state = State { table: ResourceTable::new(), http: WasiHttpCtx::new(), hooks: Hooks, model };
    let mut store = Store::new(&engine, state);
    let instance = linker.instantiate_async(&mut store, &component).await.unwrap();
    let service = Service::new(&mut store, &instance).unwrap();
    let live = instance.get_typed_func::<(), (u32,)>(&mut store, "conduct:debug/memory#live");
    let live = match live {
        Ok(f) => f,
        Err(_) => {
            let iface = instance.get_export_index(&mut store, None, "conduct:debug/memory").unwrap();
            let index = instance.get_export_index(&mut store, Some(&iface), "live").unwrap();
            instance.get_typed_func::<(), (u32,)>(&mut store, &index).unwrap()
        }
    };
    let mut answers = Vec::new();
    let mut before = None;
    for (method, uri, body) in requests {
        let request = http::Request::builder()
            .method(*method)
            .uri(format!("http://todo.test{uri}"))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body.to_string())))
            .unwrap();
        let (request, _io) = Request::from_http(&mut Hooks, request);
        let answer = store
            .run_concurrent(async |accessor| {
                let response = service.handle(accessor, request).await.unwrap().unwrap();
                let response = accessor.with(|store| response.into_http(store, async { Ok(()) })).unwrap();
                let (parts, body) = response.into_parts();
                let body = body.collect().await.unwrap().to_bytes();
                let mut headers: Vec<(String, String)> =
                    parts.headers.iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string())).collect();
                headers.sort();
                (parts.status.as_u16(), headers, String::from_utf8(body.to_vec()).unwrap())
            })
            .await
            .unwrap();
        answers.push(answer);
        let (now,) = live.call_async(&mut store, ()).await.unwrap();
        match before {
            None => before = Some(now),
            Some(b) => assert_eq!(now, b, "the route's memory grew serving {method} {uri}"),
        }
    }
    answers
}

fn model(todos: &[(&str, bool)]) -> Arc<Mutex<Model>> {
    let mut m = Model::default();
    for (title, completed) in todos {
        m.next += 1;
        m.todos.push(Todo { id: format!("t{}", m.next), title: title.to_string(), completed: *completed });
    }
    Arc::new(Mutex::new(m))
}

#[test]
fn routes_fit_the_contract() {
    let todos = load(TODOS);
    assert_eq!(
        todos.type_of("handle").unwrap(),
        // The claim, `(handle! request) with: {model} : response`, is what
        // callers see.
        "((\\ {body: text, headers: (dict text), method: text, params: (dict text), path: text, search: (dict text)}) \
         : (task {body: text, code: integer, headers: (dict text)} with: {model}))"
    );
    route::component(&todos).unwrap_or_else(|e| panic!("{e}"));
    let todo = load(TODO);
    route::component(&todo).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_route_uses_only_the_model() {
    let mut cb = route::codebase().unwrap();
    cb.add_host("now", "clock", vec![], conduct::types::Ty::int()).unwrap();
    cb.add_source("(define (handle! r) (do (now!) (status 200)))").unwrap();
    let e = route::component(&cb).unwrap_err().to_string();
    assert!(e.contains("this uses `clock`, where only {model} may be used"), "{e}");
}

#[tokio::test(flavor = "current_thread")]
async fn the_list_route() {
    let m = model(&[("Buy milk", false), ("Walk dog", true)]);
    let answers = serve(
        TODOS,
        m.clone(),
        &[
            ("GET", "/api/todos", ""),
            ("GET", "/api/todos?filter=active", ""),
            ("POST", "/api/todos", r#"{"title": "Write tests"}"#),
            ("POST", "/api/todos", r#"{"title": "  "}"#),
            ("PATCH", "/api/todos", r#"{"completed": true}"#),
            ("PATCH", "/api/todos", r#"{"completed": "yes"}"#),
            ("GET", "/api/todos?filter=completed&x=%20y", ""),
            ("DELETE", "/api/todos", ""),
            ("PUT", "/api/todos", ""),
            ("GET", "/api/todos", ""),
        ],
    )
    .await;
    let json = || vec![("content-type".to_string(), "application/json".to_string()), route_header("/api/todos")];
    let empty = || vec![route_header("/api/todos")];
    let expected: Vec<Answer> = vec![
        (
            200,
            json(),
            r#"{"counts":{"active":1,"completed":1},"todos":[{"completed":false,"id":"t1","title":"Buy milk"},{"completed":true,"id":"t2","title":"Walk dog"}]}"#.into(),
        ),
        (200, json(), r#"{"counts":{"active":1,"completed":1},"todos":[{"completed":false,"id":"t1","title":"Buy milk"}]}"#.into()),
        (201, json(), r#"{"completed":false,"id":"t3","title":"Write tests"}"#.into()),
        (422, empty(), String::new()),
        (204, empty(), String::new()),
        (400, empty(), String::new()),
        (
            200,
            json(),
            r#"{"counts":{"active":0,"completed":3},"todos":[{"completed":true,"id":"t1","title":"Buy milk"},{"completed":true,"id":"t2","title":"Walk dog"},{"completed":true,"id":"t3","title":"Write tests"}]}"#.into(),
        ),
        (204, empty(), String::new()),
        (405, empty(), String::new()),
        (200, json(), r#"{"counts":{"active":0,"completed":0},"todos":[]}"#.into()),
    ];
    for (i, (got, want)) in answers.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "response {i}");
    }
    assert_eq!(answers.len(), expected.len());
    let calls = m.lock().unwrap().calls.clone();
    // A PATCH without a boolean, and a method the route does not serve,
    // never reach the model.
    assert_eq!(
        calls,
        [
            "query[Enum(\"all\")]",
            "query[Enum(\"active\")]",
            "add[String(\"Write tests\")]",
            "add[String(\"  \")]",
            "toggle-all[Bool(true)]",
            "query[Enum(\"completed\")]",
            "clear-completed[]",
            "query[Enum(\"all\")]",
        ]
    );
}

fn route_header(pattern: &str) -> (String, String) {
    ("x-demo-route".to_string(), pattern.to_string())
}

#[tokio::test(flavor = "current_thread")]
async fn the_item_route() {
    let m = model(&[("Buy milk", false), ("Walk dog", true)]);
    let answers = serve(
        TODO,
        m.clone(),
        &[
            ("PATCH", "/api/todos/t1", r#"{"completed": true}"#),
            ("PATCH", "/api/todos/t2", r#"{"title": "Walk the dog", "completed": false}"#),
            ("PATCH", "/api/todos/t2", r#"{"title": ""}"#),
            ("PATCH", "/api/todos/t9", r#"{"title": "Nope"}"#),
            ("DELETE", "/api/todos/t%31", ""),
            ("DELETE", "/api/todos/t1", ""),
            ("GET", "/api/todos/t2", ""),
        ],
    )
    .await;
    let json = || vec![("content-type".to_string(), "application/json".to_string()), route_header("/api/todos/:id")];
    let empty = || vec![route_header("/api/todos/:id")];
    let expected: Vec<Answer> = vec![
        (200, json(), r#"{"completed":true,"id":"t1","title":"Buy milk"}"#.into()),
        (200, json(), r#"{"completed":false,"id":"t2","title":"Walk the dog"}"#.into()),
        (422, empty(), String::new()),
        (404, empty(), String::new()),
        (204, empty(), String::new()),
        (404, empty(), String::new()),
        (405, empty(), String::new()),
    ];
    for (i, (got, want)) in answers.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "response {i}");
    }
    assert_eq!(answers.len(), expected.len());
    let todos = m.lock().unwrap().todos.clone();
    assert_eq!(todos, [Todo { id: "t2".into(), title: "Walk the dog".into(), completed: false }]);
    let calls = m.lock().unwrap().calls.clone();
    assert_eq!(calls[0], "update[String(\"t1\"), Option(None), Option(Some(Bool(true)))]");
}

#[tokio::test(flavor = "current_thread")]
async fn nothing_from_the_model_is_the_empty_map() {
    // The model answers `{}` for `toggle-all`; it equals the literal and
    // encodes as `{}`.
    let src = "(define pattern \"/x\")
               (define (handle! request)
                 (let [r (toggle-all! true)]
                   (json {:eq (= r {}) :json (to-json r)} status: 200)))";
    let answers = serve(src, model(&[]), &[("GET", "/x", "")]).await;
    assert_eq!(answers[0].2, r#"{"eq":true,"json":"{}"}"#);
}
