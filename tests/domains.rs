//! Definitions are hashes, and names point at them. A source may name the
//! domain its definitions go in, as Dialog names attributes; what a name
//! refers to is decided when the source is added, and stored as a hash.

use conduct::Codebase;

fn add(cb: &mut Codebase, src: &str) -> Vec<String> {
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}")).into_iter().map(|a| a.name).collect()
}

fn error(cb: &mut Codebase, src: &str) -> String {
    cb.add_source(src).unwrap_err().to_string()
}

fn refs(cb: &Codebase, name: &str) -> Vec<conduct::Hash> {
    let mut out = Vec::new();
    cb.lookup(name).unwrap().term.refs(&mut out);
    out
}

#[test]
fn a_domain_names_the_definitions_of_a_source() {
    let mut cb = Codebase::new();
    let added = add(&mut cb, "(domain io.gozala.math) (define (double x) (* 2 x)) (define four (double 2))");
    assert_eq!(added, ["io.gozala.math/double", "io.gozala.math/four"]);
    assert_eq!(cb.type_of("io.gozala.math/double").unwrap(), "((\\ integer) : integer)");
    assert!(cb.type_of("double").is_none());
}

#[test]
fn a_name_is_written_whole_relative_or_bare() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.math) (define (double x) (* 2 x))");
    add(&mut cb, "(define a (io.gozala.math/double 1))");
    // No other definition is named `double`, so it can only be that one.
    add(&mut cb, "(define b (double 2))");
    // `./` is the source's own domain.
    add(&mut cb, "(domain io.gozala.math) (define quad (./double (./double 1)))");
    let double = cb.names["io.gozala.math/double"];
    assert_eq!(refs(&cb, "a"), [double]);
    assert_eq!(refs(&cb, "b"), [double]);
    assert_eq!(refs(&cb, "io.gozala.math/quad"), [double]);
}

#[test]
fn a_name_in_two_domains_is_ambiguous_until_written_whole() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.list) (define (size xs) (count xs))");
    add(&mut cb, "(domain io.gozala.text) (define (size s) (str s \"!\"))");
    let e = error(&mut cb, "(define n (size [1 2]))");
    assert!(e.contains("`size` could be any of `io.gozala.list/size`, `io.gozala.text/size`"), "{e}");
    add(&mut cb, "(define n (io.gozala.list/size [1 2]))");
    // Inside a domain, `./size` is unambiguous; a bare `size` is not.
    add(&mut cb, "(domain io.gozala.list) (define two (./size [1 2]))");
    let e = error(&mut cb, "(domain io.gozala.list) (define three (size [1 2 3]))");
    assert!(e.contains("could be any of"), "{e}");
}

#[test]
fn a_built_in_is_a_candidate_too() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.list) (define (count xs) 0)");
    let e = error(&mut cb, "(define n (count [1 2]))");
    assert!(e.contains("could be any of `io.gozala.list/count`, `conduct/count`"), "{e}");
    add(&mut cb, "(define n (conduct/count [1 2]))");
    assert_eq!(cb.type_of("n").unwrap(), "natural");
    // A definition with no domain still comes before the built-in.
    add(&mut cb, "(define (trim s) 1) (define t (trim \"a\"))");
    assert_eq!(cb.type_of("t").unwrap(), "natural");
}

#[test]
fn a_definition_of_the_source_comes_first() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain a.one) (define (f x) x)");
    add(&mut cb, "(domain b.two) (define (f x) (+ x 1)) (define g (f 1))");
    assert_eq!(refs(&cb, "b.two/g"), [cb.names["b.two/f"]]);
}

#[test]
fn the_same_code_in_two_domains_is_one_definition() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain a.one) (define (inc x) (+ x 1))");
    let again = cb.add_source("(domain b.two) (define (inc x) (+ x 1))").unwrap();
    assert!(!again[0].new, "the hash was already there");
    assert_eq!(cb.names["a.one/inc"], cb.names["b.two/inc"]);
}

#[test]
fn recursion_and_claims_in_a_domain() {
    let mut cb = Codebase::new();
    add(
        &mut cb,
        "(domain io.gozala.list)
         (claim (len (list any)) : integer)
         (define (len xs) (match xs [] 0 [_ rest ...] (+ 1 (len rest))))
         (define (even? xs) (match xs [] true [_ rest ...] (odd? rest)))
         (define (odd? xs) (match xs [] false [_ rest ...] (even? rest)))",
    );
    assert_eq!(cb.type_of("io.gozala.list/len").unwrap(), "((\\ (list any)) : integer)");
    assert_eq!(cb.type_of("io.gozala.list/odd?").unwrap(), "((\\ (list any)) : boolean)");
}

#[test]
fn a_domain_comes_first_and_once() {
    let mut cb = Codebase::new();
    let e = error(&mut cb, "(define a 1) (domain io.gozala.x)");
    assert!(e.contains("one domain, first"), "{e}");
    let e = error(&mut cb, "(domain a.b) (domain c.d) (define x 1)");
    assert!(e.contains("one domain, first"), "{e}");
    let e = error(&mut cb, "(domain Io.Gozala) (define x 1)");
    assert!(e.contains("is not a domain"), "{e}");
    // A failed source leaves nothing behind, and the next source has no
    // domain.
    add(&mut cb, "(define y 2)");
    assert!(cb.type_of("y").is_some());
}
