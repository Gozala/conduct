//! `conduct.syntax/inspect` writes any value as conduct writes it, so it
//! reads back; `show` writes it for people, by its type: a type's own
//! `show` if it has one, else its shape, with `show` again for each part
//! the type says it has. A secret is `<secret>` to both.

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
fn inspect_writes_values_as_they_are_written() {
    let out = texts(
        "(type title : (demo.todo/title text))
         (type done : (demo.todo/done boolean))
         (type pin : (demo.todo/pin (secret natural)))
         (type (thing ?a) : {(demo.thing/thing ?a)})
         (define out [(inspect \"milk\")
                      (inspect -7)
                      (inspect [\"a\" \"b \\\"c\\\"\"])
                      (inspect {title: \"milk\", done: false})
                      (inspect [{done: true, title: \"a\"}])
                      (inspect (thing (thing 5)))
                      (inspect {:class \"toggle\" :type \"checkbox\"})
                      (inspect :ready)
                      (inspect (fn [x] x))
                      (syntax/inspect (secret \"hunter2\"))
                      (conduct.syntax/inspect {title: \"a\", pin: (secret 1)})])",
    );
    assert_eq!(
        out,
        [
            r#""milk""#,
            "-7",
            r#"["a" "b \"c\""]"#,
            r#"{done: false, title: "milk"}"#,
            r#"[{done: true, title: "a"}]"#,
            "{thing: {thing: 5}}",
            r#"{:class "toggle" :type "checkbox"}"#,
            ":ready",
            "<function>",
            "<secret>",
            r#"{pin: <secret>, title: "a"}"#,
        ]
    );
}

#[test]
fn join_without_a_separator_puts_texts_together() {
    let out = texts(r#"(define out [(join ["a" "b" "c"]) (join ["a" "b"] with: ", ") (join [])])"#);
    assert_eq!(out, ["abc", "a, b", ""]);
}

#[test]
fn show_goes_by_the_type() {
    let out = texts(
        "(type login : (demo.user/login text))
         (type token : (demo.user/token (secret text)))
         (type (thing ?a) : {(demo.thing/thing ?a)})
         (type (money ?a) : {(demo.money/money ?a)})
         (domain demo.money (define (show (money n)) (join [(show n) \" USD\"])))
         (define out [(show \"milk\")
                      (show 42)
                      (show [1 2])
                      (show [\"a\" \"b\"])
                      (show (money 5))
                      (show [(money 5) (money 7)])
                      (show (thing (money 3)))
                      (show {login: \"ada\", token: (secret \"hunter2\")})
                      (show (secret [1 2]))
                      (show :ready)])",
    );
    assert_eq!(
        out,
        [
            "milk",
            "42",
            "[1 2]",
            "[a b]",
            "5 USD",
            "[5 USD 7 USD]",
            "(thing 3 USD)",
            "{login: ada, token: <secret>}",
            "<secret>",
            ":ready",
        ]
    );
}

#[test]
fn show_shows_what_the_type_selects() {
    let out = texts(
        "(type login : (demo.user/login text))
         (type token : (demo.user/token text))
         (claim (describe {login: text}) : text)
         (define (describe u) (show u))
         (define (generic u) (show u))
         (define user {login: \"ada\", token: \"t0k3n\"})
         (define out [(describe user) (generic user) (inspect user)])",
    );
    assert_eq!(out, ["{login: ada}", "{login: ada, token: t0k3n}", r#"{login: "ada", token: "t0k3n"}"#]);
}

#[test]
fn a_generic_function_shows_by_its_caller_s_type() {
    let out = texts(
        "(type (money ?a) : {(demo.money/money ?a)})
         (domain demo.money (define (show (money n)) (join [(show n) \" USD\"])))
         (define (label x) (join [\"got \" (show x)]))
         (define (labels xs) (map xs each: label))
         (define out [(label (money 1)) (label [(money 2)]) (label 3) (join (labels [(money 4)]))])",
    );
    assert_eq!(out, ["got 1 USD", "got [2 USD]", "got 3", "got 4 USD"]);
}

#[test]
fn a_union_of_variants_shows_each_by_its_own() {
    let out = texts(
        "(type (money ?a) : {(demo.money/money ?a)})
         (type (thing ?a) : {(demo.thing/thing ?a)})
         (domain demo.money (define (show (money n)) (join [(show n) \" USD\"])))
         (define (pick n) (if (= n 0) (money 1) (thing 2)))
         (define out [(show [(pick 0) (pick 1)]) (show (ok (money 3)))])",
    );
    assert_eq!(out, ["[1 USD (thing 2)]", "(ok 3 USD)"]);
}

#[test]
fn same_is_equality_unless_a_type_says_otherwise() {
    let out = texts(
        "(type (money ?a) : {(demo.money/money ?a)})
         (type amount : (demo.order/amount (money integer)))
         (type note : (demo.order/note text))
         (domain demo.money
           (define (same? (money a) as: (money b)) (= (if (< a 0) (- 0 a) a) (if (< b 0) (- 0 b) b))))
         (define (yes b) (if b \"yes\" \"no\"))
         (define out [(yes (same? 1 as: 1))
                      (yes (same? (money 3) as: (money -3)))
                      (yes (same? [(money 1) (money 2)] as: [(money -1) (money -2)]))
                      (yes (same? [(money 1)] as: [(money 1) (money 1)]))
                      (yes (same? {amount: (money 5), note: \"a\"} as: {amount: (money -5), note: \"a\"}))
                      (yes (same? {amount: (money 5), note: \"a\"} as: {amount: (money 5), note: \"b\"}))
                      (yes (= (money 3) (money -3)))])",
    );
    assert_eq!(out, ["yes", "yes", "yes", "no", "yes", "no", "no"]);
}

#[test]
fn a_secret_opens_only_by_reveal() {
    let out = texts(
        "(define s (secret \"hunter2\"))
         (define out [(reveal s) (show s) (inspect (= s (secret \"hunter2\")))])",
    );
    assert_eq!(out, ["hunter2", "<secret>", "true"]);
    let mut cb = Codebase::new();
    let e = cb.add_source("(define (peek (secret x)) x)").unwrap_err().to_string();
    assert!(e.contains("opaque"), "{e}");
    let e = cb.add_source("(define (peek s) (match s {conduct.secret/secret: x} x))").unwrap_err().to_string();
    assert!(e.contains("opaque"), "{e}");
}

#[test]
fn show_through_claims_optional_attributes_updates_and_recursive_types() {
    let out = texts(
        "(type login : (demo.user/login text))
         (type token : (demo.user/token (secret text)))
         (type (leaf ?a) : {(demo.tree/leaf ?a)})
         (type (node ?a) : {(demo.tree/node ?a)})
         (type (tree ?a) : [(leaf ?a) (node (list (tree ?a)))])
         (type (money ?a) : {(demo.money/money ?a)})
         (domain demo.money (define (show (money n)) (join [(show n) \" USD\"])))
         (claim (label ?a) : text)
         (define (label x) (join [\"got \" (show x)]))
         (define (pick c) (if c {login: \"ada\", token: (secret \"x\")} {login: \"bob\"}))
         (define (rename u) (assoc u login: \"eve\"))
         (define out [(label 1)
                      (label (money 2))
                      (show (pick true))
                      (show (pick false))
                      (show (rename (pick false)))
                      (show (node [(leaf 1) (node [(leaf 2)])]))])",
    );
    assert_eq!(
        out,
        [
            "got 1",
            "got 2 USD",
            "{login: ada, token: <secret>}",
            "{login: bob}",
            "{login: eve}",
            "(node [(leaf 1) (node [(leaf 2)])])",
        ]
    );
}
