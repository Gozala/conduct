//! Text by byte offset: what a parser walks its source with.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn types(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

/// Run `entry`, give its value back, and check nothing it made is left.
fn run(src: &str, entry: &str) -> Value {
    let cb = types(src);
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

fn list(src: &str) -> Vec<Value> {
    let Value::List(items) = run(src, "out") else { panic!() };
    items
}

fn int(n: i64) -> Value {
    Value::Int(n)
}

fn text(s: &str) -> Value {
    Value::Str(s.into())
}

#[test]
fn bytes_and_code_points() {
    let items = list(
        "(define out [(byte-count \"héllo\") (byte-count \"\")
                      (byte-at \"abc\" at: 1 or: -1) (byte-at \"abc\" at: 3 or: -1) (byte-at \"é\" at: 1 or: -1)
                      (code-point-at \"é\" at: 0 or: -1) (code-point-at \"é\" at: 1 or: -1)
                      (code-point-at \"a€\" at: 1 or: -1) (code-point-at \"😀\" at: 0 or: -1)
                      (code-point-at \"abc\" at: 3 or: -1)])",
    );
    // `é` is two bytes; its second is no code point's start.
    assert_eq!(
        items,
        vec![int(6), int(0), int(98), int(-1), int(0xa9), int(233), int(-1), int(0x20ac), int(0x1f600), int(-1)]
    );
    // An offset is a natural: the checker refuses one that may be negative.
    let e = Codebase::new().add_source("(define (f n) (byte-at \"abc\" at: (- n 1) or: 0))").unwrap_err().to_string();
    assert!(e.contains("integer is not a natural"), "{e}");
}

#[test]
fn slices_and_searches() {
    let items = list(
        "(define out [(slice \"hello\" from: 1 to: 3) (slice \"hello\" from: 3 to: 1) (slice \"hello\" from: 2 to: 99)
                      (slice \"hello\" from: 0 to: 5) (slice \"héllo\" from: 1 to: 3)
                      (occurs? \"ll\" in: \"hello\" at: 2) (occurs? \"ll\" in: \"hello\" at: 3)
                      (occurs? \"o\" in: \"hello\" at: 9) (occurs? \"\" in: \"hello\" at: 5)
                      (index-of \"l\" in: \"hello\" from: 0 or: -1) (index-of \"l\" in: \"hello\" from: 3 or: -1)
                      (index-of \"z\" in: \"hello\" from: 0 or: -1) (index-of \"lo\" in: \"hello\" from: 4 or: -1)
                      (index-of \"\" in: \"hello\" from: 2 or: -1)])",
    );
    assert_eq!(
        items,
        vec![
            text("el"),
            text(""),
            text("llo"),
            text("hello"),
            text("é"),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(true),
            int(2),
            int(3),
            int(-1),
            int(-1),
            int(2),
        ]
    );
}

#[test]
fn text_from_code_points() {
    let items = list(
        "(define out [(from-code-point 65) (from-code-point 233) (from-code-point 0x20ac) (from-code-point 0x1f600)
                      (from-code-point 0x110000) (from-code-point 0xd800)
                      (= (from-code-point (code-point-at \"€\" at: 0 or: 0)) \"€\")])",
    );
    assert_eq!(
        items,
        vec![text("A"), text("é"), text("€"), text("😀"), text("\u{fffd}"), text("\u{fffd}"), Value::Bool(true)]
    );
}

#[test]
fn what_may_be_missing_answers_its_default() {
    let cb = types(
        "(define (head t) (code-point-at t at: 0 or: :end))
         (define (find t) (index-of \",\" in: t from: 0 or: :none))
         (define (piece t) (slice t from: 1 to: 2))",
    );
    assert_eq!(cb.type_of("head").unwrap(), "((\\ text) : [natural :end])");
    assert_eq!(cb.type_of("find").unwrap(), "((\\ text) : [natural :none])");
    assert_eq!(cb.type_of("piece").unwrap(), "((\\ text) : text)");
}
