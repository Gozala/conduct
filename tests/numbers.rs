//! Naturals and integers, and what a standard library builds on them:
//! bits, hashes, and lists updated by index.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store, Val};

fn types(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

/// Compile `entry`, call it, and check that the call freed everything
/// it allocated once its result is given back.
fn run(src: &str, entry: &str) -> Value {
    let cb = types(src);
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
    let wasm = wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).unwrap();
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
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "blocks leaked by {entry}");
    value
}

fn ints(ns: &[i64]) -> Value {
    Value::List(ns.iter().map(|n| Value::Int(*n)).collect())
}

#[test]
fn a_number_that_is_not_negative_is_a_natural() {
    let cb = types(
        "(define one 1)
         (define minus -1)
         (define sum (+ 1 2))
         (define size (count [1 2 3]))
         (define (twice n) (* 2 n))
         (claim (index natural) : natural)
         (define (index n) n)
         (define at (index 3))",
    );
    assert_eq!(cb.type_of("one").unwrap(), "natural");
    assert_eq!(cb.type_of("minus").unwrap(), "integer");
    // Arithmetic is on integers, which every natural is.
    assert_eq!(cb.type_of("sum").unwrap(), "integer");
    assert_eq!(cb.type_of("size").unwrap(), "natural");
    assert_eq!(cb.type_of("twice").unwrap(), "((\\ integer) : integer)");
    assert_eq!(cb.type_of("at").unwrap(), "natural");
    let e = error("(claim (index natural) : natural) (define (index n) n) (define x (index -1))");
    assert!(e.contains("integer is not a natural"), "{e}");
    // A natural and an integer together are an integer.
    let cb = types("(define (pick b) (if b 1 -1))");
    assert_eq!(cb.type_of("pick").unwrap(), "((\\ boolean) : integer)");
}

#[test]
fn hexadecimal_and_binary() {
    let cb = types("(define mask 0x1f) (define bits 0b1010) (define low -0x10)");
    assert_eq!(cb.type_of("mask").unwrap(), "natural");
    assert_eq!(run("(define mask 0x1f)", "mask"), Value::Int(31));
    assert_eq!(run("(define bits 0b1010)", "bits"), Value::Int(10));
    assert_eq!(run("(define low -0x10)", "low"), Value::Int(-16));
    let e = error("(define x 0xfffffffffffffffff)");
    assert!(e.contains("fits in 63 bits"), "{e}");
}

#[test]
fn bits_of_naturals() {
    let cb = types("(define (mix a b) (bit-xor (shift-left a 5) b))");
    assert_eq!(cb.type_of("mix").unwrap(), "((\\ natural b: natural) : natural)");
    let e = error("(define x (bit-and -1 3))");
    assert!(e.contains("integer is not a natural"), "{e}");
    let src = "(define bits [(bit-and 12 10) (bit-or 12 10) (bit-xor 12 10)
                              (shift-left 1 4) (shift-right 256 4) (count-ones 0xff)
                              (shift-left 1 61) (shift-left 1 62) (shift-left 3 61) (shift-right 7 70)])";
    assert_eq!(run(src, "bits"), ints(&[8, 14, 6, 16, 16, 8, 1 << 61, 0, 1 << 61, 0]));
}

#[test]
fn equal_values_hash_alike() {
    let src = "(type n : (t/n integer))
               (define hashes [(hash \"abc\") (hash (str \"a\" \"bc\"))
                               (hash [1 2 [3]]) (hash (conj [1 2] [3]))
                               (hash {n: 1}) (hash (assoc {} n: 1))
                               (hash {}) (hash (dissoc {:k 1} :k))
                               (hash \"abd\") (hash [1 2]) (hash :abc)])";
    let Value::List(h) = run(src, "hashes") else { panic!() };
    assert_eq!(h[0], h[1]);
    assert_eq!(h[2], h[3]);
    assert_eq!(h[4], h[5]);
    assert_eq!(h[6], h[7], "`{{}}` is one value as a record and as a dictionary");
    assert_ne!(h[0], h[8]);
    assert_ne!(h[2], h[9]);
    assert_ne!(h[0], h[10], "a keyword is not its text");
    for v in &h {
        let Value::Int(n) = v else { panic!() };
        assert!((0..1i64 << 32).contains(n), "a hash is a 32-bit natural");
    }
}

#[test]
fn lists_updated_by_index() {
    let cb = types("(define (swap xs) (set-at xs 0 (item xs at: 1 or: 0)))");
    assert_eq!(cb.type_of("swap").unwrap(), "((\\ (list ?a)) : (list [?a natural]))");
    assert_eq!(run("(define x [(item [1 2] at: 1 or: 0) (item [1 2] at: 2 or: 0)])", "x"), ints(&[2, 0]));
    let src = "(define xs [(set-at [1 2 3] 1 20) (set-at [1 2 3] 3 0)
                           (insert-at [1 2 3] 0 0) (insert-at [1 2 3] 2 25) (insert-at [1 2 3] 9 4)
                           (remove-at [1 2 3] 0) (remove-at [1 2 3] 2) (remove-at [1 2 3] 3)
                           (insert-at [] 0 1) (remove-at [1] 0)])";
    assert_eq!(
        run(src, "xs"),
        Value::List(vec![
            ints(&[1, 20, 3]),
            ints(&[1, 2, 3]),
            ints(&[0, 1, 2, 3]),
            ints(&[1, 2, 25, 3]),
            ints(&[1, 2, 3, 4]),
            ints(&[2, 3]),
            ints(&[1, 2]),
            ints(&[1, 2, 3]),
            ints(&[1]),
            ints(&[]),
        ])
    );
    // A list another value still holds is copied, not changed, and the
    // values in it are counted, so nothing leaks or is freed twice.
    let src = "(define pair (let [xs [[1] [2] [3]]
                                 ys (set-at xs 0 [10])
                                 zs (insert-at xs 1 [15])
                                 ws (remove-at xs 2)]
                             [xs ys zs ws]))";
    let one = |n| ints(&[n]);
    assert_eq!(
        run(src, "pair"),
        Value::List(vec![
            Value::List(vec![one(1), one(2), one(3)]),
            Value::List(vec![one(10), one(2), one(3)]),
            Value::List(vec![one(1), one(15), one(2), one(3)]),
            Value::List(vec![one(1), one(2)]),
        ])
    );
    // A list built up by inserts grows in place.
    let src = "(define (fill xs n) (if (= n 0) xs (fill (insert-at xs 0 n) n: (- n 1))))
               (define built (fill [] n: 40))";
    let Value::List(items) = run(src, "built") else { panic!() };
    assert_eq!(items.len(), 40);
    assert_eq!(items[0], Value::Int(1));
}

#[test]
fn an_integer_becomes_a_natural_or_a_default() {
    let cb = types("(define (count-down n) (natural (- n 1) or: 0))");
    assert_eq!(cb.type_of("count-down").unwrap(), "((\\ integer) : natural)");
    let src = "(define r [(natural 5 or: 0) (natural (- 0 3) or: 7) (natural 0 or: 9)])";
    assert_eq!(run(src, "r"), ints(&[5, 7, 0]));
    assert!(error("(define r (natural 1 or: (- 0 1)))").contains("natural"));
}
