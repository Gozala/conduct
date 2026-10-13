//! Macros by rules: patterns in the head, the narrowest rule wins, and
//! hygiene from marks and pins.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn check(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

fn run(cb: &Codebase, entry: &str) -> Value {
    let wat = codegen::compile_exports(cb, &[(entry, cb.names[entry])]).unwrap();
    let wasm = wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let out = f.call(&mut store, ()).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    decode(memory.data(&store), out)
}

#[test]
fn a_call_expands_by_its_rule() {
    let cb = check(
        "(macro (unless test then: e) (if test {} e))
         (define (f x) (unless (= x 0) then: :zero))
         (define g (cond (= 1 2) :no (= 1 1) :yes :else :never))
         (define h (and true (or false true) true))",
    );
    assert_eq!(cb.type_of("f").unwrap(), "(Any) -> {} | :zero");
    assert_eq!(run(&cb, "g"), Value::Keyword("yes".into()));
    assert_eq!(run(&cb, "h"), Value::Bool(true));
    let e = error("(define x (cond))");
    assert!(e.contains("no rule of `cond` matches `(cond)`"), "{e}");
}

#[test]
fn rules_are_a_set() {
    // A call may come before the rules it expands by, and the rules'
    // order is not part of the macro.
    let a = check("(define x (twice 2)) (macro (twice v) (+ v v)) (macro (twice v w) (+ v w))");
    let b = check("(macro (twice v w) (+ v w)) (define x (twice 2)) (macro (twice v) (+ v v))");
    assert_eq!(a.macros["twice"].hash, b.macros["twice"].hash);
    assert_eq!(a.lookup("x").unwrap().hash, b.lookup("x").unwrap().hash);
    assert_eq!(run(&a, "x"), Value::Int(4));
}

#[test]
fn the_narrowest_rule_wins() {
    let cb = check(
        "(macro (size :none) 0)
         (macro (size x) 1)
         (macro (size x more ...) 2)
         (define a [(size :none) (size :some) (size 1 2 3)])",
    );
    let ints = |v: &[i64]| Value::List(v.iter().map(|n| Value::Int(*n)).collect());
    assert_eq!(run(&cb, "a"), ints(&[0, 1, 2]));
    let e = error("(macro (f :x b) b) (macro (f a :y) a)");
    assert!(e.contains("some call matches both `(f :x b)` and `(f a :y)`, and neither is narrower"), "{e}");
    let e = error("(macro (f a) a) (macro (f b) b)");
    assert!(e.contains("match the same calls"), "{e}");
}

#[test]
fn a_later_compilation_adds_rules() {
    let mut cb = check("(macro (pick :a) 1)");
    let before = cb.macros["pick"].hash;
    cb.add_source("(macro (pick :b) 2) (define both [(pick :a) (pick :b)])").unwrap();
    assert_ne!(cb.macros["pick"].hash, before);
    assert_eq!(run(&cb, "both"), Value::List(vec![Value::Int(1), Value::Int(2)]));
    // A failed compilation leaves the macro as it was.
    let after = cb.macros["pick"].hash;
    assert!(cb.add_source("(macro (pick x) 3) (macro (pick y) 4)").is_err());
    assert_eq!(cb.macros["pick"].hash, after);
}

#[test]
fn repetition() {
    let cb = check(
        "(macro (pairs (k v) ...) [[k v] ...])
         (define p (pairs (1 2) (3 4)))
         (macro (all x ...) (and true x ...))
         (define t (all (= 1 1) (= 2 2)))",
    );
    let ints = |v: &[i64]| Value::List(v.iter().map(|n| Value::Int(*n)).collect());
    assert_eq!(run(&cb, "p"), Value::List(vec![ints(&[1, 2]), ints(&[3, 4])]));
    assert_eq!(run(&cb, "t"), Value::Bool(true));
    assert!(error("(macro (f x ...) x)").contains("so the template uses it under `...` too"));
    assert!(error("(macro (f x) (g x ...))").contains("repeats nothing"));
    assert!(error("(macro (f x ... y) x)").contains("nothing follows it"));
    assert!(error("(macro (f x x) x)").contains("binds `x` twice"));
}

#[test]
fn what_a_template_binds_no_call_captures() {
    // The template's `tmp` is not the caller's.
    let cb = check(
        "(macro (add-first a b) (let [tmp a] (+ tmp b)))
         (define (f tmp) (add-first 1 tmp))
         (define r (f 10))",
    );
    assert_eq!(run(&cb, "r"), Value::Int(11));
}

#[test]
fn what_a_template_names_no_local_shadows() {
    // The caller's `helper` is a local; the template's is the definition.
    let cb = check(
        "(define (helper x) (+ x 100))
         (macro (bump v) (helper v))
         (define (g helper) (bump helper))
         (define r (g 1))",
    );
    assert_eq!(run(&cb, "r"), Value::Int(101));
}

#[test]
fn what_a_template_names_is_pinned() {
    let mut cb = check("(define (helper x) (+ x 1)) (macro (inc1 v) (helper v))");
    cb.add_source("(define (helper x) (+ x 1000))").unwrap();
    cb.add_source("(define r (inc1 1))").unwrap();
    assert_eq!(run(&cb, "r"), Value::Int(2));
}

#[test]
fn a_macro_may_write_a_definition() {
    let cb = check("(macro (constant name value) (define name value)) (constant answer 42)");
    assert_eq!(run(&cb, "answer"), Value::Int(42));
}

#[test]
fn a_name_is_one_thing() {
    assert!(error("(define and 1)").contains("`and` is a macro"));
    assert!(error("(macro (if x) x)").contains("reserved"));
    assert!(error("(macro (concat x) x)").contains("built in"));
    assert!(error("(define (f x) x) (macro (f x) x)").contains("`f` is a macro"));
    let mut cb = check("(define (g x) x)");
    let e = cb.add_source("(macro (g x) x)").unwrap_err().to_string();
    assert!(e.contains("already defined"), "{e}");
}

#[test]
fn a_template_performs() {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![conduct::types::Ty::str()], conduct::types::Ty::record([])).unwrap();
    cb.add_source("(macro (twice! line) (do (print! line) (print! line))) (define (greet! who) (twice! who))").unwrap();
    assert_eq!(cb.type_of("greet").unwrap(), "(String) -> (task {} with: {print})");
}

#[test]
fn a_macro_does_not_hide_a_task() {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![conduct::types::Ty::str()], conduct::types::Ty::record([])).unwrap();
    let e = cb.add_source("(macro (print! x) x)").unwrap_err().to_string();
    assert!(e.contains("already defined"), "{e}");
}

#[test]
fn expansions_do_not_change_hashes_or_labels() {
    // A definition a template writes is the definition written by hand.
    let cb = check(
        "(macro (mkid name) (define (name x) x))
         (mkid id1) (mkid id2) (define (id3 x) x)
         (macro (mkpair name) (define (name x y) [x y]))
         (mkpair pair)
         (define p (pair 1 y: 2))",
    );
    let h = |n: &str| cb.lookup(n).unwrap().hash;
    assert_eq!(h("id1"), h("id3"));
    assert_eq!(h("id2"), h("id3"));
    assert_eq!(cb.type_of("pair").unwrap(), "(a, y: a) -> List[a]");
}

#[test]
fn source_cannot_write_a_mark() {
    let e = error("(macro (add-first a b) (let [tmp a] (+ tmp b))) (define r (add-first 1 tmp\u{1}1))");
    assert!(e.contains("control characters"), "{e}");
}

#[test]
fn adding_the_same_source_again() {
    let src = "(define (helper x) (+ x 1)) (macro (inc1 v) (helper v))";
    let mut cb = check(src);
    let before = cb.macros["inc1"].hash;
    cb.add_source(src).unwrap();
    assert_eq!(cb.macros["inc1"].hash, before);
}

#[test]
fn a_definition_a_template_writes_refers_to_itself() {
    let mut cb = check("(define (count n) 999)");
    cb.add_source(
        "(macro (defcount) (define (count n) (if (= n 0) 0 (count (- n 1)))))
         (defcount)
         (define r (count 3))",
    )
    .unwrap();
    assert_eq!(run(&cb, "r"), Value::Int(0));
}

#[test]
fn a_macro_and_a_local_do_not_share_a_name() {
    let e = error("(macro (sq x) (* x x)) (define (q2 sq) (sq 3))");
    assert!(e.contains("`sq` is a macro, so it cannot also name a local"), "{e}");
    let e = error("(define (q3 and) 1)");
    assert!(e.contains("`and` is a macro"), "{e}");
    let e = error("(macro (foo! x) x) (define (foo x) (+ x 1))");
    assert!(e.contains("`foo` is a macro"), "{e}");
}

#[test]
fn errors_show_names_as_written() {
    let e = error("(macro (g a b) a) (macro (m x) (g y)) (define r (m 1))");
    assert!(e.contains("no rule of `g` matches `(g y)`"), "{e}");
    assert!(!e.contains('\u{1}'), "{e}");
}

#[test]
fn a_variable_matched_once_repeats_with_the_others() {
    let cb = check("(macro (offsets v x ...) [(+ v x) ...]) (define o (offsets 10 1 2))");
    assert_eq!(run(&cb, "o"), Value::List(vec![Value::Int(11), Value::Int(12)]));
}
