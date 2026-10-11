//! `<todo-item>`, written in conduct, compiled to a component of the
//! demo's element world, and driven through the Wasm Component Model
//! Polyfill as the demo's page drives an element.

use conduct::{Codebase, element};
use wcmp::*;

type Emitted = Vec<(u32, String, String)>;

struct Element {
    store: Store<Emitted>,
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
        let mut cb = Codebase::new();
        cb.add_source(source).unwrap_or_else(|e| panic!("{e}"));
        let bytes = element::component(&cb).unwrap_or_else(|e| panic!("{e}"));
        let backend = wcmp_wasm_core_wasmtime::Wasmtime::new().unwrap();
        let engine = Engine::with_backend(backend).unwrap();
        let mut store = Store::new(&engine, Emitted::new()).unwrap();
        let component = Component::new(&engine, &bytes).await.unwrap();
        let mut linker: Linker<Emitted> = Linker::new(&engine);
        let host: InterfaceIdentifier = "demo:element/host".parse().unwrap();
        linker
            .instance(&host)
            .func_wrap(
                "emit",
                |mut call: HostCall<'_, Emitted>, (id, name, detail): (u32, String, String)| -> wcmp::Result<()> {
                    call.data_mut().push((id, name, detail));
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

    async fn handle(&mut self, id: u32, handler: &str, event: Val) {
        self.call("handle-event", &[Val::U32(id), Val::String(handler.into()), event]).await;
    }

    async fn set_attr(&mut self, id: u32, name: &str, value: Option<&str>) {
        let value = Val::Option(value.map(|v| Box::new(Val::String(v.into()))));
        self.call("attribute-changed", &[Val::U32(id), Val::String(name.into()), value]).await;
    }

    fn take_emitted(&mut self) -> Emitted {
        std::mem::take(self.store.data_mut())
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
    let mut cb = Codebase::new();
    // `view` must return a View.
    cb.add_source("(defn init [a] 0) (defn view [m a] \"not a view\")").unwrap();
    let err = element::check(&cb).err().unwrap().to_string();
    assert!(err.contains("`view` does not fit"), "{err}");

    // `update` must return the model `view` accepts.
    let mut cb = Codebase::new();
    cb.add_source(
        "(defn init [a] {:n 0})
         (defn update [m handler e a] {:model 1 :emit []})
         (defn view [m a] (h \"p\" {} [(:n m)]))",
    )
    .unwrap();
    let err = element::check(&cb).err().unwrap().to_string();
    assert!(err.contains("does not fit"), "{err}");
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
