//! Dictionaries: keyword keys, kept sorted; and `get`, which reads an
//! attribute of a record or a key of a dictionary.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn check(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

fn run(src: &str, entry: &str) -> Value {
    let cb = check(src);
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
    let wasm = wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let out = f.call(&mut store, ()).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    decode(memory.data(&store), out)
}

fn keys(v: Value) -> Vec<String> {
    let Value::Dict(entries) = v else { panic!("not a dictionary: {v:?}") };
    entries.into_iter().map(|(k, _)| k).collect()
}

#[test]
fn keys_are_keywords_in_one_order() {
    let cb = check("(define a {:b 1 :a 2}) (define b {:a 2 :b 1})");
    // However a dictionary is written, it is the same value.
    assert_eq!(cb.lookup("a").unwrap().hash, cb.lookup("b").unwrap().hash);
    assert_eq!(keys(run("(define d {:c 1 :a 2 :b 3})", "d")), ["a", "b", "c"]);
    // However it is built.
    assert_eq!(keys(run("(define d (assoc (assoc (assoc {} :c 1) :a 2) :b 3))", "d")), ["a", "b", "c"]);
    assert_eq!(keys(run("(define d (dissoc (assoc {:z 0 :a 1} :m 2) :a))", "d")), ["m", "z"]);
    assert_eq!(
        run("(define j (to-json (assoc {:title \"x\"} :completed true)))", "j"),
        Value::Str(r#"{"completed":true,"title":"x"}"#.into())
    );
    assert!(error("(define d {\"a\" 1})").contains("write `:a`, not `\"a\"`"));
    assert!(error("(define d {:a 1 :a 2})").contains("the key `:a` appears twice"));
}

#[test]
fn get_reads_a_key_with_a_default() {
    let cb = check(
        "(define (lookup d) (get :k of: d or: 0))
         (define (by k d) (get k of: d or: \"\"))",
    );
    assert_eq!(cb.type_of("lookup").unwrap(), "(Dict[a]) -> a | Int");
    assert_eq!(cb.type_of("by").unwrap(), "(Keyword, d: Dict[a]) -> a | String");
    assert_eq!(run("(define v (get :b of: {:a 1 :b 2} or: 0))", "v"), Value::Int(2));
    assert_eq!(run("(define v (get :c of: {:a 1 :b 2} or: 0))", "v"), Value::Int(0));
    let e = error("(define (f d) (get :k of: d))");
    assert!(e.contains("a dictionary may lack any key, so reading one needs `or:`"), "{e}");
    let e = error("(define v (get \"k\" of: {:k 1} or: 0))");
    assert!(e.contains("String is not a Keyword"), "{e}");
    let e = error("(define v (get (ok 1) of: {:k 1} or: 0))");
    assert!(e.contains("is not a Keyword"), "{e}");
    assert!(error("(define (f d) (get of: d or: 1))").contains("the key goes first"));
    assert!(error("(define (f d) (get :k in: d or: 1))").contains("it has no `in:`"));
}

#[test]
fn get_reads_an_attribute() {
    let src = "(type x : (io.gozala.point/x integer))
               (type y : (io.gozala.point/y integer))";
    let cb = check(&format!(
        "{src}
         (define (horizontal p) (get x of: p))
         (define (vertical p) (get y of: p or: 0))
         (define a (vertical {{x: 1}}))
         (define b (vertical {{x: 1, y: 2}}))"
    ));
    assert_eq!(cb.type_of("horizontal").unwrap(), "({x: a}) -> a");
    assert_eq!(cb.type_of("vertical").unwrap(), "({y?: a}) -> a | Int");
    // The same as calling the attribute.
    let called = check(&format!("{src} (define (horizontal p) (x p))"));
    assert_eq!(cb.lookup("horizontal").unwrap().hash, called.lookup("horizontal").unwrap().hash);
    assert_eq!(run(&format!("{src} (define a (get y of: {{x: 1}} or: 0))"), "a"), Value::Int(0));
    assert_eq!(run(&format!("{src} (define b (get y of: {{x: 1, y: 2}} or: 0))"), "b"), Value::Int(2));
    // A local of the attribute's name is a key, not the attribute.
    let cb = check(&format!("{src} (define (f x d) (get x of: d or: 0))"));
    assert_eq!(cb.type_of("f").unwrap(), "(Keyword, d: Dict[a]) -> a | Int");
}
