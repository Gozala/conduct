//! A definition's hash depends on what it does, not on what anything is
//! called.

use conduct::{Codebase, Hash};

fn hash(src: &str, name: &str) -> Hash {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb.names[name]
}

#[test]
fn renaming_locals_and_definitions_keeps_the_hash() {
    // Parameters' labels are what callers pass arguments by, so they stay;
    // the names the body knows things by may change.
    let a = hash("(define (inc x) (let [one 1] (+ x one)))", "inc");
    let b = hash("(define (succ x) (let [k 1] (+ x k)))", "succ");
    assert_eq!(a, b);
    let a = hash("(define (add x y) (+ x y))", "add");
    let b = hash("(define (plus x y: n) (+ x n))", "plus");
    assert_eq!(a, b);
}

#[test]
fn changing_behaviour_changes_the_hash() {
    let a = hash("(define (f x) (+ x 1))", "f");
    let b = hash("(define (f x) (+ x 2))", "f");
    let c = hash("(define (f x y) (+ x y))", "f");
    let d = hash("(define (f x y) (+ y x))", "f");
    assert_ne!(a, b);
    assert_ne!(c, d);
}

#[test]
fn record_field_order_does_not_matter() {
    let claims = "(type a : (t/a ?a)) (type b : (t/b ?b))";
    assert_eq!(
        hash(&format!("{claims} (define r {{a: 1, b: 2}})"), "r"),
        hash(&format!("{claims} (define r {{b: 2, a: 1}})"), "r")
    );
}

#[test]
fn dependencies_are_referenced_by_hash() {
    // Renaming a dependency leaves its dependents' hashes alone.
    let a = hash("(define (dbl x) (* x 2)) (define (quad x) (dbl (dbl x)))", "quad");
    let b = hash("(define (twice x) (* x 2)) (define (quad x) (twice (twice x)))", "quad");
    assert_eq!(a, b);
    // Changing a dependency changes its dependents.
    let c = hash("(define (dbl x) (+ x x)) (define (quad x) (dbl (dbl x)))", "quad");
    assert_ne!(a, c);
}

#[test]
fn cycles_hash_independently_of_names_and_order() {
    let a = "(define (even? n) (if (= n 0) true (odd? (- n 1))))
             (define (odd? n) (if (= n 0) false (even? (- n 1))))";
    let b = "(define (is-odd n) (if (= n 0) false (is-even (- n 1))))
             (define (is-even n) (if (= n 0) true (is-odd (- n 1))))";
    assert_eq!(hash(a, "even?"), hash(b, "is-even"));
    assert_eq!(hash(a, "odd?"), hash(b, "is-odd"));
    assert_ne!(hash(a, "even?"), hash(a, "odd?"));
}

#[test]
fn adding_known_code_is_not_checked_again() {
    let mut cb = Codebase::new();
    // What every codebase starts with, `core.cdt`.
    let core = cb.defs.len();
    let first = cb.add_source("(define (inc x) (+ x 1))").unwrap();
    assert!(first[0].new);
    let again = cb.add_source("(define (plus-one x) (+ x 1))").unwrap();
    assert!(!again[0].new);
    assert_eq!(first[0].hash, again[0].hash);
    // Both names point at the one definition.
    assert_eq!(cb.names["inc"], cb.names["plus-one"]);
    assert_eq!(cb.defs.len(), core + 1);
}

#[test]
fn later_sources_build_on_earlier_ones() {
    let mut cb = Codebase::new();
    cb.add_source("(define (inc x) (+ x 1))").unwrap();
    let added = cb.add_source("(define two (inc 1))").unwrap();
    assert_eq!(cb.defs[&added[0].hash].type_string(), "natural");
    assert!(cb.add_source("(define oops (missing 1))").is_err());
}
