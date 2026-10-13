//! Labeled parameters and claimed attributes.

use conduct::Codebase;

fn check(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

#[test]
fn the_first_parameter_is_the_default_label() {
    let cb = check(
        "(define (sum x with) (- x with))
         (define a (sum 10 with: 3))
         (define b (sum with: 3 x: 10))
         (define (greet who greeting: g) (concat g who))
         (define c (greet [1] greeting: [2]))",
    );
    // Labels in any order call the same function the same way.
    assert_eq!(cb.lookup("a").unwrap().hash, cb.lookup("b").unwrap().hash);
    assert_eq!(cb.type_of("sum").unwrap(), "(Int, with: Int) -> Int");
    assert_eq!(cb.type_of("greet").unwrap(), "(List[a], greeting: List[a]) -> List[a]");
}

#[test]
fn calls_name_their_labels() {
    let src = "(define (f x y) x)";
    assert!(error(&format!("{src} (define z (f 1 2))")).contains("only the first argument goes without a label"));
    assert!(error(&format!("{src} (define z (f 1 q: 2))")).contains("`f` has no parameter labeled `q:`"));
    assert!(error(&format!("{src} (define z (f 1))")).contains("`f` needs y:"));
    assert!(error(&format!("{src} (define z (f 1 y: 2 y: 3))")).contains("appears twice"));
}

#[test]
fn function_values_carry_their_labels() {
    let cb = check(
        "(define (sum x with) (- x with))
         (define (apply2 g) (g 10 with: 4))
         (define d (apply2 sum))",
    );
    assert_eq!(cb.type_of("apply2").unwrap(), "((Int, with: Int) -> a) -> a");
    let e = error("(define (f x y) x) (define (g h) (h 1 z: 2)) (define w (g f))");
    assert!(e.contains("a function labeled `y:` is called with `z:`"), "{e}");
    let e = error("(define (g h) (h x: 1))");
    assert!(e.contains("takes its first argument without a label"), "{e}");
}

#[test]
fn records_are_keyed_by_declared_attributes() {
    let cb = check(
        "(type name : (io.gozala.person/name text))
         (type age : (io.gozala.person/age integer))
         (define ada {name: \"Ada\", age: 36})
         (define same {io.gozala.person/name: \"Ada\", age: 36})
         (define (older p) (assoc p age: (+ (age p) 1) name: (name p)))
         (define (nick p) (name p \"anonymous\"))",
    );
    assert_eq!(cb.type_of("ada").unwrap(), "{age: Int, name: String}");
    assert_eq!(cb.lookup("ada").unwrap().type_string(), "{io.gozala.person/age: Int, io.gozala.person/name: String}");
    // The local name and the global one are the same key.
    assert_eq!(cb.lookup("ada").unwrap().hash, cb.lookup("same").unwrap().hash);
    assert_eq!(cb.type_of("nick").unwrap(), "({name?: a}) -> a | String");
}

#[test]
fn a_type_constrains_what_an_attribute_holds() {
    let e = error("(type age : (t/age integer)) (define bad {age: \"old\"})");
    assert!(e.contains("the attribute `t/age` holds what its type says"), "{e}");
    let e = error("(type age : (t/age integer)) (define (f r) (assoc r age: true))");
    assert!(e.contains("the attribute `t/age`"), "{e}");
}

#[test]
fn keys_must_be_declared() {
    assert!(error("(define r {nmae: 1})").contains("`nmae:` is not a declared attribute"));
    assert!(error("(define r {t/x: 1})").contains("no type declares the attribute `t/x`"));
    assert!(error("(define (f r) (match r {nmae: x} x _ 0))").contains("`nmae:` is not a declared attribute"));
}

#[test]
fn types_agree() {
    let mut cb = check("(type name : (t/name text))");
    let e = cb.add_source("(type moniker : (t/name integer))").unwrap_err().to_string();
    assert!(e.contains("already declared with values `text`, not `integer`"), "{e}");
    let e = cb.add_source("(type name : (t/other text))").unwrap_err().to_string();
    assert!(e.contains("already declared as the attribute `t/name`"), "{e}");
    // A type may be repeated, and another local name may share a global.
    cb.add_source("(type name : (t/name text)) (type moniker : (t/name text))").unwrap();
    let e = cb.add_source("(define (name x) x)").unwrap_err().to_string();
    assert!(e.contains("declared as an attribute"), "{e}");
    let e = Codebase::new().add_source("(type f : (t/f text)) (define (f x) x)").unwrap_err().to_string();
    assert!(e.contains("cannot also name a type"), "{e}");
}

#[test]
fn retired_forms_say_what_to_write() {
    assert!(error("(def x 1)").contains("`def` is now `define`"));
    assert!(error("(defn f [x] x)").contains("`defn` is now `define`"));
    assert!(
        error("(type a : (t/a text)) (type b : (t/b text)) (define r {b: 1 :a 1})").contains("write `a:`, not `:a`")
    );
    assert!(error("(define d {\"a\" 1})").contains("a dictionary's keys are keywords: write `:a`"));
    assert!(error("(type a : (t/a text)) (define (f r) (:a r))").contains("read an attribute by calling it"));
    assert!(error("(define (f) 1)").contains("takes no parameters, so it is a value"));
}

#[test]
fn do_runs_in_order() {
    let cb = check("(define (f x) (do (+ x 1) (concat [x] [2])))");
    assert_eq!(cb.type_of("f").unwrap(), "(Int) -> List[Int]");
}
