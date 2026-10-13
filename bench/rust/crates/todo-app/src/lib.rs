//! `<todo-app>`: the TodoMVC application. It owns the list: it is the
//! one element that calls the API, and it passes the todos to its
//! children as attributes. A child reports what a person did with a
//! custom event, and this element calls a route and renders again with
//! the new list. Its `filter` attribute holds the filter, which the page
//! sets from the URL hash.
//!
//! The page shows a spinner inside this element while it compiles the
//! elements, and the element keeps showing it, through a slot, until the
//! list has loaded. Then the element reveals the list: it lays the list
//! out out of sight to learn its height, and then, at once, grows or
//! shrinks its frame to that height, fades the spinner out as it rises,
//! and fades the list in as it rises into place.
//!
//! Its handlers await requests, so it implements the element interface
//! itself rather than through `element_kit::Element`: each element is
//! shared, and borrowed only between awaits, so that the host can render
//! it while a request is in flight.

mod http;

use element_kit::exports::demo::element::element::Guest;
use element_kit::wasi::http::types::Method;
use element_kit::{Event, Html, Node, Registry, View, attribute, h};
use serde::Deserialize;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

/// The list, as `GET /api/todos` answers it.
#[derive(Deserialize, Default)]
#[serde(default)]
struct List {
    todos: Vec<Todo>,
    counts: Counts,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Todo {
    id: String,
    title: String,
    completed: bool,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
struct Counts {
    active: u32,
    completed: u32,
}

/// How far the reveal has gone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The list waits out of sight for its height.
    Measuring,
    /// The frame moves to the list's height.
    Revealing,
    /// The list stands in the frame on its own.
    Shown,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Measuring => "measuring",
            Phase::Revealing => "revealing",
            Phase::Shown => "shown",
        }
    }
}

struct TodoApp {
    /// The filter the `filter` attribute names, or `all`.
    filter: String,
    list: List,
    phase: Phase,
    /// The height of the list, in CSS pixels, as the host last reported it.
    height: String,
    /// Whether the words about the demo show, below the list.
    about: bool,
}

/// The filter the attribute `filter` names, or all of them.
fn filter_of(name: &str) -> String {
    if name.is_empty() { "all".into() } else { name.into() }
}

type Shared = Rc<RefCell<TodoApp>>;

thread_local! {
    static APPS: RefCell<Registry<Shared>> = RefCell::new(Registry::default());
}

fn app(id: u32) -> Shared {
    APPS.with_borrow(|apps| apps.get(id).clone())
}

/// A change to the list: a request, after which the list loads again.
struct Change {
    method: Method,
    path: String,
    body: Option<String>,
}

impl Change {
    fn new(method: Method, path: impl Into<String>, body: Option<serde_json::Value>) -> Change {
        Change { method, path: path.into(), body: body.map(|b| b.to_string()) }
    }
}

/// Load the list `app` shows, and keep it. A request that fails leaves
/// the list as it was.
async fn load(app: &Shared) {
    let path = format!("/api/todos?filter={}", app.borrow().filter);
    let Ok(body) = http::request(Method::Get, &path, None).await else { return };
    if let Ok(list) = serde_json::from_slice::<List>(&body) {
        app.borrow_mut().list = list;
    }
}

/// Make each change, and load the list after each.
async fn apply(app: &Shared, changes: Vec<Change>) {
    for Change { method, path, body } in changes {
        let _ = http::request(method, &path, body).await;
        load(app).await;
    }
}

impl TodoApp {
    /// What the handler `handler` does with `event`: a step of the reveal
    /// changes the element, and anything a person did becomes changes to
    /// the list.
    fn on(&mut self, handler: &str, event: Event) -> Vec<Change> {
        match (handler, event.value) {
            ("measured", Some(height)) => {
                self.height = height;
                if self.phase == Phase::Measuring {
                    self.phase = Phase::Revealing;
                }
                vec![]
            }
            ("revealed", _) => {
                if self.phase == Phase::Revealing {
                    self.phase = Phase::Shown;
                }
                vec![]
            }
            ("about", _) => {
                self.about = !self.about;
                vec![]
            }
            ("add", Some(title)) => {
                vec![Change::new(Method::Post, "/api/todos", Some(json!({ "title": title })))]
            }
            ("toggle-all", Some(all)) => {
                vec![Change::new(Method::Patch, "/api/todos", Some(json!({ "completed": all == "true" })))]
            }
            // The todo with that id, if there is one, flips.
            ("toggle", Some(which)) => self
                .list
                .todos
                .iter()
                .filter(|todo| todo.id == which)
                .map(|todo| {
                    let path = format!("/api/todos/{}", todo.id);
                    Change::new(Method::Patch, path, Some(json!({ "completed": !todo.completed })))
                })
                .collect(),
            ("destroy", Some(which)) => vec![Change::new(Method::Delete, format!("/api/todos/{which}"), None)],
            // The detail is the id, a newline, and the new title. An edit
            // to an empty title deletes the todo.
            ("edit", Some(detail)) => {
                let mut parts = detail.split('\n');
                let id = parts.next().unwrap_or("");
                let title = parts.next().unwrap_or("").trim();
                let path = format!("/api/todos/{id}");
                if title.is_empty() {
                    vec![Change::new(Method::Delete, path, None)]
                } else {
                    vec![Change::new(Method::Patch, path, Some(json!({ "title": title })))]
                }
            }
            ("clear", _) => vec![Change::new(Method::Delete, "/api/todos", None)],
            _ => vec![],
        }
    }

    // Attributes and events are listed by name, in the order conduct's
    // dictionaries keep them, so the node lists match conduct's exactly.
    fn render(&self) -> View {
        let Counts { active, completed } = self.list.counts;
        let total = active + completed;
        let flag = |b: bool| if b { "true" } else { "false" };
        let header = h("header").attr("class", "header").child(h("h1").child("todos")).child(
            h("todo-input")
                .attr("all-completed", flag(total > 0 && active == 0))
                .attr("has-todos", flag(total > 0))
                .on("todo-add", "add")
                .on("todo-toggle-all", "toggle-all"),
        );
        let mut list = h("section").attr("class", "todoapp").child(header);
        if total > 0 {
            let items = self.list.todos.iter().map(|todo| {
                h("todo-item")
                    .key(todo.id.as_str())
                    .attr("completed", flag(todo.completed))
                    .attr("title", todo.title.as_str())
                    .attr("todo-id", todo.id.as_str())
                    .on("todo-destroy", "destroy")
                    .on("todo-edit", "edit")
                    .on("todo-toggle", "toggle")
            });
            list = list
                .child(h("section").attr("class", "main").child(h("ul").attr("class", "todo-list").children(items)))
                .child(
                    h("todo-footer")
                        .attr("active", active.to_string())
                        .attr("completed", completed.to_string())
                        .attr("filter", self.filter.as_str())
                        .on("todo-clear", "clear"),
                );
        }
        let content = h("div").key("content").attr("class", "content").child(list).child(self.info());
        let phase = self.phase.as_str();
        if self.phase == Phase::Shown {
            return h("div").attr("class", "frame").attr("data-phase", phase).child(content).into();
        }
        let mut frame = h("div").attr("class", "frame").attr("data-phase", phase);
        if self.phase == Phase::Revealing {
            frame = frame.attr("style", format!("height: {}px", self.height));
        }
        frame
            // The spinner is the page's, which the slot shows.
            .child(h("div").key("spinner").attr("class", "spinner").child(h("slot")))
            .child(content.on("resize", "measured").on("transitionend", "revealed"))
            .into()
    }

    /// Below the list: the pill that shows the words about the demo, and
    /// the words while it is open.
    fn info(&self) -> Html {
        let toggle = h("button")
            .attr("aria-controls", "about")
            .attr("aria-expanded", if self.about { "true" } else { "false" })
            .attr("class", "about-toggle")
            .attr("type", "button")
            .child(h("span").attr("aria-hidden", "true").attr("class", "icon").child("i"))
            .child("About this demo")
            .on("click", "about");
        let about = self.about.then(|| h("div").attr("class", "about").attr("id", "about").children(about()));
        h("div").attr("class", "info").child(toggle).child_if(about)
    }
}

/// A link out of the demo, which opens in a new tab.
fn link(href: &str, label: &str) -> View {
    h("a").attr("href", href).attr("rel", "noopener").attr("target", "_blank").child(label).into()
}

/// The words about the demo: what it shows, and how it is made.
fn about() -> [Html; 3] {
    [
        h("p").children::<View>([
            "This app demonstrates some of the potential of the ".into(),
            link("https://component-model.bytecodealliance.org/", "Wasm Component Model"),
            ". It has not shipped in web browsers, but you can use it today via the ".into(),
            link("https://github.com/dialog-db/wasm-component-model-polyfill", "Wasm Component Model Polyfill"),
            ".".into(),
        ]),
        h("p").children::<View>([
            "The site consists of a Rust app shell and a Rust service worker, shipped as two Wasm core \
             modules with a "
                .into(),
            link("https://wasm-bindgen.github.io/wasm-bindgen/", "wasm-bindgen"),
            " JS shim. The shell defines a closed set of custom elements and the service worker a closed \
             set of routes."
                .into(),
        ]),
        h("p").children::<View>([
            "Each element and each route is implemented in a ".into(),
            link("https://github.com/Gozala/conduct", "conduct"),
            " source file, and the conduct compiler, itself a Wasm component, compiles each one in your \
             browser when the page loads. Open the sources below to read the type it infers for each \
             definition, or to change one and see it compile again."
                .into(),
        ]),
    ]
}

struct Component;

impl Guest for Component {
    async fn observed_attributes() -> Vec<String> {
        vec!["filter".into()]
    }

    async fn styles() -> String {
        include_str!("styles.css").into()
    }

    async fn create(attributes: Vec<(String, String)>) -> u32 {
        let app = TodoApp {
            filter: filter_of(attribute(&attributes, "filter").unwrap_or_default()),
            list: List::default(),
            phase: Phase::Measuring,
            height: String::new(),
            about: false,
        };
        APPS.with_borrow_mut(|apps| apps.insert(|_host| Rc::new(RefCell::new(app))))
    }

    async fn connected(id: u32) {
        load(&app(id)).await;
    }

    async fn disconnected(id: u32) {
        APPS.with_borrow_mut(|apps| apps.remove(id));
    }

    async fn attribute_changed(id: u32, name: String, value: Option<String>) {
        if let ("filter", Some(value)) = (name.as_str(), value) {
            let app = app(id);
            app.borrow_mut().filter = filter_of(&value);
            load(&app).await;
        }
    }

    async fn handle_event(id: u32, handler: String, event: Event) {
        let app = app(id);
        let changes = app.borrow_mut().on(&handler, event);
        apply(&app, changes).await;
    }

    async fn render(id: u32) -> Vec<Node> {
        app(id).borrow().render().into_nodes()
    }
}

element_kit::export!(Component with_types_in element_kit);
