//! Claims: record types, constructors and patterns, type constructors,
//! ranked variants, and signatures.

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

/// Compile and run a definition of no arguments.
fn run(src: &str, entry: &str) -> Value {
    let cb = check(src);
    let wat = codegen::compile_exports(&cb, &[(entry, cb.names[entry])]).unwrap();
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

const PERSON: &str = "
(claim name : (io.gozala.person/name text))
(claim age : (io.gozala.person/age integer))
(claim phone : (io.gozala.contact/phone text))
(claim person : {name age ? phone})";

#[test]
fn a_record_type_constructs() {
    let cb = check(&format!(
        "{PERSON}
         (define ada (person \"Ada\" age: 36))
         (define bob (person name: \"Bob\" age: 40 phone: \"555\"))
         (define same {{name: \"Ada\", age: 36}})"
    ));
    assert_eq!(cb.type_of("ada").unwrap(), "{age: Int, name: String}");
    assert_eq!(cb.type_of("bob").unwrap(), "{age: Int, name: String, phone: String}");
    // A constructor builds the record its attributes make.
    assert_eq!(cb.lookup("ada").unwrap().hash, cb.lookup("same").unwrap().hash);
    assert!(error(&format!("{PERSON} (define x (person \"Ada\"))")).contains("`person` needs `age:`"));
    assert!(
        error(&format!("{PERSON} (define x (person \"Ada\" age: 3 email: \"\"))"))
            .contains("no attribute labeled `email:`")
    );
    assert!(error(&format!("{PERSON} (define x (person \"Ada\" age: \"old\"))")).contains("holds what its claim says"));
}

#[test]
fn a_record_type_matches() {
    let cb = check(&format!(
        "{PERSON}
         (define (describe p) (match p (person who age: years) (str who \" \" years)))
         (define (call p) (match p (person phone: n) n _ \"none\"))"
    ));
    assert_eq!(cb.type_of("describe").unwrap(), "({age: Any, name: Any}) -> String");
    assert_eq!(cb.type_of("call").unwrap(), "({age?: Any, name?: Any, phone?: a}) -> a | String");
    let src = format!(
        "{PERSON}
         (define (describe p) (match p (person who age: years) (str who \" \" years)))
         (define x (describe (person \"Ada\" age: 36)))"
    );
    assert_eq!(run(&src, "x"), Value::Str("Ada 36".into()));
}

#[test]
fn constructors_of_variants() {
    let cb = check(
        "(define (safe-div n by) (if (= by 0) (error :division-by-zero) (ok (quot n by))))
         (define (show r) (match r (ok v) (str v) (error why) (str why)))",
    );
    assert_eq!(cb.type_of("safe-div").unwrap(), "(Int, by: Int) -> (error :division-by-zero) | (ok Int)");
    assert_eq!(cb.type_of("show").unwrap(), "((error Any) | (ok Any)) -> String");
    let src = "(define (safe-div n by) (if (= by 0) (error :division-by-zero) (ok (quot n by))))
               (define (show r) (match r (ok v) (str v) (error why) (str why)))";
    assert_eq!(run(&format!("{src} (define a (show (safe-div 7 by: 2)))"), "a"), Value::Str("3".into()));
    assert_eq!(
        run(&format!("{src} (define a (show (safe-div 7 by: 0)))"), "a"),
        Value::Str(":division-by-zero".into())
    );
}

#[test]
fn type_constructors_take_types() {
    let cb = check(
        "(claim (half integer) : (result integer error: [:odd]))
         (define (half n) (if (= (rem n 2) 0) (ok (quot n 2)) (error :odd)))",
    );
    assert_eq!(cb.type_of("half").unwrap(), "(Int) -> (error :odd) | (ok Int)");
    let e = error(
        "(claim (half integer) : (result integer error: [:odd]))
         (define (half n) (if (= (rem n 2) 0) (ok (quot n 2)) (error :uneven)))",
    );
    assert!(e.contains("does not fit its claim"), "{e}");
    assert!(error("(claim (box ?a) : {(t/box ?b)})").contains("`?b` is not one of the type's parameters"));
    assert!(error("(claim (f text) : (result integer)) (define (f x) (ok 1))").contains("needs `error:`"));
    assert!(error("(claim x : nothing)").contains("`nothing` is not a type"));
    assert!(error("(claim (x ?a) : (list ?a)) (define y (x 1))").contains("only a record type constructs"));
    assert!(error("(define (f x) (match x (nothing a) a _ 0))").contains("not a claimed record type"));
}

const JOB: &str = "
(claim description : (io.gozala.job/description text))
(claim started-at : (io.gozala.job/started-at integer))
(claim completed-at : (io.gozala.job/completed-at integer))
(claim idle : {description})
(claim pending : {description started-at})
(claim done : {description completed-at})";

#[test]
fn a_variant_ranks_overlapping_records() {
    // Arms in any order: the variant's rank decides, so a job that has
    // finished is done, though it has also started.
    let src = format!(
        "{JOB}
         (claim job : [done pending idle])
         (define (report job)
           (match job
             (idle description: d) (str \"Idle: \" d)
             (pending description: d) (str \"Pending: \" d)
             (done description: d) (str \"Done: \" d)))"
    );
    let cb = check(&src);
    assert_eq!(cb.type_of("report").unwrap(), "({completed-at?: Any, description: Any, started-at?: Any}) -> String");
    let job = |fields: &str| run(&format!("{src} (define a (report {{{fields}}}))"), "a");
    assert_eq!(job("description: \"x\""), Value::Str("Idle: x".into()));
    assert_eq!(job("description: \"x\", started-at: 1"), Value::Str("Pending: x".into()));
    assert_eq!(job("description: \"x\", started-at: 1, completed-at: 2"), Value::Str("Done: x".into()));
    // Covering every case is exhaustive: a value without a description
    // is not a job.
    let mut cb = check(&src);
    let e = cb.add_source("(claim other : (t/other ?other)) (define b (report {other: 1}))").unwrap_err().to_string();
    assert!(e.contains("io.gozala.job/description"), "{e}");
}

#[test]
fn a_wider_case_ranked_first_is_an_error() {
    let e = error(&format!("{JOB} (claim job : [idle done pending])"));
    assert!(e.contains("`idle` is ranked above `done`, but every `done` also fits `idle`"), "{e}");
}

#[test]
fn overlapping_records_need_a_rank() {
    let e = error(&format!("{JOB} (define (r j) (match j (done description: d) d (idle description: d) d))"));
    assert!(e.contains("a value may be more than one of `done`, `idle`"), "{e}");
    assert!(e.contains("(claim name : [done idle])"), "{e}");
}

#[test]
fn signatures_are_checked_and_shown() {
    let cb = check(
        "(claim (twice f: ((\\ ?a) : ?a) x: ?a) : ?a)
         (define (twice f x) (f (f x)))
         (claim (id ?a) : ?a)
         (define (id x) x)
         (claim pi : integer)
         (define pi 3)",
    );
    assert_eq!(cb.type_of("twice").unwrap(), "((a) -> a, x: a) -> a");
    assert_eq!(cb.type_of("id").unwrap(), "(a) -> a");
    let e = error("(claim (id ?a) : ?a) (define (id x) (+ x 1))");
    assert!(e.contains("?a, any type the caller chooses is not a Int"), "{e}");
    let e = error("(claim (f integer) : text) (define (f x) (+ x 1))");
    assert!(e.contains("Int is not a String"), "{e}");
    let e = error("(claim (f integer extra: integer) : integer) (define (f x y) x)");
    assert!(e.contains("the claim labels the parameters `extra:`, the definition `y:`"), "{e}");
    let e = error("(claim (f other: integer) : integer) (define (f x) x)");
    assert!(e.contains("the claim's first parameter is `other:`"), "{e}");
}

#[test]
fn a_signature_is_what_callers_see() {
    // The body answers only `:a`; the claim promises more, and callers
    // must handle it.
    let cb = check("(claim (pick integer) : [:a :b]) (define (pick n) :a)");
    assert_eq!(cb.type_of("pick").unwrap(), "(Int) -> :a | :b");
    // The claim is part of what the definition is.
    let plain = check("(define (pick n) :a)");
    assert_ne!(cb.lookup("pick").unwrap().hash, plain.lookup("pick").unwrap().hash);
}

#[test]
fn capabilities_in_signatures() {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![conduct::types::Ty::str()], conduct::types::Ty::record([])).unwrap();
    cb.add_source("(claim (greet! text) with: {print} : {}) (define (greet! who) (print! who))").unwrap();
    assert_eq!(cb.type_of("greet").unwrap(), "(String) -> (task {} with: {print})");
    let e = cb
        .add_source("(claim (quiet! text) with: {} : {}) (define (quiet! who) (print! who))")
        .unwrap_err()
        .to_string();
    assert!(e.contains("this uses `print`, where only pure code is allowed"), "{e}");
    // Left out, they are what the body uses.
    cb.add_source("(claim (hi! text) : {}) (define (hi! who) (print! who))").unwrap();
    assert_eq!(cb.type_of("hi").unwrap(), "(String) -> (task {} with: {print})");
}
