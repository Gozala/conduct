//! Bytes, after Roc's `List U8`: a list of naturals below 256, kept one
//! to a byte while it is one.

use conduct::Codebase;
use conduct::codegen::{self, KIND_BYTES, KIND_LIST, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn types(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

/// Run `entry`: its value, and the kind of object it is, if it is one.
/// Nothing it made may be left.
fn run(src: &str, entry: &str) -> (Value, Option<u32>) {
    let cb = types(src);
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let out = f.call(&mut store, ()).unwrap_or_else(|e| panic!("{entry}: {e:?}"));
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let data = memory.data(&store);
    let kind = (out & 7 == 0 && out != 0)
        .then(|| u32::from_le_bytes(data[out as usize..out as usize + 4].try_into().unwrap()));
    let value = decode(data, out);
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, out).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "{entry} leaked");
    (value, kind)
}

fn value(src: &str) -> Value {
    run(src, "out").0
}

fn ints(ns: &[i64]) -> Value {
    Value::List(ns.iter().map(|n| Value::Int(*n)).collect())
}

fn text(s: &str) -> Value {
    Value::Str(s.into())
}

#[test]
fn text_and_its_bytes() {
    assert_eq!(value("(define out (utf8 \"hé€\"))"), ints(&[104, 0xc3, 0xa9, 0xe2, 0x82, 0xac]));
    assert_eq!(run("(define out (utf8 \"hé\"))", "out").1, Some(KIND_BYTES));
    let Value::List(items) = value(
        "(define out [(from-utf8 (utf8 \"hé😀\") or: :bad) (from-utf8 [104 105] or: :bad) (from-utf8 [] or: :bad)
                      (from-utf8 [0xc3] or: :bad) (from-utf8 [0xc0 0x80] or: :bad) (from-utf8 [0xed 0xa0 0x80] or: :bad)
                      (from-utf8 [0xf4 0x90 0x80 0x80] or: :bad) (from-utf8 [0x80] or: :bad)
                      (from-utf8 [0xe0 0x80 0x80] or: :bad) (from-utf8 [0xf0 0x9f 0x98 0x80] or: :bad)])",
    ) else {
        panic!()
    };
    let bad = Value::Keyword("bad".into());
    assert_eq!(
        items,
        vec![
            text("hé😀"),
            text("hi"),
            text(""),
            bad.clone(),
            bad.clone(),
            bad.clone(),
            bad.clone(),
            bad.clone(),
            bad,
            text("😀"),
        ]
    );
    let cb = types(
        "(define (encode t) (utf8 t))
         (define (decode b) (from-utf8 b or: :invalid))
         (define (narrow n) (byte n or: 0))",
    );
    assert_eq!(cb.type_of("encode").unwrap(), "((\\ text) : (list byte))");
    assert_eq!(cb.type_of("decode").unwrap(), "((\\ (list byte)) : [text :invalid])");
    assert_eq!(cb.type_of("narrow").unwrap(), "((\\ integer) : byte)");
    // Only bytes are UTF-8.
    let e = Codebase::new().add_source("(define x (from-utf8 [300] or: \"\"))").unwrap_err().to_string();
    assert!(e.contains("natural is not a byte"), "{e}");
}

#[test]
fn bytes_are_a_list() {
    let Value::List(items) = value(
        "(define b (utf8 \"abc\"))
         (define out [(= b [97 98 99]) (= [97 98 99] b) (= b [97 98]) (= b (utf8 \"abc\")) (= b \"abc\")
                      (= (hash b) (hash [97 98 99])) (compare b [97 98 100]) (compare b [97 98]) (compare [97 98 99] b)
                      (count b) (first b 0) (rest b) (item b at: 2 or: 0) (item b at: 3 or: 0)
                      (concat b b) (concat b [1000]) (slice b from: 1 to: 9) (show b)
                      (match b [x y rest ...] [y x rest] _ [])])",
    ) else {
        panic!()
    };
    assert_eq!(
        items,
        vec![
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Int(-1),
            Value::Int(1),
            Value::Int(0),
            Value::Int(3),
            Value::Int(97),
            ints(&[98, 99]),
            Value::Int(99),
            Value::Int(0),
            ints(&[97, 98, 99, 97, 98, 99]),
            ints(&[97, 98, 99, 1000]),
            ints(&[98, 99]),
            text("[97 98 99]"),
            Value::List(vec![Value::Int(98), Value::Int(97), ints(&[99])]),
        ]
    );
}

#[test]
fn bytes_stay_bytes_while_they_are() {
    let kind = |e: &str| run(&format!("(define b (utf8 \"abc\")) (define out {e})"), "out");
    // What only bytes join stays bytes.
    for e in [
        "(conj b 100)",
        "(concat b b)",
        "(slice b from: 1 to: 2)",
        "(rest b)",
        "(set-at b 0 1)",
        "(insert-at b 1 7)",
        "(remove-at b 0)",
        "(filter b where: (fn [x] (> x 97)))",
        "(conj (conj [] 1) 2)",
    ] {
        assert_eq!(kind(e).1, Some(KIND_BYTES), "{e}");
    }
    // Anything else makes a list of values, with the same items.
    for (e, items) in [
        ("(conj b 256)", vec![97, 98, 99, 256]),
        ("(conj b -1)", vec![97, 98, 99, -1]),
        ("(set-at b 1 1000)", vec![97, 1000, 99]),
        ("(insert-at b 0 999)", vec![999, 97, 98, 99]),
        ("(concat [1000] b)", vec![1000, 97, 98, 99]),
        ("(map b each: (fn [x] (* x 2)))", vec![194, 196, 198]),
    ] {
        let (v, k) = kind(e);
        assert_eq!((v, k), (ints(&items), Some(KIND_LIST)), "{e}");
    }
    assert_eq!(value("(define out (conj (utf8 \"a\") \"b\"))"), Value::List(vec![Value::Int(97), text("b")]));
    // The types say the same.
    let cb = types(
        "(define (more b) (conj (utf8 b) 1))
         (define (wider b) (conj (utf8 b) 1000))",
    );
    assert_eq!(cb.type_of("more").unwrap(), "((\\ text) : (list byte))");
    assert_eq!(cb.type_of("wider").unwrap(), "((\\ text) : (list natural))");
}

#[test]
fn bytes_built_a_byte_at_a_time() {
    // A unique list of bytes grows in place, a byte at a time.
    let (v, k) = run(
        "(define (fill b n) (if (= n 0) b (fill (conj b (rem n 256)) n: (- n 1))))
         (define out (count (fill [] n: 100000)))",
        "out",
    );
    assert_eq!((v, k), (Value::Int(100000), None));
    let (v, k) = run(
        "(define (fill b n) (if (= n 0) b (fill (conj b (rem n 256)) n: (- n 1))))
         (define out (fill [] n: 600))",
        "out",
    );
    assert_eq!(k, Some(KIND_BYTES));
    let Value::List(items) = v else { panic!() };
    assert_eq!(items.len(), 600);
    assert_eq!(items[0], Value::Int(600 % 256));
    assert_eq!(items[599], Value::Int(1));
}

#[test]
fn a_literal_is_a_byte_until_it_is_not() {
    let cb = types(
        "(define small 255)
         (define big 256)
         (define sum (+ 200 100))
         (define half (quot 200 2))",
    );
    assert_eq!(cb.type_of("small").unwrap(), "byte");
    assert_eq!(cb.type_of("big").unwrap(), "natural");
    // A sum of bytes need not be one; a quotient is no larger.
    assert_eq!(cb.type_of("sum").unwrap(), "natural");
    assert_eq!(cb.type_of("half").unwrap(), "byte");
    assert_eq!(value("(define out (+ 200 100))"), Value::Int(300));
}
