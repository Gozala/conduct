//! The demo's service worker, for tests: the conduct routes, each in a
//! store of its own on the polyfill, answering requests as the demo's
//! router does, over one todo model.

use super::http_types::{self, Body, Fields, HttpHost, HttpTable, Request, Types};
use super::{Todo, TodoApi};
use conduct::route;
use std::sync::{Arc, Mutex};
use wcmp::{
    Accessor, Component, Engine, Error, ExternType, Func, Instance, InterfaceIdentifier, Linker, Store, Val, ValField,
};

/// The model's interface.
const MODEL: &str = "demo:todo/model";

/// What a route's store keeps: its HTTP resources and the model.
pub struct Worker {
    pub http: HttpTable,
    pub model: Arc<Mutex<TodoApi>>,
}

impl HttpHost for Worker {
    fn http(&mut self) -> &mut HttpTable {
        &mut self.http
    }
}

/// A route, instantiated.
pub struct Route {
    pub pattern: String,
    store: Store<Worker>,
    instance: Instance,
    handle: Func,
    types: Types,
}

/// A response: its status, headers, and body.
pub type Answer = (u16, Vec<(String, String)>, String);

/// The demo's routes, in conduct, over `model`.
pub struct Routes {
    pub routes: Vec<Route>,
    pub model: Arc<Mutex<TodoApi>>,
}

/// The routes the demo defines, in the order it matches them.
pub const SOURCES: [&str; 2] =
    [include_str!("../../examples/todos-route.cdt"), include_str!("../../examples/todo-route.cdt")];

/// Whether `path` matches `pattern`, as the demo's router decides.
fn matches(pattern: &str, path: &str) -> bool {
    let want: Vec<&str> = pattern.split('/').collect();
    let have: Vec<&str> = path.split('/').collect();
    want.len() == have.len() && want.iter().zip(&have).all(|(want, have)| want.starts_with(':') || want == have)
}

impl Routes {
    pub async fn new(model: Arc<Mutex<TodoApi>>) -> Routes {
        let mut routes = Vec::new();
        for src in SOURCES {
            routes.push(Route::new(src, model.clone()).await);
        }
        Routes { routes, model }
    }

    /// Answer a request with the first route whose pattern its path
    /// matches, or 404.
    pub async fn serve(&mut self, method: &str, path_with_query: &str, body: &str) -> Answer {
        let path = path_with_query.split('?').next().unwrap_or_default();
        match self.routes.iter_mut().find(|r| matches(&r.pattern, path)) {
            Some(route) => route.serve(method, path_with_query, body).await,
            None => (404, vec![], String::new()),
        }
    }

    /// How many blocks each route's memory holds.
    pub async fn live(&mut self) -> Vec<u32> {
        let mut out = Vec::new();
        for route in &mut self.routes {
            out.push(route.live().await);
        }
        out
    }
}

impl Route {
    async fn new(src: &str, model: Arc<Mutex<TodoApi>>) -> Route {
        let mut cb = route::codebase().unwrap_or_else(|e| panic!("{e}"));
        cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
        let pattern = route::check(&cb).unwrap().pattern.expect("a route with a pattern");
        let bytes = route::debug_component(&cb).unwrap_or_else(|e| panic!("{e}"));
        let backend = wcmp_wasm_core_wasmtime::Wasmtime::new().unwrap();
        let engine = Engine::with_backend(backend).unwrap();
        let component = Component::new(&engine, &bytes).await.unwrap();
        let mut linker: Linker<Worker> = Linker::new(&engine);
        let types = http_types::define(&mut linker, &component).unwrap().expect("the route imports wasi:http/types");
        define_model(&mut linker, &component).unwrap();
        let mut http = HttpTable::default();
        http.set_types(Some(types));
        let mut store = Store::new(&engine, Worker { http, model }).unwrap();
        let instance = linker.instantiate(&mut store, &component).await.unwrap();
        let handle = instance
            .exports()
            .instance("wasi:http/handler@0.3.0")
            .and_then(|handler| handler.func("handle"))
            .expect("the route exports wasi:http/handler");
        Route { pattern, store, instance, handle, types }
    }

    /// Hand the request to the route's `handle`, and read back the
    /// response, as the demo's router does.
    pub async fn serve(&mut self, method: &str, path_with_query: &str, body: &str) -> Answer {
        let rep = self.store.data_mut().http().insert_request(Request {
            method: method.to_string(),
            scheme: None,
            authority: None,
            path_with_query: Some(path_with_query.to_string()),
            headers: Fields { entries: vec![("content-type".into(), b"application/json".to_vec())], immutable: true },
            body: Body::Bytes(body.as_bytes().to_vec()),
        });
        let request = self.store.resource_new(self.types.request(), rep).unwrap();
        let handle = &self.handle;
        self.store
            .run_concurrent(async |accessor: &Accessor<Worker>| {
                let returned = handle.call_concurrent(accessor, &[Val::Own(request)]).await.unwrap();
                let rep = match returned.first() {
                    Some(Val::Result(Ok(Some(response)))) => match response.as_ref() {
                        Val::Own(response) => response.rep(),
                        other => panic!("`handle` answered {other:?}"),
                    },
                    other => panic!("`handle` answered {other:?}"),
                };
                let (status, headers, body) = accessor
                    .with(|store| {
                        let response = store.data_mut().http().take_response(rep).expect("a response the host holds");
                        let body = http_types::read_body(store, response.body).unwrap();
                        (response.status, response.headers, body)
                    })
                    .unwrap();
                let body = body.await;
                let mut headers: Vec<(String, String)> = headers
                    .entries
                    .into_iter()
                    .map(|(name, value)| (name, String::from_utf8(value).unwrap()))
                    .collect();
                headers.sort();
                (status, headers, String::from_utf8(body).unwrap())
            })
            .await
            .unwrap()
    }

    async fn live(&mut self) -> u32 {
        let iface = self.instance.exports().instance("conduct:debug/memory").expect("the debug interface");
        let f = iface.func("live").expect("live");
        match &f.call(&mut self.store, &[]).await.unwrap()[..] {
            [Val::U32(n)] => *n,
            other => panic!("live returned {other:?}"),
        }
    }
}

/// Define the model's functions the route imports, as the demo's
/// service worker does: each is `async`, and answers once it has had
/// the model.
fn define_model(linker: &mut Linker<Worker>, component: &Component) -> Result<(), Error> {
    let Some(items) =
        component.imports.iter().find_map(|import| match (&import.ty, import.name.to_string() == MODEL) {
            (ExternType::Instance(instance), true) => Some(instance.items.clone()),
            _ => None,
        })
    else {
        return Ok(());
    };
    let identifier: InterfaceIdentifier = MODEL.parse().expect("the interface's name parses");
    let mut model = linker.instance(&identifier);
    for item in items {
        let ExternType::Function(ty) = item.ty else { continue };
        let function = item.name.clone();
        model.func_new_concurrent(item.name, ty, move |accessor: &Accessor<Worker>, args: Vec<Val>| {
            let function = function.clone();
            let model = accessor.with(|store| store.data().model.clone());
            let model = model.expect("the store");
            async move {
                // Answer on a later turn, as a model in IndexedDB does.
                tokio::task::yield_now().await;
                let mut model = model.lock().unwrap();
                Ok(call(&mut model, &function, &args).into_iter().collect::<Vec<_>>())
            }
        })?;
    }
    Ok(())
}

fn record(fields: Vec<(&str, Val)>) -> Val {
    Val::Record(fields.into_iter().map(|(name, value)| ValField { name: name.into(), value }).collect())
}

fn todo_val(todo: &Todo) -> Val {
    record(vec![
        ("id", Val::String(todo.id.clone())),
        ("title", Val::String(todo.title.clone())),
        ("completed", Val::Bool(todo.completed)),
    ])
}

fn outcome(result: Result<Option<Val>, &str>) -> Val {
    match result {
        Ok(value) => Val::Result(Ok(value.map(Box::new))),
        Err(error) => Val::Result(Err(Some(Box::new(Val::Variant { discriminant: error.to_string(), payload: None })))),
    }
}

fn text(v: &Val) -> String {
    match v {
        Val::String(s) => s.clone(),
        other => panic!("not a string: {other:?}"),
    }
}

/// Answer one call of the model, as the demo's model does.
fn call(api: &mut TodoApi, function: &str, args: &[Val]) -> Option<Val> {
    api.calls.push(format!("{function} {args:?}"));
    match function {
        "query" => {
            let Val::Enum(filter) = &args[0] else { panic!("{args:?}") };
            let shown: Vec<Val> = api
                .todos
                .iter()
                .filter(|t| match filter.as_str() {
                    "active" => !t.completed,
                    "completed" => t.completed,
                    _ => true,
                })
                .map(todo_val)
                .collect();
            let active = api.todos.iter().filter(|t| !t.completed).count() as u32;
            let completed = api.todos.len() as u32 - active;
            Some(Val::Tuple(
                vec![
                    Val::List(shown.into()),
                    record(vec![("active", Val::U32(active)), ("completed", Val::U32(completed))]),
                ]
                .into(),
            ))
        }
        "add" => {
            let title = text(&args[0]).trim().to_string();
            if title.is_empty() {
                return Some(outcome(Err("empty-title")));
            }
            let id = api.add(&title, false);
            let todo = api.todos.iter().find(|t| t.id == id).unwrap();
            Some(outcome(Ok(Some(todo_val(todo)))))
        }
        "update" => {
            let id = text(&args[0]);
            let Some(at) = api.todos.iter().position(|t| t.id == id) else {
                return Some(outcome(Err("not-found")));
            };
            if let Val::Option(Some(title)) = &args[1] {
                let title = text(title).trim().to_string();
                if title.is_empty() {
                    return Some(outcome(Err("empty-title")));
                }
                api.todos[at].title = title;
            }
            if let Val::Option(Some(done)) = &args[2] {
                api.todos[at].completed = matches!(**done, Val::Bool(true));
            }
            Some(outcome(Ok(Some(todo_val(&api.todos[at])))))
        }
        "remove" => {
            let id = text(&args[0]);
            let Some(at) = api.todos.iter().position(|t| t.id == id) else {
                return Some(outcome(Err("not-found")));
            };
            api.todos.remove(at);
            Some(outcome(Ok(None)))
        }
        "toggle-all" => {
            let done = matches!(args[0], Val::Bool(true));
            api.todos.iter_mut().for_each(|t| t.completed = done);
            None
        }
        "clear-completed" => {
            api.todos.retain(|t| !t.completed);
            None
        }
        other => panic!("no model function {other}"),
    }
}
