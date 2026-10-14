//! A function value is identified as Unison identifies terms: by the hash
//! of its code, with what it captured. Not by where a module happens to
//! keep its code, so a closure is equal, hashes and orders alike in
//! every module it is compiled into.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

/// Run `entry` in a module of `src` alone, give its value back, and
/// check nothing it made is left.
fn run(src: &str, entry: &str) -> Value {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let out = f.call(&mut store, ()).unwrap_or_else(|e| panic!("{entry}: {e:?}"));
    let value = decode(instance.get_memory(&mut store, "memory").unwrap().data(&store), out);
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, out).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "{entry} leaked");
    value
}

const ADDER: &str = "(define (adder n) (fn [x] (+ x n))) (define (twice x) (* 2 x))";

#[test]
fn a_closure_is_its_code_and_what_it_captured() {
    let out = run(
        &format!(
            "{ADDER}
             (define (adder-too m) (fn [x] (+ x m)))
             (define (scaler n) (fn [x] (* x n)))
             (define out [(= (adder 1) (adder 1)) (= (adder 1) (adder 2))
                          (= (adder 1) (adder-too 1)) (= (hash (adder 1)) (hash (adder-too 1)))
                          (= (adder 1) (scaler 1))
                          (compare (adder 1) (adder-too 1))])"
        ),
        "out",
    );
    let Value::List(items) = out else { panic!() };
    assert_eq!(items[0], Value::Bool(true), "the same code capturing the same value");
    assert_eq!(items[1], Value::Bool(false), "the same code capturing different values");
    // The same lambda written in two definitions, its capture named apart,
    // is one function, as a term is one in Unison however it is named.
    assert_eq!(items[2], Value::Bool(true));
    assert_eq!(items[3], Value::Bool(true));
    assert_eq!(items[4], Value::Bool(false), "different code");
    assert_eq!(items[5], Value::Int(0));
}

#[test]
fn a_closure_hashes_alike_in_every_module() {
    let probe = "(define out [(hash (adder 7)) (hash twice)])";
    // In the second module other functions come first, so `adder`'s lambda
    // and `twice` have other places in its function table.
    let first = run(&format!("{ADDER} {probe}"), "out");
    let second = run(
        &format!(
            "(define (other a) (- a 1))
             {ADDER}
             (define out [(hash other) (hash (fn [q] (+ q 100))) (hash (adder 7)) (hash twice)])"
        ),
        "out",
    );
    let (Value::List(a), Value::List(b)) = (first, second) else { panic!() };
    assert_eq!(a[0], b[2], "a lambda's hash does not depend on its module");
    assert_eq!(a[1], b[3], "a definition's hash does not depend on its module");
}
