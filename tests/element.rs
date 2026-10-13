//! `<todo-item>`, written in conduct, compiled to a component of the
//! demo's element world, and driven through the Wasm Component Model
//! Polyfill as the demo's page drives an element.

mod support;

use conduct::element;
use support::Host;
use wcmp::*;

type Emitted = Vec<(u32, String, String)>;

struct Element {
    store: Store<Host>,
    instance: Instance,
}

fn field<'a>(v: &'a Val, name: &str) -> &'a Val {
    let Val::Record(fields) = v else { panic!("expected a record, found {v:?}") };
    &fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}")).value
}

fn string(v: &Val) -> String {
    match v {
        Val::String(s) => s.clone(),
        other => panic!("expected a string, found {other:?}"),
    }
}

fn pairs(v: &Val) -> Vec<(String, Val)> {
    let Val::List(items) = v else { panic!("expected a list, found {v:?}") };
    items
        .iter()
        .map(|t| {
            let Val::Tuple(parts) = t else { panic!("expected a tuple, found {t:?}") };
            (string(&parts[0]), parts[1].clone())
        })
        .collect()
}

/// A rendered node list as HTML-like text: the key as `#key`,
/// properties as `.name=value`, events as `@event=handler`.
fn html(nodes: &[Val]) -> String {
    fn go(nodes: &[Val], i: usize, out: &mut String) {
        let node = &nodes[i];
        let kind = field(node, "kind");
        let Val::Variant { discriminant, payload } = kind else { panic!() };
        let payload = payload.as_deref().unwrap();
        if discriminant == "text" {
            out.push_str(&string(payload));
            return;
        }
        let tag = string(field(payload, "tag"));
        out.push('<');
        out.push_str(&tag);
        if let Val::Option(Some(key)) = field(node, "key") {
            out.push_str(&format!(" #{}", string(key)));
        }
        for (name, value) in pairs(field(payload, "attributes")) {
            out.push_str(&format!(" {name}=\"{}\"", string(&value)));
        }
        for (name, value) in pairs(field(payload, "properties")) {
            let Val::Variant { discriminant, payload } = value else { panic!() };
            let shown = match (discriminant.as_str(), payload.as_deref()) {
                ("text", Some(v)) => format!("\"{}\"", string(v)),
                ("flag", Some(Val::Bool(b))) => b.to_string(),
                other => panic!("unexpected property {other:?}"),
            };
            out.push_str(&format!(" .{name}={shown}"));
        }
        for (event, handler) in pairs(field(payload, "events")) {
            out.push_str(&format!(" @{event}={}", string(&handler)));
        }
        out.push('>');
        for (j, child) in nodes.iter().enumerate() {
            if field(child, "parent") == &Val::Option(Some(Box::new(Val::U32(i as u32)))) {
                go(nodes, j, out);
            }
        }
        out.push_str(&format!("</{tag}>"));
    }
    let mut out = String::new();
    go(nodes, 0, &mut out);
    out
}

fn event(kind: &str, value: Option<&str>, key: Option<&str>) -> Val {
    event_with(kind, value, None, key)
}

fn event_with(kind: &str, value: Option<&str>, checked: Option<bool>, key: Option<&str>) -> Val {
    let opt = |s: Option<&str>| Val::Option(s.map(|s| Box::new(Val::String(s.into()))));
    Val::Record(
        vec![
            ValField { name: "kind".into(), value: Val::String(kind.into()) },
            ValField { name: "value".into(), value: opt(value) },
            ValField { name: "checked".into(), value: Val::Option(checked.map(|c| Box::new(Val::Bool(c)))) },
            ValField { name: "key".into(), value: opt(key) },
        ]
        .into(),
    )
}

impl Element {
    async fn load(source: &str) -> Element {
        Element::load_with(source, Host::default()).await
    }

    async fn load_with(source: &str, host: Host) -> Element {
        let mut cb = element::codebase();
        cb.add_source(source).unwrap_or_else(|e| panic!("{e}"));
        let bytes = element::debug_component(&cb).unwrap_or_else(|e| panic!("{e}"));
        let backend = wcmp_wasm_core_wasmtime::Wasmtime::new().unwrap();
        let engine = Engine::with_backend(backend).unwrap();
        let mut store = Store::new(&engine, host).unwrap();
        let component = Component::new(&engine, &bytes).await.unwrap();
        let mut linker: Linker<Host> = Linker::new(&engine);
        let types = support::define_http(&mut linker, &component).unwrap();
        store.data_mut().http.set_types(types);
        let host: InterfaceIdentifier = "demo:element/host".parse().unwrap();
        linker
            .instance(&host)
            .func_wrap(
                "emit",
                |mut call: HostCall<'_, Host>, (id, name, detail): (u32, String, String)| -> wcmp::Result<()> {
                    call.data_mut().emitted.push((id, name, detail));
                    Ok(())
                },
            )
            .unwrap();
        let instance = linker.instantiate(&mut store, &component).await.unwrap();
        Element { store, instance }
    }

    async fn call(&mut self, name: &str, args: &[Val]) -> Box<[Val]> {
        let iface = self.instance.exports().instance("demo:element/element").expect("the element interface");
        let f = iface.func(name).unwrap_or_else(|| panic!("no export {name}"));
        f.call(&mut self.store, args).await.unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    async fn create(&mut self, attrs: &[(&str, &str)]) -> u32 {
        let list = attrs
            .iter()
            .map(|(k, v)| Val::Tuple(vec![Val::String((*k).into()), Val::String((*v).into())].into()))
            .collect::<Vec<_>>();
        match &self.call("create", &[Val::List(list.into())]).await[..] {
            [Val::U32(id)] => *id,
            other => panic!("create returned {other:?}"),
        }
    }

    async fn render(&mut self, id: u32) -> String {
        match &self.call("render", &[Val::U32(id)]).await[..] {
            [Val::List(nodes)] => html(nodes),
            other => panic!("render returned {other:?}"),
        }
    }

    /// How many blocks the element's memory holds.
    async fn live(&mut self) -> u32 {
        let iface = self.instance.exports().instance("conduct:debug/memory").expect("the debug interface");
        let f = iface.func("live").expect("live");
        match &f.call(&mut self.store, &[]).await.unwrap()[..] {
            [Val::U32(n)] => *n,
            other => panic!("live returned {other:?}"),
        }
    }

    async fn connect(&mut self, id: u32) {
        self.call("connected", &[Val::U32(id)]).await;
    }

    async fn handle(&mut self, id: u32, handler: &str, event: Val) {
        self.call("handle-event", &[Val::U32(id), Val::String(handler.into()), event]).await;
    }

    async fn set_attr(&mut self, id: u32, name: &str, value: Option<&str>) {
        let value = Val::Option(value.map(|v| Box::new(Val::String(v.into()))));
        self.call("attribute-changed", &[Val::U32(id), Val::String(name.into()), value]).await;
    }

    fn take_emitted(&mut self) -> Emitted {
        std::mem::take(&mut self.store.data_mut().emitted)
    }

    fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.data_mut().requests)
    }
}

const TODO_ITEM: &str = include_str!("../examples/todo-item.cdt");

#[tokio::test(flavor = "current_thread")]
async fn todo_item_constants() {
    let mut el = Element::load(TODO_ITEM).await;
    let styles = el.call("styles", &[]).await;
    assert!(matches!(&styles[0], Val::String(s) if s.contains(":host")));
    let observed = el.call("observed-attributes", &[]).await;
    assert_eq!(
        observed[0],
        Val::List(
            vec![Val::String("todo-id".into()), Val::String("title".into()), Val::String("completed".into())].into()
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn todo_item_renders_and_reports_changes() {
    let mut el = Element::load(TODO_ITEM).await;
    let id = el.create(&[("todo-id", "7"), ("title", "Buy milk"), ("completed", "false")]).await;
    assert_eq!(id, 1);
    assert_eq!(
        el.render(id).await,
        "<li class=\"\">\
         <input #toggle class=\"toggle\" type=\"checkbox\" aria-label=\"Toggle the todo\" .checked=false @change=toggle></input>\
         <label @dblclick=edit>Buy milk</label>\
         <button class=\"destroy\" aria-label=\"Delete the todo\" @click=destroy>×</button>\
         </li>"
    );

    // Toggling reports the todo's id and keeps the element as it was.
    el.handle(id, "toggle", event("change", None, None)).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-toggle".to_string(), "7".to_string())]);

    // The host flips the attribute, and the view follows.
    el.set_attr(id, "completed", Some("true")).await;
    let html = el.render(id).await;
    assert!(html.starts_with("<li class=\"completed\">"), "{html}");
    assert!(html.contains(".checked=true"), "{html}");

    // A double-click edits in place; Enter saves the new title.
    el.handle(id, "edit", event("dblclick", None, None)).await;
    assert_eq!(
        el.render(id).await,
        "<li class=\"editing\">\
         <input #edit class=\"edit\" autofocus=\"\" aria-label=\"Edit the todo\" .value=\"Buy milk\" @keydown=key @blur=save></input>\
         </li>"
    );
    el.handle(id, "key", event("keydown", Some("Buy oat milk"), Some("Enter"))).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-edit".to_string(), "7\nBuy oat milk".to_string())]);
    assert!(el.render(id).await.starts_with("<li class=\"completed\">"));

    // Escape cancels an edit without reporting anything.
    el.handle(id, "edit", event("dblclick", None, None)).await;
    el.handle(id, "key", event("keydown", Some("ignored"), Some("Escape"))).await;
    assert!(el.take_emitted().is_empty());
    assert!(!el.render(id).await.contains("editing"));

    // A second element keeps its own state.
    let other = el.create(&[("todo-id", "8"), ("title", "Walk")]).await;
    el.handle(other, "edit", event("dblclick", None, None)).await;
    assert!(el.render(other).await.contains("editing"));
    assert!(!el.render(id).await.contains("editing"));
    el.handle(other, "destroy", event("click", None, None)).await;
    assert_eq!(el.take_emitted(), vec![(other, "todo-destroy".to_string(), "8".to_string())]);
}

#[test]
fn contract_violations_are_type_errors() {
    let mut cb = element::codebase();
    // `view` must return a View.
    cb.add_source("(define (init attrs) 0) (define (view m attrs) \"not a view\")").unwrap();
    let err = element::check(&cb).err().unwrap().to_string();
    assert!(err.contains("`view` does not fit"), "{err}");

    // `update` must return the model `view` accepts.
    let mut cb = element::codebase();
    cb.add_source(
        "(type n : (t/n integer))
         (define (init attrs) {n: 0})
         (define (update m handler event attrs) {model: 1, effects: []})
         (define (view m attrs) (h \"p\" {} [(n m)]))",
    )
    .unwrap();
    let err = element::check(&cb).err().unwrap().to_string();
    assert!(err.contains("does not fit"), "{err}");
}

const TODO_APP: &str = include_str!("../examples/todo-app.cdt");

async fn add_and_destroy(el: &mut Element, id: u32, n: usize) {
    el.handle(id, "add", event("todo-add", Some(&format!("Todo {n}")), None)).await;
    let added = el.store.data().api.todos.last().unwrap().id.clone();
    el.handle(id, "destroy", event("todo-destroy", Some(&added), None)).await;
    el.render(id).await;
}

/// `<todo-app>` against an in-process copy of the demo's API: each
/// export that runs `update` finishes after the requests it caused have
/// been answered and their responses have reached `update`, so a render
/// after it shows the result.
///
/// Its model flows back into its own updates, which once sent the
/// contract check around a cycle forever; building it checks that too.
#[tokio::test(flavor = "current_thread")]
async fn todo_app_talks_to_the_api() {
    let mut host = Host::default();
    host.api.add("Buy milk", false);
    host.api.add("Walk", true);
    let mut el = Element::load_with(TODO_APP, host).await;
    let id = el.create(&[("filter", "")]).await;
    assert!(el.render(id).await.contains("spinner"));

    el.connect(id).await;
    assert_eq!(el.take_requests(), ["GET /api/todos?filter=all"]);
    let page = el.render(id).await;
    assert!(page.contains("<todo-item #t1 todo-id=\"t1\" title=\"Buy milk\" completed=\"false\""), "{page}");
    assert!(page.contains("<todo-item #t2 todo-id=\"t2\" title=\"Walk\" completed=\"true\""), "{page}");
    assert!(page.contains("<todo-footer active=\"1\" completed=\"1\" filter=\"all\""), "{page}");

    // A change goes out, and the list is loaded again before the
    // export returns.
    el.handle(id, "add", event("todo-add", Some("Read"), None)).await;
    assert_eq!(el.take_requests(), ["POST /api/todos {\"title\":\"Read\"}", "GET /api/todos?filter=all"]);
    let page = el.render(id).await;
    assert!(page.contains("title=\"Read\" completed=\"false\""), "{page}");
    assert!(page.contains("<todo-footer active=\"2\" completed=\"1\""), "{page}");

    el.handle(id, "toggle", event("todo-toggle", Some("t1"), None)).await;
    assert_eq!(el.take_requests(), ["PATCH /api/todos/t1 {\"completed\":true}", "GET /api/todos?filter=all"]);
    el.handle(id, "edit", event("todo-edit", Some("t3\nRead a book"), None)).await;
    assert_eq!(el.take_requests(), ["PATCH /api/todos/t3 {\"title\":\"Read a book\"}", "GET /api/todos?filter=all"]);
    el.handle(id, "clear", event("todo-clear", Some(""), None)).await;
    assert_eq!(el.take_requests(), ["DELETE /api/todos", "GET /api/todos?filter=all"]);
    let page = el.render(id).await;
    assert!(page.contains("title=\"Read a book\""), "{page}");
    assert!(!page.contains("Buy milk") && !page.contains("Walk"), "{page}");
    assert_eq!(el.store.data().api.todos.len(), 1);

    // The route's filter comes in as an attribute.
    el.set_attr(id, "filter", Some("completed")).await;
    assert_eq!(el.take_requests(), ["GET /api/todos?filter=completed"]);
    let page = el.render(id).await;
    assert!(!page.contains("<todo-item"), "{page}");
    assert!(page.contains("filter=\"completed\""), "{page}");

    // Requests free all they hold: adding a todo and deleting it again,
    // four requests and their responses, leaves memory as it was.
    el.set_attr(id, "filter", Some("all")).await;
    add_and_destroy(&mut el, id, 0).await;
    let before = el.live().await;
    for n in 1..50 {
        add_and_destroy(&mut el, id, n).await;
    }
    assert_eq!(el.take_requests().len(), 1 + 4 * 50);
    assert_eq!(el.live().await, before, "requests leak");
}

const TODO_INPUT: &str = include_str!("../examples/todo-input.cdt");
const TODO_FOOTER: &str = include_str!("../examples/todo-footer.cdt");

#[tokio::test(flavor = "current_thread")]
async fn todo_input_adds_and_clears() {
    let mut el = Element::load(TODO_INPUT).await;
    let id = el.create(&[("has-todos", "false"), ("all-completed", "false")]).await;
    let field = "<input class=\"new-todo\" placeholder=\"What needs to be done?\" autofocus=\"\"";
    assert_eq!(el.render(id).await, format!("<div class=\"row\">{field} @keydown=key></input></div>"));

    // Enter adds the trimmed title and clears the field once.
    el.handle(id, "key", event("keydown", Some("  Buy milk "), Some("Enter"))).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-add".to_string(), "Buy milk".to_string())]);
    assert!(el.render(id).await.contains(".value=\"\""));
    // The next keystroke leaves the field to the person typing.
    el.handle(id, "key", event("keydown", Some(""), Some("a"))).await;
    assert!(!el.render(id).await.contains(".value"));
    // A blank title adds nothing.
    el.handle(id, "key", event("keydown", Some("   "), Some("Enter"))).await;
    assert!(el.take_emitted().is_empty());

    // With todos, the toggle-all checkbox appears and reports its state.
    el.set_attr(id, "has-todos", Some("true")).await;
    el.set_attr(id, "all-completed", Some("true")).await;
    let html = el.render(id).await;
    assert!(
        html.starts_with("<div class=\"row\"><input id=\"toggle-all\" class=\"toggle-all\" type=\"checkbox\""),
        "{html}"
    );
    assert!(html.contains(".checked=true @change=toggleAll"), "{html}");
    el.handle(id, "toggleAll", event_with("change", None, Some(false), None)).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-toggle-all".to_string(), "false".to_string())]);
}

#[tokio::test(flavor = "current_thread")]
async fn todo_footer_counts_filters_and_clears() {
    let mut el = Element::load(TODO_FOOTER).await;
    let id = el.create(&[("active", "1"), ("completed", "0"), ("filter", "active")]).await;
    assert_eq!(
        el.render(id).await,
        "<footer class=\"footer\">\
         <span class=\"todo-count\"><strong>1</strong> item left</span>\
         <ul class=\"filters\">\
         <li><a href=\"#/\" class=\"\">All</a></li>\
         <li><a href=\"#/active\" class=\"selected\">Active</a></li>\
         <li><a href=\"#/completed\" class=\"\">Completed</a></li>\
         </ul>\
         </footer>"
    );
    el.set_attr(id, "active", Some("3")).await;
    el.set_attr(id, "completed", Some("2")).await;
    let html = el.render(id).await;
    assert!(html.contains("<strong>3</strong> items left"), "{html}");
    assert!(html.contains("<button class=\"clear-completed\" @click=clear>Clear completed</button>"), "{html}");
    el.handle(id, "clear", event("click", None, None)).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-clear".to_string(), String::new())]);
}

/// The whole demo in conduct, on the polyfill: `<todo-app>` sends its
/// requests to the demo's routes, written in conduct, each in a store of
/// its own as the demo's service worker keeps it, and the routes call
/// the todo model.
#[tokio::test(flavor = "current_thread")]
async fn todo_app_runs_on_conduct_routes() {
    let model = std::sync::Arc::new(std::sync::Mutex::new(support::TodoApi::default()));
    {
        let mut m = model.lock().unwrap();
        m.add("Buy milk", false);
        m.add("Walk", true);
    }
    let routes = support::routes::Routes::new(model.clone()).await;
    let routes = std::sync::Arc::new(tokio::sync::Mutex::new(routes));
    let host = Host { routes: Some(routes.clone()), ..Host::default() };
    let mut el = Element::load_with(TODO_APP, host).await;
    let id = el.create(&[("filter", "")]).await;

    el.connect(id).await;
    let page = el.render(id).await;
    assert!(page.contains("<todo-item #t1 todo-id=\"t1\" title=\"Buy milk\" completed=\"false\""), "{page}");
    assert!(page.contains("<todo-footer active=\"1\" completed=\"1\" filter=\"all\""), "{page}");

    el.handle(id, "add", event("todo-add", Some("Read"), None)).await;
    el.handle(id, "toggle", event("todo-toggle", Some("t1"), None)).await;
    el.handle(id, "edit", event("todo-edit", Some("t3\nRead a book"), None)).await;
    el.handle(id, "clear", event("todo-clear", Some(""), None)).await;
    assert_eq!(
        el.take_requests(),
        [
            "GET /api/todos?filter=all",
            "POST /api/todos {\"title\":\"Read\"}",
            "GET /api/todos?filter=all",
            "PATCH /api/todos/t1 {\"completed\":true}",
            "GET /api/todos?filter=all",
            "PATCH /api/todos/t3 {\"title\":\"Read a book\"}",
            "GET /api/todos?filter=all",
            "DELETE /api/todos",
            "GET /api/todos?filter=all",
        ]
    );
    let page = el.render(id).await;
    assert!(page.contains("title=\"Read a book\" completed=\"false\""), "{page}");
    assert!(!page.contains("Buy milk") && !page.contains("Walk"), "{page}");
    assert!(page.contains("<todo-footer active=\"1\" completed=\"0\""), "{page}");
    // Every change reached the model through a route.
    let calls = model.lock().unwrap().calls.clone();
    assert_eq!(calls.len(), 9, "{calls:?}");
    assert!(calls[1].starts_with("add [String(\"Read\")]"), "{calls:?}");
    assert!(calls[7].starts_with("clear-completed"), "{calls:?}");

    // Neither the element nor the routes hold more for serving more.
    el.set_attr(id, "filter", Some("completed")).await;
    el.set_attr(id, "filter", Some("all")).await;
    let element_before = el.live().await;
    let routes_before = routes.lock().await.live().await;
    for n in 0..20 {
        el.handle(id, "add", event("todo-add", Some(&format!("Todo {n}")), None)).await;
        let added = model.lock().unwrap().todos.last().unwrap().id.clone();
        el.handle(id, "toggle", event("todo-toggle", Some(&added), None)).await;
        el.handle(id, "destroy", event("todo-destroy", Some(&added), None)).await;
    }
    assert_eq!(el.live().await, element_before, "the element leaks");
    assert_eq!(routes.lock().await.live().await, routes_before, "the routes leak");
    assert_eq!(model.lock().unwrap().todos.len(), 1);
}
