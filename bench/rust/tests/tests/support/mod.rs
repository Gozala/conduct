//! Copied from the repository's tests/support/mod.rs, without its conduct
//! routes. A host for elements that make requests: `wasi:http/types` from the
//! polyfill's TodoMVC demo (vendored in `http_types`, MPL-2.0, as the
//! demo's page defines it), a `wasi:http/client` whose `send` answers
//! from an in-process todo API instead of the network, and that API,
//! which behaves as the demo's routes do.

// Each test crate that includes this module uses only part of it.
#![allow(dead_code)]

#[allow(clippy::all)]
pub mod http_types;

use http_types::{Body, Fields, HttpHost, HttpTable, Response};
use serde_json::{Value, json};
use wcmp::{Accessor, Component, Error, ExternType, InterfaceIdentifier, Linker, Val};

/// The interface `send` belongs to.
pub const CLIENT: &str = "wasi:http/client@0.3.0";

/// What a store's host keeps for its components.
#[derive(Default)]
pub struct Host {
    pub http: HttpTable,
    pub api: TodoApi,
    /// Custom events the element sent: its id, the name, the detail.
    pub emitted: Vec<(u32, String, String)>,
    /// Every request the API answered, as `METHOD path body`.
    pub requests: Vec<String>,
}

impl HttpHost for Host {
    fn http(&mut self) -> &mut HttpTable {
        &mut self.http
    }
}

/// Define `wasi:http/types` and `wasi:http/client` in `linker`, if
/// `component` imports them.
pub fn define_http(linker: &mut Linker<Host>, component: &Component) -> Result<Option<http_types::Types>, Error> {
    let types = http_types::define(linker, component)?;
    let Some(ty) = component.imports.iter().find_map(|import| {
        let ExternType::Instance(instance) = &import.ty else { return None };
        if import.name.to_string() != CLIENT {
            return None;
        }
        instance.items.iter().find_map(|item| match &item.ty {
            ExternType::Function(ty) if item.name == "send" => Some(ty.clone()),
            _ => None,
        })
    }) else {
        return Ok(types);
    };
    let identifier: InterfaceIdentifier = CLIENT.parse().expect("the interface's name parses");
    linker.instance(&identifier).func_new_concurrent("send", ty, |accessor: &Accessor<Host>, args: Vec<Val>| {
        let accessor = accessor.clone();
        async move {
            let rep = match args.first() {
                Some(Val::Own(handle)) => handle.rep(),
                other => panic!("`send` was given {other:?}"),
            };
            let (request, body) = accessor.with(|store| {
                let http_types::Request { method, scheme, authority, path_with_query, body, .. } =
                    store.data_mut().http().take_request(rep).expect("a request the host holds");
                Ok::<_, Error>(((method, scheme, authority, path_with_query), http_types::read_body(store, body)?))
            })??;
            let body = String::from_utf8(body.await).expect("a UTF-8 body");
            let (method, scheme, authority, path) = request;
            let path = path.unwrap_or_else(|| "/".into());
            let routed: Option<(u16, Vec<(String, String)>, String)> = None;
            let response = accessor.with(|store| {
                let host = store.data_mut();
                let origin = match (scheme, authority) {
                    (Some(s), Some(a)) => format!("{}://{a}", s.to_ascii_lowercase()),
                    _ => String::new(),
                };
                host.requests.push(format!("{method} {origin}{path} {body}").trim_end().to_string());
                let (status, headers, answer) = match routed {
                    Some(answer) => answer,
                    None => {
                        let (status, answer) = host.api.handle(&method, &path, &body);
                        (status, vec![("content-type".into(), "application/json".into())], answer)
                    }
                };
                let response = Response {
                    status,
                    headers: Fields {
                        entries: headers.into_iter().map(|(k, v)| (k, v.into_bytes())).collect(),
                        immutable: true,
                    },
                    body: Body::Bytes(answer.into_bytes()),
                };
                let rep = host.http().insert_response(response);
                http_types::response_handle(store, rep)
            })??;
            Ok(vec![Val::Result(Ok(Some(Box::new(Val::Own(response)))))])
        }
    })?;
    Ok(types)
}

/// The demo's todo routes, in memory: `/api/todos` for the list and
/// `/api/todos/{id}` for one todo.
#[derive(Default)]
pub struct TodoApi {
    pub todos: Vec<Todo>,
    next: u32,
    /// Every call of the model, when routes answer.
    pub calls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

impl TodoApi {
    pub fn add(&mut self, title: &str, completed: bool) -> String {
        self.next += 1;
        let id = format!("t{}", self.next);
        self.todos.push(Todo { id: id.clone(), title: title.into(), completed });
        id
    }

    fn json(todo: &Todo) -> Value {
        json!({"id": todo.id, "title": todo.title, "completed": todo.completed})
    }

    /// Answer a request: its status and JSON body.
    pub fn handle(&mut self, method: &str, path: &str, body: &str) -> (u16, String) {
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let filter = query.strip_prefix("filter=").unwrap_or("all");
        match (method, path.strip_prefix("/api/todos")) {
            ("GET", Some("")) => {
                let shown = self.todos.iter().filter(|t| match filter {
                    "active" => !t.completed,
                    "completed" => t.completed,
                    _ => true,
                });
                let active = self.todos.iter().filter(|t| !t.completed).count();
                let out = json!({
                    "todos": shown.map(Self::json).collect::<Vec<_>>(),
                    "counts": {"active": active, "completed": self.todos.len() - active},
                });
                (200, out.to_string())
            }
            ("POST", Some("")) => match body["title"].as_str() {
                Some(title) if !title.trim().is_empty() => {
                    let id = self.add(title.trim(), false);
                    (201, Self::json(self.todos.iter().find(|t| t.id == id).unwrap()).to_string())
                }
                _ => (400, String::new()),
            },
            ("PATCH", Some("")) => match body["completed"].as_bool() {
                Some(done) => {
                    self.todos.iter_mut().for_each(|t| t.completed = done);
                    (204, String::new())
                }
                None => (400, String::new()),
            },
            ("DELETE", Some("")) => {
                self.todos.retain(|t| !t.completed);
                (204, String::new())
            }
            (_, Some(rest)) if rest.starts_with('/') => {
                let id = &rest[1..];
                let Some(at) = self.todos.iter().position(|t| t.id == id) else { return (404, String::new()) };
                match method {
                    "PATCH" => {
                        if let Some(done) = body["completed"].as_bool() {
                            self.todos[at].completed = done;
                        }
                        if let Some(title) = body["title"].as_str() {
                            self.todos[at].title = title.into();
                        }
                        (200, Self::json(&self.todos[at]).to_string())
                    }
                    "DELETE" => {
                        self.todos.remove(at);
                        (204, String::new())
                    }
                    _ => (405, String::new()),
                }
            }
            _ => (404, String::new()),
        }
    }
}
