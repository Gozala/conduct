//! Names are paths to hashes. A source may name the namespace its
//! definitions go in and the namespaces it uses; what a name refers to is
//! decided when the source is added, and stored as a hash.

use conduct::Codebase;

fn add(cb: &mut Codebase, src: &str) -> Vec<String> {
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}")).into_iter().map(|a| a.name).collect()
}

fn error(cb: &mut Codebase, src: &str) -> String {
    cb.add_source(src).unwrap_err().to_string()
}

#[test]
fn a_namespace_names_the_definitions_of_a_source() {
    let mut cb = Codebase::new();
    let added = add(&mut cb, "(namespace math) (define (double x) (* 2 x)) (define four (double 2))");
    assert_eq!(added, ["math/double", "math/four"]);
    assert_eq!(cb.type_of("math/double").unwrap(), "((\\ integer) : integer)");
    assert!(cb.type_of("double").is_none());
}

#[test]
fn another_source_writes_the_whole_name_uses_the_namespace_or_a_unique_ending() {
    let mut cb = Codebase::new();
    add(&mut cb, "(namespace math) (define (double x) (* 2 x))");
    add(&mut cb, "(define a (math/double 1))");
    add(&mut cb, "(use math) (define b (double 2))");
    // No other name ends with `double`, so it can only be that one.
    add(&mut cb, "(define c (double 3))");
    let hashes: Vec<_> = ["a", "b", "c"].iter().map(|n| cb.lookup(n).unwrap().term.clone()).collect();
    // Each refers to the same hash, however it was written.
    let mut refs = Vec::new();
    for t in &hashes {
        let mut r = Vec::new();
        t.refs(&mut r);
        refs.push(r);
    }
    assert_eq!(refs[0], refs[1]);
    assert_eq!(refs[1], refs[2]);
}

#[test]
fn two_names_that_end_alike_are_ambiguous_until_one_is_chosen() {
    let mut cb = Codebase::new();
    add(&mut cb, "(namespace list) (define (size xs) (count xs))");
    add(&mut cb, "(namespace text) (define (size s) (concat s \"!\"))");
    let e = error(&mut cb, "(define n (size [1 2]))");
    assert!(e.contains("`size` could be any of `list/size`, `text/size`"), "{e}");
    add(&mut cb, "(use list) (define n (size [1 2]))");
    let e = error(&mut cb, "(use list text) (define m (size [1 2]))");
    assert!(e.contains("could be any of `list/size`, `text/size`"), "{e}");
}

#[test]
fn a_name_in_the_namespace_comes_before_the_same_name_elsewhere() {
    let mut cb = Codebase::new();
    add(&mut cb, "(define (twice x) (+ x x))");
    add(&mut cb, "(namespace strings) (define (twice s) (concat s s))");
    add(&mut cb, "(namespace strings) (define shout (twice \"a\"))");
    assert_eq!(cb.type_of("strings/shout").unwrap(), "text");
    add(&mut cb, "(define sum (twice 2))");
    assert_eq!(cb.type_of("sum").unwrap(), "integer");
}

#[test]
fn a_definition_of_the_source_comes_before_the_codebase() {
    let mut cb = Codebase::new();
    add(&mut cb, "(namespace a) (define (f x) x)");
    add(&mut cb, "(namespace b) (define (f x) (+ x 1)) (define g (f 1))");
    let mut refs = Vec::new();
    cb.lookup("b/g").unwrap().term.refs(&mut refs);
    assert_eq!(refs, [cb.names["b/f"]]);
}

#[test]
fn the_same_code_in_two_namespaces_is_one_definition() {
    let mut cb = Codebase::new();
    add(&mut cb, "(namespace a) (define (inc x) (+ x 1))");
    let again = cb.add_source("(namespace b) (define (inc x) (+ x 1))").unwrap();
    assert!(!again[0].new, "the hash was already there");
    assert_eq!(cb.names["a/inc"], cb.names["b/inc"]);
}

#[test]
fn recursion_and_claims_in_a_namespace() {
    let mut cb = Codebase::new();
    add(
        &mut cb,
        "(namespace list)
         (claim (len (list any)) : integer)
         (define (len xs) (match xs [] 0 [_ rest ...] (+ 1 (len rest))))
         (define (even? xs) (match xs [] true [_ rest ...] (odd? rest)))
         (define (odd? xs) (match xs [] false [_ rest ...] (even? rest)))",
    );
    assert_eq!(cb.type_of("list/len").unwrap(), "((\\ (list any)) : integer)");
    assert_eq!(cb.type_of("list/odd?").unwrap(), "((\\ (list any)) : boolean)");
}

#[test]
fn namespace_and_use_come_first() {
    let mut cb = Codebase::new();
    let e = error(&mut cb, "(define a 1) (use math)");
    assert!(e.contains("come first"), "{e}");
    let e = error(&mut cb, "(namespace a) (namespace b) (define x 1)");
    assert!(e.contains("one namespace"), "{e}");
    let e = error(&mut cb, "(namespace define) (define x 1)");
    assert!(e.contains("is not a namespace"), "{e}");
    // A failed source leaves nothing behind, and the next source has no
    // namespace.
    add(&mut cb, "(define y 2)");
    assert!(cb.type_of("y").is_some());
}
