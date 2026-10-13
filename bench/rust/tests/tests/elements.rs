//! The Rust elements, built as components in `../out` by `../build.sh`,
//! driven through the Wasm Component Model Polyfill as the demo's page
//! drives an element. The harness and the expected renders are those of
//! the repository's tests/element.rs, so the Rust elements render and
//! emit what the conduct elements do. `ELEMENTS_DIR` points the tests at
//! other builds of the same elements, such as `../out/opt-z`.

mod support;

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

/// The bytes of the component `{name}.wasm` in `$ELEMENTS_DIR`, or in
/// `../out` where build.sh puts the opt-level "s" builds.
fn component(name: &str) -> Vec<u8> {
    let dir = std::env::var("ELEMENTS_DIR").unwrap_or_else(|_| format!("{}/../out", env!("CARGO_MANIFEST_DIR")));
    let path = format!("{dir}/{name}.wasm");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} (run build.sh first)"))
}

impl Element {
    async fn load(name: &str) -> Element {
        Element::load_with(name, Host::default()).await
    }

    async fn load_with(name: &str, host: Host) -> Element {
        let bytes = component(name);
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

    async fn styles(&mut self) -> String {
        match &self.call("styles", &[]).await[..] {
            [Val::String(s)] => s.clone(),
            other => panic!("styles returned {other:?}"),
        }
    }

    async fn observed(&mut self) -> Vec<String> {
        match &self.call("observed-attributes", &[]).await[..] {
            [Val::List(names)] => names.iter().map(string).collect(),
            other => panic!("observed-attributes returned {other:?}"),
        }
    }

    fn take_emitted(&mut self) -> Emitted {
        std::mem::take(&mut self.store.data_mut().emitted)
    }

    fn take_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.data_mut().requests)
    }
}

/// The `styles` string of the conduct source `examples/{name}.cdt`, in
/// this repository.
fn conduct_styles(name: &str) -> String {
    let path = format!("{}/../../../examples/{name}.cdt", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let start = source.find("(define styles \"").expect("a styles definition") + "(define styles \"".len();
    let mut out = String::new();
    let mut chars = source[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(match chars.next().expect("an escaped character") {
                'n' => '\n',
                't' => '\t',
                other => other,
            }),
            '"' => return out,
            c => out.push(c),
        }
    }
    panic!("{path}: the styles string does not end")
}

async fn check_constants(name: &str, attributes: &[&str]) {
    let mut el = Element::load(name).await;
    assert_eq!(el.observed().await, attributes);
    let styles = el.styles().await;
    assert!(styles.contains(":host"));
    assert_eq!(styles, conduct_styles(name), "{name}'s styles differ from conduct's");
}

#[tokio::test(flavor = "current_thread")]
async fn constants_match_conduct() {
    check_constants("todo-item", &["todo-id", "title", "completed"]).await;
    check_constants("todo-input", &["has-todos", "all-completed"]).await;
    check_constants("todo-footer", &["active", "completed", "filter"]).await;
    check_constants("todo-app", &["filter"]).await;
}

#[tokio::test(flavor = "current_thread")]
async fn todo_item_renders_and_reports_changes() {
    let mut el = Element::load("todo-item").await;
    let id = el.create(&[("todo-id", "7"), ("title", "Buy milk"), ("completed", "false")]).await;
    assert_eq!(id, 1);
    assert_eq!(
        el.render(id).await,
        "<li class=\"\">\
         <input #toggle aria-label=\"Toggle the todo\" class=\"toggle\" type=\"checkbox\" .checked=false @change=toggle></input>\
         <label @dblclick=edit>Buy milk</label>\
         <button aria-label=\"Delete the todo\" class=\"destroy\" @click=destroy>×</button>\
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
         <input #edit aria-label=\"Edit the todo\" autofocus=\"\" class=\"edit\" .value=\"Buy milk\" @blur=save @keydown=keydown></input>\
         </li>"
    );
    el.handle(id, "keydown", event("keydown", Some("Buy oat milk"), Some("Enter"))).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-edit".to_string(), "7\nBuy oat milk".to_string())]);
    assert!(el.render(id).await.starts_with("<li class=\"completed\">"));

    // Escape cancels an edit without reporting anything.
    el.handle(id, "edit", event("dblclick", None, None)).await;
    el.handle(id, "keydown", event("keydown", Some("ignored"), Some("Escape"))).await;
    assert!(el.take_emitted().is_empty());
    assert!(!el.render(id).await.contains("editing"));

    // Leaving the field saves; a blur when not editing does nothing.
    el.handle(id, "edit", event("dblclick", None, None)).await;
    el.handle(id, "save", event("blur", Some("Buy milk"), None)).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-edit".to_string(), "7\nBuy milk".to_string())]);
    el.handle(id, "save", event("blur", Some("Buy milk"), None)).await;
    assert!(el.take_emitted().is_empty());

    // A second element keeps its own state.
    let other = el.create(&[("todo-id", "8"), ("title", "Walk")]).await;
    el.handle(other, "edit", event("dblclick", None, None)).await;
    assert!(el.render(other).await.contains("editing"));
    assert!(!el.render(id).await.contains("editing"));
    el.handle(other, "destroy", event("click", None, None)).await;
    assert_eq!(el.take_emitted(), vec![(other, "todo-destroy".to_string(), "8".to_string())]);

    // A disconnected element is gone, and its id is not reused.
    el.call("disconnected", &[Val::U32(other)]).await;
    assert_eq!(el.create(&[("todo-id", "9"), ("title", "Read")]).await, 3);
}

#[tokio::test(flavor = "current_thread")]
async fn todo_input_adds_and_clears() {
    let mut el = Element::load("todo-input").await;
    let id = el.create(&[("has-todos", "false"), ("all-completed", "false")]).await;
    let field = "<input #new-todo autofocus=\"\" class=\"new-todo\" placeholder=\"What needs to be done?\"";
    assert_eq!(el.render(id).await, format!("<div class=\"row\">{field} @keydown=keydown></input></div>"));

    // Enter adds the trimmed title and clears the field once.
    el.handle(id, "keydown", event("keydown", Some("  Buy milk "), Some("Enter"))).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-add".to_string(), "Buy milk".to_string())]);
    assert_eq!(
        el.render(id).await,
        format!("<div class=\"row\">{field} .value=\"\" @keydown=keydown></input></div>")
    );
    // The next keystroke leaves the field to the person typing.
    el.handle(id, "keydown", event("keydown", Some(""), Some("a"))).await;
    assert!(!el.render(id).await.contains(".value"));
    // A blank title adds nothing.
    el.handle(id, "keydown", event("keydown", Some("   "), Some("Enter"))).await;
    assert!(el.take_emitted().is_empty());
    // An update other than an add, such as the attribute <todo-app> sets
    // after it, does not clear the field again.
    el.handle(id, "keydown", event("keydown", Some("Walk"), Some("Enter"))).await;
    assert!(el.render(id).await.contains(".value=\"\""));
    el.set_attr(id, "has-todos", Some("true")).await;
    assert!(!el.render(id).await.contains(".value"));
    el.set_attr(id, "has-todos", Some("false")).await;
    el.take_emitted();

    // With todos, the toggle-all checkbox appears and reports its state.
    el.set_attr(id, "has-todos", Some("true")).await;
    el.set_attr(id, "all-completed", Some("true")).await;
    let html = el.render(id).await;
    assert_eq!(
        html,
        format!(
            "<div class=\"row\">\
             <input #toggle-all aria-label=\"Mark all as complete\" class=\"toggle-all\" id=\"toggle-all\" type=\"checkbox\" .checked=true @change=toggle-all></input>\
             {field} @keydown=keydown></input></div>"
        )
    );
    el.handle(id, "toggle-all", event_with("change", None, Some(false), None)).await;
    assert_eq!(el.take_emitted(), vec![(id, "todo-toggle-all".to_string(), "false".to_string())]);
}

#[tokio::test(flavor = "current_thread")]
async fn todo_footer_counts_filters_and_clears() {
    let mut el = Element::load("todo-footer").await;
    let id = el.create(&[("active", "1"), ("completed", "0"), ("filter", "active")]).await;
    assert_eq!(
        el.render(id).await,
        "<footer class=\"footer\">\
         <span class=\"todo-count\"><strong>1</strong> item left</span>\
         <ul class=\"filters\">\
         <li><a class=\"\" href=\"#/\">All</a></li>\
         <li><a class=\"selected\" href=\"#/active\">Active</a></li>\
         <li><a class=\"\" href=\"#/completed\">Completed</a></li>\
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

/// `<todo-app>` against an in-process copy of the demo's API: each
/// export that handles an event finishes after the requests it caused
/// have been answered, so a render after it shows the result.
#[tokio::test(flavor = "current_thread")]
async fn todo_app_talks_to_the_api() {
    let mut host = Host::default();
    host.api.add("Buy milk", false);
    host.api.add("Walk", true);
    let mut el = Element::load_with("todo-app", host).await;
    let id = el.create(&[("filter", "")]).await;
    assert!(el.render(id).await.contains("spinner"));

    el.connect(id).await;
    assert_eq!(el.take_requests(), ["GET /api/todos?filter=all"]);
    let page = el.render(id).await;
    assert!(page.contains("<todo-item #t1 completed=\"false\" title=\"Buy milk\" todo-id=\"t1\""), "{page}");
    assert!(page.contains("<todo-item #t2 completed=\"true\" title=\"Walk\" todo-id=\"t2\""), "{page}");
    assert!(page.contains("<todo-footer active=\"1\" completed=\"1\" filter=\"all\""), "{page}");

    // A change goes out, and the list is loaded again before the
    // export returns.
    el.handle(id, "add", event("todo-add", Some("Read"), None)).await;
    assert_eq!(el.take_requests(), ["POST /api/todos {\"title\":\"Read\"}", "GET /api/todos?filter=all"]);
    let page = el.render(id).await;
    assert!(page.contains("<todo-item #t3 completed=\"false\" title=\"Read\""), "{page}");
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

    // Toggle-all, a destroy, and an edit to an empty title.
    el.set_attr(id, "filter", Some("all")).await;
    el.handle(id, "add", event("todo-add", Some("Cook"), None)).await;
    el.handle(id, "toggle-all", event("todo-toggle-all", Some("true"), None)).await;
    el.handle(id, "destroy", event("todo-destroy", Some("t3"), None)).await;
    el.handle(id, "edit", event("todo-edit", Some("t4\n  "), None)).await;
    assert_eq!(
        el.take_requests(),
        [
            "GET /api/todos?filter=all",
            "POST /api/todos {\"title\":\"Cook\"}",
            "GET /api/todos?filter=all",
            "PATCH /api/todos {\"completed\":true}",
            "GET /api/todos?filter=all",
            "DELETE /api/todos/t3",
            "GET /api/todos?filter=all",
            "DELETE /api/todos/t4",
            "GET /api/todos?filter=all",
        ]
    );
    assert!(el.store.data().api.todos.is_empty());

    // A toggle of no todo it shows sends nothing.
    el.handle(id, "toggle", event("todo-toggle", Some("t9"), None)).await;
    assert!(el.take_requests().is_empty());

    // Many requests in a row.
    for n in 0..50 {
        el.handle(id, "add", event("todo-add", Some(&format!("Todo {n}")), None)).await;
        let added = el.store.data().api.todos.last().unwrap().id.clone();
        el.handle(id, "destroy", event("todo-destroy", Some(&added), None)).await;
    }
    assert_eq!(el.take_requests().len(), 4 * 50);
}

/// The reveal: the list waits out of sight, the frame moves to its
/// height, and then the list stands on its own. The about pill opens and
/// closes the words about the demo.
#[tokio::test(flavor = "current_thread")]
async fn todo_app_reveals_the_list() {
    let mut el = Element::load_with("todo-app", Host::default()).await;
    let id = el.create(&[]).await;
    el.connect(id).await;
    let empty = "<section class=\"todoapp\"><header class=\"header\"><h1>todos</h1>\
                 <todo-input all-completed=\"false\" has-todos=\"false\" @todo-add=add @todo-toggle-all=toggle-all></todo-input>\
                 </header></section>";
    let info = "<div class=\"info\"><button aria-controls=\"about\" aria-expanded=\"false\" class=\"about-toggle\" type=\"button\" @click=about>\
                <span aria-hidden=\"true\" class=\"icon\">i</span>About this demo</button></div>";
    assert_eq!(
        el.render(id).await,
        format!(
            "<div class=\"frame\" data-phase=\"measuring\">\
             <div #spinner class=\"spinner\"><slot></slot></div>\
             <div #content class=\"content\" @resize=measured @transitionend=revealed>{empty}{info}</div></div>"
        )
    );
    el.handle(id, "measured", event("resize", Some("312"), None)).await;
    assert!(el.render(id).await.starts_with("<div class=\"frame\" data-phase=\"revealing\" style=\"height: 312px\">"));
    el.handle(id, "revealed", event("transitionend", None, None)).await;
    assert_eq!(
        el.render(id).await,
        format!("<div class=\"frame\" data-phase=\"shown\"><div #content class=\"content\">{empty}{info}</div></div>")
    );
    el.handle(id, "about", event("click", None, None)).await;
    let page = el.render(id).await;
    assert!(page.contains("aria-expanded=\"true\""), "{page}");
    assert!(page.contains("<div class=\"about\" id=\"about\"><p>This app demonstrates"), "{page}");
    assert!(
        page.contains("<a href=\"https://github.com/Gozala/conduct\" rel=\"noopener\" target=\"_blank\">conduct</a>"),
        "{page}"
    );
    el.handle(id, "about", event("click", None, None)).await;
    assert!(!el.render(id).await.contains("class=\"about\""));
}
