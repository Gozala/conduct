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
(type name : (io.gozala.person/name text))
(type age : (io.gozala.person/age integer))
(type phone : (io.gozala.contact/phone text))
(type person : {name age ? phone})";

#[test]
fn a_record_type_constructs() {
    let cb = check(&format!(
        "{PERSON}
         (define ada (person \"Ada\" age: 36))
         (define bob (person name: \"Bob\" age: 40 phone: \"555\"))
         (define same {{name: \"Ada\", age: 36}})"
    ));
    assert_eq!(cb.type_of("ada").unwrap(), "{age: byte, name: text}");
    assert_eq!(cb.type_of("bob").unwrap(), "{age: byte, name: text, phone: text}");
    // A constructor builds the record its attributes make.
    assert_eq!(cb.lookup("ada").unwrap().hash, cb.lookup("same").unwrap().hash);
    assert!(error(&format!("{PERSON} (define x (person \"Ada\"))")).contains("`person` needs `age:`"));
    assert!(
        error(&format!("{PERSON} (define x (person \"Ada\" age: 3 email: \"\"))"))
            .contains("no attribute labeled `email:`")
    );
    assert!(error(&format!("{PERSON} (define x (person \"Ada\" age: \"old\"))")).contains("holds what its type says"));
}

#[test]
fn a_record_type_matches() {
    let cb = check(&format!(
        "{PERSON}
         (define (describe p) (match p (person who age: years) (str who \" \" years)))
         (define (call p) (match p (person phone: n) n _ \"none\"))"
    ));
    assert_eq!(cb.type_of("describe").unwrap(), "((\\ {age: any, name: any}) : text)");
    assert_eq!(cb.type_of("call").unwrap(), "((\\ {age?: any, name?: any, phone?: ?a}) : [?a text])");
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
    assert_eq!(
        cb.type_of("safe-div").unwrap(),
        "((\\ (& ?a integer) by: (& ?a integer)) : [(error :division-by-zero) (ok ?a)])"
    );
    assert_eq!(cb.type_of("show").unwrap(), "((\\ [(error any) (ok any)]) : text)");
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
    assert_eq!(cb.type_of("half").unwrap(), "((\\ integer) : [(error :odd) (ok integer)])");
    let e = error(
        "(claim (half integer) : (result integer error: [:odd]))
         (define (half n) (if (= (rem n 2) 0) (ok (quot n 2)) (error :uneven)))",
    );
    assert!(e.contains("does not fit its claim"), "{e}");
    assert!(error("(type (box ?a) : {(t/box ?b)})").contains("`?b` is not one of the type's parameters"));
    assert!(error("(claim (f text) : (result integer)) (define (f x) (ok 1))").contains("needs `error:`"));
    assert!(error("(type x : nowhere)").contains("`nowhere` is not a type"));
    assert!(error("(type (x ?a) : (list ?a)) (define y (x 1))").contains("only a record type constructs"));
    assert!(error("(define (f x) (match x (nothing a) a _ 0))").contains("not a declared record type"));
}

const JOB: &str = "
(type description : (io.gozala.job/description text))
(type started-at : (io.gozala.job/started-at integer))
(type completed-at : (io.gozala.job/completed-at integer))
(type idle : {description})
(type pending : {description started-at})
(type done : {description completed-at})";

#[test]
fn a_variant_ranks_overlapping_records() {
    // Arms in any order: the variant's rank decides, so a job that has
    // finished is done, though it has also started.
    let src = format!(
        "{JOB}
         (type job : [done pending idle])
         (define (report job)
           (match job
             (idle description: d) (str \"Idle: \" d)
             (pending description: d) (str \"Pending: \" d)
             (done description: d) (str \"Done: \" d)))"
    );
    let cb = check(&src);
    assert_eq!(cb.type_of("report").unwrap(), "((\\ {completed-at?: any, description: any, started-at?: any}) : text)");
    let job = |fields: &str| run(&format!("{src} (define a (report {{{fields}}}))"), "a");
    assert_eq!(job("description: \"x\""), Value::Str("Idle: x".into()));
    assert_eq!(job("description: \"x\", started-at: 1"), Value::Str("Pending: x".into()));
    assert_eq!(job("description: \"x\", started-at: 1, completed-at: 2"), Value::Str("Done: x".into()));
    // Covering every case is exhaustive: a value without a description
    // is not a job.
    let mut cb = check(&src);
    let e = cb.add_source("(type other : (t/other ?other)) (define b (report {other: 1}))").unwrap_err().to_string();
    assert!(e.contains("io.gozala.job/description"), "{e}");
}

#[test]
fn a_wider_case_ranked_first_is_an_error() {
    let e = error(&format!("{JOB} (type job : [idle done pending])"));
    assert!(e.contains("`idle` is ranked above `done`, but every `done` also fits `idle`"), "{e}");
}

#[test]
fn overlapping_records_need_a_rank() {
    let e = error(&format!("{JOB} (define (r j) (match j (done description: d) d (idle description: d) d))"));
    assert!(e.contains("a value may be more than one of `done`, `idle`"), "{e}");
    assert!(e.contains("(type name : [done idle])"), "{e}");
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
    assert_eq!(cb.type_of("twice").unwrap(), "((\\ ((\\ ?a) : ?a) x: ?a) : ?a)");
    assert_eq!(cb.type_of("id").unwrap(), "((\\ ?a) : ?a)");
    let e = error("(claim (id ?a) : ?a) (define (id x) (+ x 1))");
    assert!(e.contains("byte is not ?a, any type the caller chooses"), "{e}");
    let e = error("(claim (f integer) : text) (define (f x) (+ x 1))");
    assert!(e.contains("byte is not a text"), "{e}");
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
    assert_eq!(cb.type_of("pick").unwrap(), "((\\ integer) : [:a :b])");
    // The claim is part of what the definition is.
    let plain = check("(define (pick n) :a)");
    assert_ne!(cb.lookup("pick").unwrap().hash, plain.lookup("pick").unwrap().hash);
}

#[test]
fn capabilities_in_signatures() {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![conduct::types::Ty::str()], conduct::types::Ty::record([])).unwrap();
    cb.add_source("(claim (greet! text) with: {print} : {}) (define (greet! who) (print! who))").unwrap();
    assert_eq!(cb.type_of("greet").unwrap(), "((\\ text) : (task {} with: {print}))");
    let e = cb
        .add_source("(claim (quiet! text) with: {} : {}) (define (quiet! who) (print! who))")
        .unwrap_err()
        .to_string();
    assert!(e.contains("this uses `print`, where only pure code is allowed"), "{e}");
    // Left out, they are what the body uses.
    cb.add_source("(claim (hi! text) : {}) (define (hi! who) (print! who))").unwrap();
    assert_eq!(cb.type_of("hi").unwrap(), "((\\ text) : (task {} with: {print}))");
}

#[test]
fn constructors_are_values() {
    let cb = check(&format!(
        "{PERSON}
         (type pair : {{name age}})
         (define oks (map [1 2] each: ok))
         (define pairs (map [\"Ada\"] each: (fn [n] (pair n age: 1))))
         (define make pair)"
    ));
    assert_eq!(cb.type_of("oks").unwrap(), "(list (ok byte))");
    assert_eq!(cb.type_of("make").unwrap(), "((\\ (& ?a text) age: (& ?b integer)) : {age: ?b, name: ?a})");
    // With optional attributes, a constructor takes different labels at
    // different calls, so it is not a value yet.
    assert!(error(&format!("{PERSON} (define make person)")).contains("optional attributes"));
    assert!(
        error(&format!("{PERSON} (define (f p) (match p (person n name: m) n))"))
            .contains("gives `person`'s `name:` twice")
    );
}

#[test]
fn a_ranked_match_names_every_case_or_ends_with_any() {
    let job = format!("{JOB} (type job : [done pending idle])");
    let e = error(&format!("{job} (define (r j) (match j (pending description: d) d (done description: d) d))"));
    assert!(e.contains("this `match` does not name `idle`: name every case of `job`, or end with `_`"), "{e}");
    check(&format!("{job} (define (r j) (match j (pending description: d) d (done description: d) d _ \"?\"))"));
    // Cases with no widest one may leave a value fitting none.
    let src = "(type x : (t/x integer)) (type y : (t/y integer)) (type z : (t/z integer))
               (type a : {x y}) (type b : {x z}) (type v : [a b])";
    let e = error(&format!("{src} (define (f q) (match q (a x: n) n (b x: n) n))"));
    assert!(e.contains("a value may fit no case of `v`"), "{e}");
    let cb =
        check(&format!("{src} (define (f q) (match q (a x: n) n (b x: n) (+ n 100) _ 0)) (define r (f (b 1 z: 2)))"));
    assert_eq!(cb.type_of("r").unwrap(), "natural");
}

#[test]
fn a_type_declares_and_a_claim_signs() {
    let cb = check(
        "(type x : (io.gozala.point/x integer))
         (type y : (io.gozala.point/y integer))
         (type point : {x y})
         (define origin (point 0 y: 0))
         (define (shift p) (point (+ (x p) 1) y: (y p)))",
    );
    assert_eq!(cb.type_of("origin").unwrap(), "{x: byte, y: byte}");
    assert_eq!(
        cb.type_of("shift").unwrap(),
        "((\\ {x: (& ?a integer), y: (& ?b integer)}) : {x: [?a natural], y: ?b})"
    );
    // Attributes may be written in place, as a record writes its values.
    check("(type point : {io.gozala.point/x: integer io.gozala.point/y: integer}) (define o (point 0 y: 0))");
    check("(type point : {(io.gozala.point/x integer) (io.gozala.point/y integer)}) (define o (point 0 y: 0))");
    let e = error("(claim pi : integer)");
    assert!(e.contains("`pi` has no definition in this source"), "{e}");
    assert!(e.contains("(type pi : ..)"), "{e}");
    let e = error("(type pi : integer) (define pi 3)");
    assert!(e.contains("cannot also name a type: its signature is `(claim pi : ..)`"), "{e}");
    assert!(error("(type show! : integer)").contains("a type is not a task"));
    assert!(error("(type (t ?a) with: {print} : ?a)").contains("a type needs no capabilities"));
    let mut cb = check("(define pi 3)");
    let e = cb.add_source("(claim pi : integer)").unwrap_err().to_string();
    assert!(e.contains("a claim goes with its definition, in one source"), "{e}");
}

#[test]
fn members_of_a_record_are_attributes() {
    let point = "(type x : (io.gozala.point/x integer))
                 (type y : (io.gozala.point/y integer))
                 (type point : {x y})";
    let e = error(&format!("{point} (type line : {{point}})"));
    assert!(e.contains("`point` is a type; members of a record are attributes"), "{e}");
    // An attribute holding a record type nests it.
    let cb = check(&format!(
        "{point}
         (type start : (io.gozala.line/start point))
         (type end : (io.gozala.line/end point))
         (type line : {{start end}})
         (define l (line (point 0 y: 0) end: (point 1 y: 1)))
         (define (length l) (- (x (end l)) (x (start l))))"
    ));
    assert_eq!(cb.type_of("l").unwrap(), "{end: {x: byte, y: byte}, start: {x: byte, y: byte}}");
    assert_eq!(cb.type_of("length").unwrap(), "((\\ {end: {x: integer}, start: {x: integer}}) : integer)");
}

const TREE: &str = "
(type (leaf ?v) : {(t.tree/leaf ?v)})
(type (node ?c) : {(t.tree/node ?c)})
(type (tree ?a) : [(leaf ?a) (node (list (tree ?a)))])
(claim (total (tree natural)) : natural)
(define (total t) (match t (leaf n) n (node cs) (sum cs at: 0)))
(claim (sum (list (tree natural)) at: natural) : natural)
(define (sum cs at) (match cs [] at [c rest ...] (sum rest at: (+ at (total c)))))
(define x (total (node [(leaf 1) (node [(leaf 2) (leaf 3)])])))";

#[test]
fn a_type_may_be_made_of_itself() {
    let cb = check(TREE);
    assert_eq!(cb.type_of("total").unwrap(), "((\\ (rec ?a [(leaf natural) (node (list ?a))])) : natural)");
    assert_eq!(run(TREE, "x"), Value::Int(6));
    // Two types made of each other.
    let cb = check(
        "(type (pair ?p) : {(t.doc/pair ?p)})
         (type (many ?m) : {(t.doc/many ?m)})
         (type entry : {(t.doc/entry-value doc)})
         (type doc : [(pair text) (many (list entry))])
         (claim (depth doc) : natural)
         (define (depth d) (match d (pair _) 0 (many es) (count es)))",
    );
    assert!(cb.type_of("depth").unwrap().contains("rec"));
    // Not a type that grows each time it recurs.
    let e = error(
        "(type (leaf ?v) : {(t.tree/leaf ?v)})
         (type (wrap ?a) : {(t.tree/wrap ?a)})
         (type (nest ?a) : [(leaf ?a) (wrap (nest (list ?a)))])
         (claim (f (nest natural)) : natural)
         (define (f n) 1)",
    );
    assert!(e.contains("`nest` refers to itself with other types each time"), "{e}");
}
