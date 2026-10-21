//! `compare`: a total order over every value that agrees with `=`, and
//! the record field ids that `=` and `hash` once read as pointers, and
//! dictionary keys, which are text or keywords and hash alike as either.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode, field_id};
use wasmtime::{Engine, Instance, Module, Store};

/// Run `entry`, give its value back, and check nothing it made is left.
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

/// Attributes whose ids are multiples of 8, so a word holding one looks
/// like a pointer, and one whose id does not.
const ATTRIBUTES: &str = "(type c : (t/c integer)) (type s : (t/s integer)) (type a : (t/a integer))";

#[test]
fn the_test_attributes_look_like_pointers() {
    assert_eq!(field_id("t/c") % 8, 0);
    assert_eq!(field_id("t/s") % 8, 0);
    assert_ne!(field_id("t/a") % 8, 0);
}

#[test]
fn records_compare_and_hash_by_their_field_ids() {
    let src = format!(
        "{ATTRIBUTES}
         (define (pick x) (if x {{c: 1}} {{s: 1}}))
         (define out [(= (pick true) (pick false))
                      (= (pick true) {{c: 1}})
                      (= (hash {{c: 1}}) (hash (assoc {{}} c: 1)))
                      (= (hash {{c: 1}}) (hash {{s: 1}}))
                      (= (assoc {{c: 1}} s: 2) {{c: 1, s: 2}})
                      ;; A literal's keys and the keys `assoc` puts.
                      (= {{:k 1}} (assoc {{}} :k 1))
                      (= (hash {{:k 1}}) (hash (assoc {{}} :k 1)))])"
    );
    let bools = |v: &[bool]| Value::List(v.iter().map(|b| Value::Bool(*b)).collect());
    assert_eq!(run(&src, "out"), bools(&[false, true, true, false, true, true, true]));
}

/// Values in the order `compare` puts them (record field ids: `t/c`
/// 210538792, `t/s` 297343688, `t/a` 713141933), each group of equal values
/// together, written so that equal values are built apart and are not
/// one object.
const ORDERED: &[&[&str]] = &[
    &["-5"],
    &["0", "(- 3 3)"],
    &["3"],
    &["false", "(= 1 2)"],
    &["true"],
    &["\"\""],
    &["\"a\""],
    &["\"ab\"", "(join [\"a\" \"b\"])"],
    &["\"b\""],
    &[":a"],
    &[":b"],
    &["[]"],
    &["[1]", "(conj [] 1)"],
    &["[1 2]", "(conj [1] 2)"],
    &["[1 \"a\"]"],
    &["[2]"],
    &["{}", "(dissoc {:k 1} :k)"],
    // Records order by their fields' ids, which hash the attributes'
    // names: `t/c`, then `t/s`, then `t/a`.
    &["{c: 1}", "(assoc {} c: 1)"],
    &["{c: 1, s: 0}"],
    &["{c: 2}"],
    &["{s: 1}"],
    &["{a: 1}"],
    &["{:k 1}", "(assoc {} :k 1)"],
    &["{:k 2}"],
    &["{:k 2 :l 0}"],
    &["identity", "identity"],
];

#[test]
fn compare_is_a_total_order_that_agrees_with_equality() {
    let items: Vec<&str> = ORDERED.iter().flat_map(|group| group.iter().copied()).collect();
    let group: Vec<usize> = ORDERED.iter().enumerate().flat_map(|(g, group)| group.iter().map(move |_| g)).collect();
    let src = format!(
        "{ATTRIBUTES}
         (define (identity x) x)
         (define (row x ys)
           (match ys [] [] [y rest ...] (concat [[(compare x y) (= x y)]] (row x ys: rest))))
         (define (table xs ys)
           (match xs [] [] [x rest ...] (concat [(row x ys: ys)] (table rest ys: ys))))
         (define values [{}])
         (define out (table values ys: values))",
        items.join(" ")
    );
    let Value::List(rows) = run(&src, "out") else { panic!() };
    let n = items.len();
    let cell = |i: usize, j: usize| -> (i64, bool) {
        let Value::List(row) = &rows[i] else { panic!() };
        let Value::List(c) = &row[j] else { panic!() };
        match (&c[0], &c[1]) {
            (Value::Int(o), Value::Bool(e)) => (*o, *e),
            other => panic!("{other:?}"),
        }
    };
    for i in 0..n {
        for j in 0..n {
            let (order, equal) = cell(i, j);
            let expected = (group[i] as i64 - group[j] as i64).signum();
            assert_eq!(order, expected, "(compare {} {})", items[i], items[j]);
            assert_eq!(equal, order == 0, "(= {} {}) and (compare ..) disagree", items[i], items[j]);
        }
    }
}

#[test]
fn compare_has_a_name_and_a_type() {
    let mut cb = Codebase::new();
    cb.add_source("(define (f a b) (compare a b))").unwrap();
    assert_eq!(cb.type_of("f").unwrap(), "((\\ any b: any) : integer)");
}
