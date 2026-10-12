//! `<todo-app>`'s logic, driven without HTTP: a conduct program feeds
//! `update` the events the element would hear, including the responses
//! to its own `fetch` effects, and the test checks the effects it asks
//! for and the views it renders.

use conduct::codegen::{self, Value, decode, name_fields};
use conduct::element;
use wasmtime::{Engine, Instance, Module, Store, Val};

const TODO_APP: &str = include_str!("../examples/todo-app.cdt");

/// The events the element hears, and a response with a JSON body.
const DRIVER: &str = r#"
(define attrs {"filter" ""})
(define (ev kind value) {kind: kind, value: value})
(define (resp body) {kind: "response", value: body, status: 200})
(define list-body "{\"todos\":[{\"id\":\"a\",\"title\":\"Buy milk\",\"completed\":false},{\"id\":\"b\",\"title\":\"Walk\",\"completed\":true}],\"counts\":{\"active\":1,\"completed\":1}}")

(define connected (update (init attrs) handler: "connected" event: {kind: "connected"} attrs: attrs))
(define loaded (update (model connected) handler: "loaded" event: (resp list-body) attrs: attrs))
(define ready (model loaded))
"#;

/// Compile the app plus the driver, evaluate `entry`, decode it, and
/// check that nothing leaked.
fn eval(extra: &str, entry: &str) -> Value {
    let mut cb = element::codebase();
    cb.add_source(TODO_APP).unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(DRIVER).unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(extra).unwrap_or_else(|e| panic!("{e}"));
    let hash = cb.names[entry];
    let wat = codegen::compile_exports(&cb, &[(entry, hash)]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_func(&mut store, entry).unwrap();
    let mut results = [Val::I64(0)];
    f.call(&mut store, &[], &mut results).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let value = decode(memory.data(&store), results[0].unwrap_i64());
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, results[0].unwrap_i64()).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "{entry} leaked");
    value
}

fn fetch(method: &str, url: &str, body: &str, handler: &str) -> Value {
    Value::Fetch { method: method.into(), url: url.into(), body: body.into(), handler: handler.into() }
}

/// The effects of an `update` result.
fn effects(result: Value) -> Vec<Value> {
    let fields = name_fields(result, &["conduct.element/model", "conduct.element/effects"]);
    match fields.into_iter().find(|(n, _)| n == "conduct.element/effects").unwrap().1 {
        Value::List(items) => items,
        other => panic!("expected a list, found {other:?}"),
    }
}

/// A view as HTML-like text, as in the element tests.
fn html(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Nil => String::new(),
        Value::List(items) => items.iter().map(html).collect(),
        Value::View { tag, attrs, children, events } => {
            let mut out = format!("<{tag}");
            for (k, v) in attrs {
                if let Value::Str(s) = v {
                    out.push_str(&format!(" {k}=\"{s}\""));
                }
            }
            for (e, h) in events {
                if let Value::Str(s) = h {
                    out.push_str(&format!(" @{e}={s}"));
                }
            }
            out.push('>');
            out.push_str(&children.iter().map(html).collect::<String>());
            out.push_str(&format!("</{tag}>"));
            out
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn connecting_loads_the_list() {
    assert_eq!(effects(eval("", "connected")), vec![fetch("GET", "/api/todos?filter=all", "", "loaded")]);
    let with_filter = "(define filtered (update (init {}) handler: \"attribute-changed\" event: (ev \"attribute-changed\" value: \"filter\") attrs: {\"filter\" \"active\"}))";
    assert_eq!(effects(eval(with_filter, "filtered")), vec![fetch("GET", "/api/todos?filter=active", "", "loaded")]);
    // Another attribute changing loads nothing.
    let other = "(define other (update (init {}) handler: \"attribute-changed\" event: (ev \"attribute-changed\" value: \"lang\") attrs: {}))";
    assert_eq!(effects(eval(other, "other")), vec![]);
}

#[test]
fn a_loaded_list_renders_items_and_footer() {
    let page = html(&eval("(define page (view ready attrs: attrs))", "page"));
    assert!(page.starts_with("<div class=\"frame\" data-phase=\"measuring\">"), "{page}");
    assert!(page.contains("<todo-input has-todos=\"true\" all-completed=\"false\""), "{page}");
    assert!(
        page.contains("<todo-item key=\"a\" todo-id=\"a\" title=\"Buy milk\" completed=\"false\" @todo-toggle=toggle"),
        "{page}"
    );
    assert!(page.contains("<todo-item key=\"b\" todo-id=\"b\" title=\"Walk\" completed=\"true\""), "{page}");
    assert!(page.contains("<todo-footer active=\"1\" completed=\"1\" filter=\"all\" @todo-clear=clear>"), "{page}");
}

#[test]
fn changes_go_out_and_reload() {
    let cases = [
        (
            "(define x (update ready handler: \"add\" event: (ev \"todo-add\" value: \"Read\") attrs: attrs))",
            fetch("POST", "/api/todos", r#"{"title":"Read"}"#, "changed"),
        ),
        (
            "(define x (update ready handler: \"toggle\" event: (ev \"todo-toggle\" value: \"a\") attrs: attrs))",
            fetch("PATCH", "/api/todos/a", r#"{"completed":true}"#, "changed"),
        ),
        (
            "(define x (update ready handler: \"toggle\" event: (ev \"todo-toggle\" value: \"b\") attrs: attrs))",
            fetch("PATCH", "/api/todos/b", r#"{"completed":false}"#, "changed"),
        ),
        (
            "(define x (update ready handler: \"toggleAll\" event: (ev \"todo-toggle-all\" value: \"true\") attrs: attrs))",
            fetch("PATCH", "/api/todos", r#"{"completed":true}"#, "changed"),
        ),
        (
            "(define x (update ready handler: \"edit\" event: (ev \"todo-edit\" value: \"a\\nOat milk \") attrs: attrs))",
            fetch("PATCH", "/api/todos/a", r#"{"title":"Oat milk"}"#, "changed"),
        ),
        (
            "(define x (update ready handler: \"edit\" event: (ev \"todo-edit\" value: \"a\\n   \") attrs: attrs))",
            fetch("DELETE", "/api/todos/a", "", "changed"),
        ),
        (
            "(define x (update ready handler: \"destroy\" event: (ev \"todo-destroy\" value: \"b\") attrs: attrs))",
            fetch("DELETE", "/api/todos/b", "", "changed"),
        ),
        (
            "(define x (update ready handler: \"clear\" event: (ev \"todo-clear\" value: \"\") attrs: attrs))",
            fetch("DELETE", "/api/todos", "", "changed"),
        ),
        (
            "(define x (update ready handler: \"changed\" event: (resp \"\") attrs: attrs))",
            fetch("GET", "/api/todos?filter=all", "", "loaded"),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(effects(eval(source, "x")), vec![expected], "{source}");
    }
    // Toggling a todo that is not in the list asks for nothing.
    assert_eq!(
        effects(eval(
            "(define x (update ready handler: \"toggle\" event: (ev \"todo-toggle\" value: \"zz\") attrs: attrs))",
            "x"
        )),
        vec![]
    );
}

#[test]
fn the_reveal_moves_through_its_phases() {
    let steps = "(define measured (model (update ready handler: \"measured\" event: (ev \"resize\" value: \"320\") attrs: attrs)))
                 (define revealed (model (update measured handler: \"revealed\" event: {kind: \"transitionend\"} attrs: attrs)))
                 (define revealing-page (view measured attrs: attrs))
                 (define shown-page (view revealed attrs: attrs))";
    let revealing = html(&eval(steps, "revealing-page"));
    assert!(
        revealing.starts_with("<div class=\"frame\" data-phase=\"revealing\" style=\"height: 320px\">"),
        "{revealing}"
    );
    let shown = html(&eval(steps, "shown-page"));
    assert!(
        shown.starts_with("<div class=\"frame\" data-phase=\"shown\"><div class=\"content\" key=\"content\">"),
        "{shown}"
    );
    assert!(!shown.contains("spinner"), "{shown}");
    // The about pill opens the words about the demo.
    let about = "(define open (view (model (update ready handler: \"about\" event: {kind: \"click\"} attrs: attrs)) attrs: attrs))";
    assert!(html(&eval(about, "open")).contains("<div id=\"about\" class=\"about\">"));
}
