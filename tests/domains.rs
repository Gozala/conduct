//! Definitions are hashes, and names point at them. A name is written
//! with as much of a prefix as it needs, `parser/run` or `run` for
//! `conduct.parser/run`, and `domain` writes a prefix for the definitions
//! it wraps. What a name refers to is decided when its source is added,
//! and stored as a hash.

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
fn a_definition_is_named_as_written() {
    let mut cb = Codebase::new();
    let added = add(&mut cb, "(define (io.gozala.math/double x) (* 2 x)) (define four (double 2))");
    assert_eq!(added, ["io.gozala.math/double", "four"]);
    assert_eq!(refs(&cb, "four"), [cb.names["io.gozala.math/double"]]);
}

#[test]
fn a_domain_prefixes_what_it_wraps() {
    let mut cb = Codebase::new();
    let added = add(
        &mut cb,
        "(domain io.gozala.math
           (define (double x) (* 2 x))
           (define four (double 2))
           (define quad (./double (./double 1)))
           (define (other.place/kept x) x))
         (define outside 1)",
    );
    assert_eq!(
        added,
        ["io.gozala.math/double", "io.gozala.math/four", "io.gozala.math/quad", "other.place/kept", "outside"]
    );
    assert_eq!(cb.type_of("io.gozala.math/double").unwrap(), "((\\ (& ?a integer)) : [?a natural])");
}

#[test]
fn a_name_is_any_trailing_run_of_its_segments() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.math (define (double x) (* 2 x)))");
    add(
        &mut cb,
        "(define a (io.gozala.math/double 1)) (define b (gozala.math/double 1)) (define c (math/double 1)) (define d (double 1))",
    );
    let double = cb.names["io.gozala.math/double"];
    for name in ["a", "b", "c", "d"] {
        assert_eq!(refs(&cb, name), [double], "{name}");
    }
    // Segments, not characters: `ath/double` is not a suffix.
    let e = error(&mut cb, "(define e (ath/double 1))");
    assert!(e.contains("`ath/double` is not defined"), "{e}");
}

#[test]
fn a_name_in_two_domains_needs_more_of_its_name() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.list (define (size xs) (count xs)))");
    add(&mut cb, "(domain io.gozala.text (define (size s) (join [s \"!\"])))");
    let e = error(&mut cb, "(define n (size [1 2]))");
    assert!(e.contains("`size` could be any of `io.gozala.list/size`, `io.gozala.text/size`"), "{e}");
    add(&mut cb, "(define n (list/size [1 2]))");
    assert_eq!(refs(&cb, "n"), [cb.names["io.gozala.list/size"]]);
}

#[test]
fn a_built_in_is_a_candidate_too() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.list (define (count xs) 0))");
    let e = error(&mut cb, "(define n (count [1 2]))");
    assert!(e.contains("could be any of `io.gozala.list/count`, `conduct/count`"), "{e}");
    add(&mut cb, "(define n (conduct/count [1 2]))");
    assert_eq!(cb.type_of("n").unwrap(), "natural");
    // A definition of the source named as a built-in is a candidate as a
    // definition of the codebase is: the use decides when it can.
    let e = error(&mut cb, "(define (trim s) 1) (define t (trim \"a\"))");
    assert!(e.contains("`trim` could be any of `trim`, `conduct/trim`"), "{e}");
    add(&mut cb, "(define (trim s by) 1) (define t (trim \"a\" by: 2))");
    assert_eq!(cb.type_of("t").unwrap(), "byte");
    add(&mut cb, "(define u (trim \" a \"))");
    assert_eq!(cb.type_of("u").unwrap(), "text");
}

#[test]
fn a_definition_of_the_source_comes_first() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain a.one (define (f x) x))");
    add(&mut cb, "(domain b.two (define (f x) (+ x 1)) (define g (f 1)))");
    assert_eq!(refs(&cb, "b.two/g"), [cb.names["b.two/f"]]);
    // Two of the source's own that the name could be are ambiguous too.
    let e = error(&mut cb, "(define (c.one/h x) x) (define (c.two/h x) x) (define k (h 1))");
    assert!(e.contains("`h` could be any of `c.one/h`, `c.two/h`"), "{e}");
}

#[test]
fn the_same_code_under_two_names_is_one_definition() {
    let mut cb = Codebase::new();
    add(&mut cb, "(domain a.one (define (inc x) (+ x 1)))");
    let again = cb.add_source("(domain b.two (define (inc x) (+ x 1)))").unwrap();
    assert!(!again[0].new, "the hash was already there");
    assert_eq!(cb.names["a.one/inc"], cb.names["b.two/inc"]);
}

#[test]
fn recursion_and_claims_in_a_domain() {
    let mut cb = Codebase::new();
    add(
        &mut cb,
        "(domain io.gozala.list
           (claim (len (list any)) : integer)
           (define (len xs) (match xs [] 0 [_ rest ...] (+ 1 (len rest))))
           (define (even? xs) (match xs [] true [_ rest ...] (odd? rest)))
           (define (odd? xs) (match xs [] false [_ rest ...] (even? rest))))",
    );
    assert_eq!(cb.type_of("io.gozala.list/len").unwrap(), "((\\ (list any)) : integer)");
    assert_eq!(cb.type_of("io.gozala.list/odd?").unwrap(), "((\\ (list any)) : boolean)");
}

#[test]
fn a_domain_wraps_what_it_names() {
    let mut cb = Codebase::new();
    let e = error(&mut cb, "(domain io.gozala.x) (define a 1)");
    assert!(e.contains("a domain wraps what it names"), "{e}");
    let e = error(&mut cb, "(domain a.b (domain c.d (define x 1)))");
    assert!(e.contains("domains do not nest"), "{e}");
    let e = error(&mut cb, "(domain Io.Gozala (define x 1))");
    assert!(e.contains("is not a domain"), "{e}");
    // A failed source leaves nothing behind.
    add(&mut cb, "(define y 2)");
    assert!(cb.type_of("y").is_some());
    assert!(cb.type_of("x").is_none());
}

#[test]
fn a_name_several_things_could_be_is_the_one_that_fits() {
    // Unison's type-directed name resolution: where only one candidate
    // makes the definition check, the name is that one.
    let mut cb = Codebase::new();
    add(&mut cb, "(domain io.gozala.list (define (len xs) (count xs)))");
    add(&mut cb, "(domain io.gozala.text (define (len s) (byte-count s)))");
    add(&mut cb, "(define a (len [1 2])) (define b (len \"abc\"))");
    assert_eq!(refs(&cb, "a"), [cb.names["io.gozala.list/len"]]);
    assert_eq!(refs(&cb, "b"), [cb.names["io.gozala.text/len"]]);
    // A built-in is a candidate like any other.
    add(&mut cb, "(domain io.gozala.text (define (count s) (byte-count s)))");
    add(&mut cb, "(define c (count [1 2 3]))");
    assert_eq!(refs(&cb, "c"), [], "the built-in `count`, which a list fits");
    // When none fits, or several do, it is still ambiguous.
    let e = error(&mut cb, "(define d (len 5))");
    assert!(e.contains("`len` could be any of"), "{e}");
    let e = error(&mut cb, "(define (e x) (len x))");
    assert!(e.contains("`len` could be any of"), "{e}");
}
