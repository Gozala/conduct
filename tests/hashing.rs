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
    let a = hash("(defn inc [x] (let [one 1] (+ x one)))", "inc");
    let b = hash("(defn succ [n] (let [k 1] (+ n k)))", "succ");
    assert_eq!(a, b);
}

#[test]
fn changing_behaviour_changes_the_hash() {
    let a = hash("(defn f [x] (+ x 1))", "f");
    let b = hash("(defn f [x] (+ x 2))", "f");
    let c = hash("(defn f [x y] (+ x y))", "f");
    let d = hash("(defn f [x y] (+ y x))", "f");
    assert_ne!(a, b);
    assert_ne!(c, d);
}

#[test]
fn record_field_order_does_not_matter() {
    assert_eq!(hash("(def r {:a 1 :b 2})", "r"), hash("(def r {:b 2 :a 1})", "r"));
}

#[test]
fn dependencies_are_referenced_by_hash() {
    // Renaming a dependency leaves its dependents' hashes alone.
    let a = hash("(defn dbl [x] (* x 2)) (defn quad [x] (dbl (dbl x)))", "quad");
    let b = hash("(defn twice [x] (* x 2)) (defn quad [x] (twice (twice x)))", "quad");
    assert_eq!(a, b);
    // Changing a dependency changes its dependents.
    let c = hash("(defn dbl [x] (+ x x)) (defn quad [x] (dbl (dbl x)))", "quad");
    assert_ne!(a, c);
}

#[test]
fn cycles_hash_independently_of_names_and_order() {
    let a = "(defn even? [n] (if (= n 0) true (odd? (- n 1))))
             (defn odd? [n] (if (= n 0) false (even? (- n 1))))";
    let b = "(defn is-odd [k] (if (= k 0) false (is-even (- k 1))))
             (defn is-even [k] (if (= k 0) true (is-odd (- k 1))))";
    assert_eq!(hash(a, "even?"), hash(b, "is-even"));
    assert_eq!(hash(a, "odd?"), hash(b, "is-odd"));
    assert_ne!(hash(a, "even?"), hash(a, "odd?"));
}

#[test]
fn adding_known_code_is_not_checked_again() {
    let mut cb = Codebase::new();
    let first = cb.add_source("(defn inc [x] (+ x 1))").unwrap();
    assert!(first[0].new);
    let again = cb.add_source("(defn plus-one [y] (+ y 1))").unwrap();
    assert!(!again[0].new);
    assert_eq!(first[0].hash, again[0].hash);
    // Both names point at the one definition.
    assert_eq!(cb.names["inc"], cb.names["plus-one"]);
    assert_eq!(cb.defs.len(), 1);
}

#[test]
fn later_sources_build_on_earlier_ones() {
    let mut cb = Codebase::new();
    cb.add_source("(defn inc [x] (+ x 1))").unwrap();
    let added = cb.add_source("(def two (inc 1))").unwrap();
    assert_eq!(cb.defs[&added[0].hash].type_string(), "Int");
    assert!(cb.add_source("(def oops (missing 1))").is_err());
}
