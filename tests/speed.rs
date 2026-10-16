//! How fast compiled code runs: ignored by default, run with
//! `cargo test --test speed -- --ignored --nocapture`.

use conduct::Codebase;
use conduct::codegen;
use std::time::Instant;
use wasmtime::{Engine, Instance, Module, Store};

fn time(src: &str, entry: &str) -> (i64, f64) {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let start = Instant::now();
    let out = f.call(&mut store, ()).unwrap();
    (out >> 1, start.elapsed().as_secs_f64())
}

#[test]
#[ignore]
fn a_counting_loop() {
    let n = 50_000_000;
    let (out, secs) = time(
        &format!(
            "(define (count-up n acc) (if (= n 0) acc (count-up (- n 1) acc: (+ acc 1))))
             (define out (count-up {n} acc: 0))"
        ),
        "out",
    );
    assert_eq!(out, n);
    println!("counting loop: {:.2} s, {:.1} ns an iteration", secs, secs * 1e9 / n as f64);
}

#[test]
#[ignore]
fn summing_a_list() {
    let (out, secs) = time(
        "(define (build xs n) (if (= n 0) xs (build (conj xs n) n: (- n 1))))
         (define (sum xs at total) (if (= at (count xs)) total (sum xs at: (+ at 1) total: (+ total (item xs at: at or: 0)))))
         (define out (sum (build [] n: 2000000) at: 0 total: 0))",
        "out",
    );
    assert_eq!(out, 2_000_000 * 2_000_001 / 2);
    println!("building and summing 2M items: {secs:.2} s");
}

#[test]
#[ignore]
fn a_computed_constant_is_computed_once() {
    // `table` costs a million steps to compute; using it a thousand
    // times costs no more than using it once.
    let (out, secs) = time(
        "(define (fill xs n) (if (= n 0) xs (fill (conj xs n) n: (- n 1))))
         (define table (fill [] n: 1000000))
         (define (use-it n acc) (if (= n 0) acc (use-it (- n 1) acc: (+ acc (count table)))))
         (define out (use-it 1000 acc: 0))",
        "out",
    );
    assert_eq!(out, 1_000_000_000);
    println!("a constant used a thousand times: {secs:.2} s");
}
