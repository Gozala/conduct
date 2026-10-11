//! `<todo-app>`'s logic, driven without HTTP: a conduct program feeds
//! `update` the events the element would hear, including the responses
//! to its own `fetch` effects, and the test checks the effects it asks
//! for and the views it renders.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode, name_fields};
use wasmtime::{Engine, Instance, Module, Store, Val};

const TODO_APP: &str = include_str!("../examples/todo-app.cdt");

/// The events the element hears, and a response with a JSON body.
const DRIVER: &str = r#"
(def attrs {"filter" ""})
(defn ev [kind value] {:kind kind :value value :checked nil :key nil :status nil})
(defn resp [body] {:kind "response" :value body :checked nil :key nil :status 200})
(def list-body "{\"todos\":[{\"id\":\"a\",\"title\":\"Buy milk\",\"completed\":false},{\"id\":\"b\",\"title\":\"Walk\",\"completed\":true}],\"counts\":{\"active\":1,\"completed\":1}}")

(def connected (update (init attrs) "connected" (ev "connected" nil) attrs))
(def loaded (update (:model connected) "loaded" (resp list-body) attrs))
(def ready (:model loaded))
"#;

/// Compile the app plus the driver, evaluate `entry`, decode it, and
/// check that nothing leaked.
fn eval(extra: &str, entry: &str) -> Value {
    let mut cb = Codebase::new();
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
    let fields = name_fields(result, &["model", "effects"]);
    match fields.into_iter().find(|(n, _)| n == "effects").unwrap().1 {
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
    let with_filter = "(def filtered (update (init {}) \"attribute-changed\" (ev \"attribute-changed\" \"filter\") {\"filter\" \"active\"}))";
    assert_eq!(effects(eval(with_filter, "filtered")), vec![fetch("GET", "/api/todos?filter=active", "", "loaded")]);
    // Another attribute changing loads nothing.
    let other = "(def other (update (init {}) \"attribute-changed\" (ev \"attribute-changed\" \"lang\") {}))";
    assert_eq!(effects(eval(other, "other")), vec![]);
}

#[test]
fn a_loaded_list_renders_items_and_footer() {
    let page = html(&eval("(def page (view ready attrs))", "page"));
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
            "(def x (update ready \"add\" (ev \"todo-add\" \"Read\") attrs))",
            fetch("POST", "/api/todos", r#"{"title":"Read"}"#, "changed"),
        ),
        (
            "(def x (update ready \"toggle\" (ev \"todo-toggle\" \"a\") attrs))",
            fetch("PATCH", "/api/todos/a", r#"{"completed":true}"#, "changed"),
        ),
        (
            "(def x (update ready \"toggle\" (ev \"todo-toggle\" \"b\") attrs))",
            fetch("PATCH", "/api/todos/b", r#"{"completed":false}"#, "changed"),
        ),
        (
            "(def x (update ready \"toggleAll\" (ev \"todo-toggle-all\" \"true\") attrs))",
            fetch("PATCH", "/api/todos", r#"{"completed":true}"#, "changed"),
        ),
        (
            "(def x (update ready \"edit\" (ev \"todo-edit\" \"a\\nOat milk \") attrs))",
            fetch("PATCH", "/api/todos/a", r#"{"title":"Oat milk"}"#, "changed"),
        ),
        (
            "(def x (update ready \"edit\" (ev \"todo-edit\" \"a\\n   \") attrs))",
            fetch("DELETE", "/api/todos/a", "", "changed"),
        ),
        (
            "(def x (update ready \"destroy\" (ev \"todo-destroy\" \"b\") attrs))",
            fetch("DELETE", "/api/todos/b", "", "changed"),
        ),
        (
            "(def x (update ready \"clear\" (ev \"todo-clear\" \"\") attrs))",
            fetch("DELETE", "/api/todos", "", "changed"),
        ),
        ("(def x (update ready \"changed\" (resp \"\") attrs))", fetch("GET", "/api/todos?filter=all", "", "loaded")),
    ];
    for (source, expected) in cases {
        assert_eq!(effects(eval(source, "x")), vec![expected], "{source}");
    }
    // Toggling a todo that is not in the list asks for nothing.
    assert_eq!(effects(eval("(def x (update ready \"toggle\" (ev \"todo-toggle\" \"zz\") attrs))", "x")), vec![]);
}

#[test]
fn the_reveal_moves_through_its_phases() {
    let steps = "(def measured (:model (update ready \"measured\" (ev \"resize\" \"320\") attrs)))
                 (def revealed (:model (update measured \"revealed\" (ev \"transitionend\" nil) attrs)))
                 (def revealing-page (view measured attrs))
                 (def shown-page (view revealed attrs))";
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
    let about = "(def open (view (:model (update ready \"about\" (ev \"click\" nil) attrs)) attrs))";
    assert!(html(&eval(about, "open")).contains("<div id=\"about\" class=\"about\">"));
}
