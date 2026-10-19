//! `str`: text as it is, and any other value as conduct writes it, with
//! text inside it quoted, so that what is shown reads back.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn texts(src: &str) -> Vec<String> {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let wat = codegen::compile_exports(&cb, &[("out", cb.names["out"])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, "out").unwrap();
    let out = f.call(&mut store, ()).unwrap_or_else(|e| panic!("{e:?}"));
    let value = decode(instance.get_memory(&mut store, "memory").unwrap().data(&store), out);
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, out).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "leaked");
    let Value::List(items) = value else { panic!("{value:?}") };
    items.into_iter().map(|v| if let Value::Str(s) = v { s } else { panic!("{v:?}") }).collect()
}

#[test]
fn values_show_as_they_are_written() {
    let out = texts(
        "(type title : (demo.todo/title text))
         (type done : (demo.todo/done boolean))
         (type (thing ?a) : {(demo.thing/thing ?a)})
         (define out [(str \"milk\")
                      (str 42 \" \" -7)
                      (str [\"a\" \"b \\\"c\\\"\"])
                      (str {title: \"milk\", done: false})
                      (str [{done: true, title: \"a\"}])
                      (str (thing (thing 5)))
                      (str {:class \"toggle\" :type \"checkbox\"})
                      (str :ready [])
                      (str (fn [x] x))])",
    );
    assert_eq!(
        out,
        [
            "milk",
            "42 -7",
            r#"["a" "b \"c\""]"#,
            r#"{done: false, title: "milk"}"#,
            r#"[{done: true, title: "a"}]"#,
            "{thing: {thing: 5}}",
            r#"{:class "toggle" :type "checkbox"}"#,
            ":ready[]",
            "<function>",
        ]
    );
}

#[test]
fn a_record_s_rest_shows_without_what_was_taken() {
    let out = texts(
        "(type login : (demo.user/login text))
         (type password : (demo.user/password text))
         (define (public {password: _ rest ...}) rest)
         (define user {login: \"ada\", password: \"hunter2\"})
         (define out [(str user) (str (public user))])",
    );
    assert_eq!(out, [r#"{login: "ada", password: "hunter2"}"#, r#"{login: "ada"}"#]);
}
