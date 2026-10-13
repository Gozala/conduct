//! List patterns, and functions defined by several clauses.

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

fn ints(v: &[i64]) -> Value {
    Value::List(v.iter().map(|n| Value::Int(*n)).collect())
}

#[test]
fn list_patterns_in_match() {
    let cb = check(
        "(define (describe xs)
           (match xs
             [] :empty
             [x] :one
             [x y rest ...] :more))
         (define (second xs) (match xs [_ y _ ...] y _ 0))
         (define (tail xs) (match xs [_ rest ...] rest [] []))
         (define r [(describe []) (describe [1]) (describe [1 2 3])])
         (define s [(second [1 2 3]) (second [1])])
         (define t (tail [1 2 3]))",
    );
    assert_eq!(cb.type_of("describe").unwrap(), "((\\ (list any)) : [:empty :more :one])");
    assert_eq!(cb.type_of("tail").unwrap(), "((\\ (list ?a)) : (list ?a))");
    let kws = |v: &[&str]| Value::List(v.iter().map(|k| Value::Keyword(k.to_string())).collect());
    assert_eq!(run(&cb, "r"), kws(&["empty", "one", "more"]));
    assert_eq!(run(&cb, "s"), ints(&[2, 0]));
    assert_eq!(run(&cb, "t"), ints(&[2, 3]));
    let e = error("(define (f xs) (match xs [] 0 [x] 1))");
    assert!(e.contains("cannot cover every value"), "{e}");
    assert!(error("(define (f xs) (match xs [x ... y] 0 _ 1))").contains("nothing follows it"));
    assert!(error("(define (f xs) (match xs [[x] ...] 0 _ 1))").contains("a name for the rest"));
}

#[test]
fn several_defines_are_one_function() {
    let cb = check(
        "(define (len []) 0)
         (define (len [_ rest ...]) (+ 1 (len rest)))
         (define (fib 0) 0)
         (define (fib 1) 1)
         (define (fib n) (+ (fib (- n 1)) (fib (- n 2))))
         (define r [(len [1 2 3]) (fib 10)])",
    );
    assert_eq!(cb.type_of("len").unwrap(), "((\\ (list any)) : integer)");
    assert_eq!(cb.type_of("fib").unwrap(), "((\\ integer) : integer)");
    assert_eq!(run(&cb, "r"), ints(&[3, 55]));
}

#[test]
fn clauses_are_a_set() {
    let a = check("(define (sign 0) :zero) (define (sign n) (if (< n 0) :negative :positive))");
    let b = check("(define (sign n) (if (< n 0) :negative :positive)) (define (sign 0) :zero)");
    assert_eq!(a.lookup("sign").unwrap().hash, b.lookup("sign").unwrap().hash);
    assert_eq!(a.type_of("sign").unwrap(), "((\\ integer) : [:negative :positive :zero])");
}

#[test]
fn clauses_with_labels_and_variants() {
    let cb = check(
        "(define (show (ok v)) (str \"ok \" v))
         (define (show (error e)) (str \"error \" e))
         (define (take [] n) [])
         (define (take [_ _ ...] n: 0) [])
         (define (take [x rest ...] n) (concat [x] (take rest n: (- n 1))))
         (define r [(show (ok 1)) (show (error :no))])
         (define t (take [1 2 3] n: 2))",
    );
    assert_eq!(cb.type_of("show").unwrap(), "((\\ [(error any) (ok any)]) : text)");
    assert_eq!(run(&cb, "r"), Value::List(vec![Value::Str("ok 1".into()), Value::Str("error :no".into())]));
    assert_eq!(run(&cb, "t"), ints(&[1, 2]));
}

#[test]
fn clauses_must_be_ordered_and_cover() {
    let e = error("(define (f x) 1) (define (f x) 2)");
    assert!(e.contains("the clauses `(x)` and `(x)` match the same calls"), "{e}");
    let e = error("(define (f x) 1) (define (f y) 2)");
    assert!(e.contains("call the first parameter `x` and `y`"), "{e}");
    let e = error("(define (g [x rest ...] n) 1) (define (g xs n: 0) 2) (define (g [] n) 3)");
    assert!(e.contains("neither is narrower"), "{e}");
    let e = error("(define (h []) 0)");
    assert!(e.contains("do not cover every call"), "{e}");
    let e = error("(define (k []) 0) (define (k [x]) 1)");
    assert!(e.contains("do not cover every call"), "{e}");
    let e = error("(define (m xs) 0) (define (m xs by: 1) 1)");
    assert!(e.contains("different parameters"), "{e}");
    let e = error("(define v 1) (define v 2)");
    assert!(e.contains("`v` is defined twice"), "{e}");
}

#[test]
fn a_task_by_clauses() {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![conduct::types::Ty::str()], conduct::types::Ty::record([])).unwrap();
    cb.add_source("(define (say! []) {}) (define (say! [x rest ...]) (do (print! x) (say! rest)))").unwrap();
    assert_eq!(cb.type_of("say").unwrap(), "((\\ (list text)) : (task {} with: {print}))");
}

#[test]
fn clauses_of_several_parameters_cover_together() {
    let cb = check(
        "(define (zip [] ys) [])
         (define (zip [x xs ...] ys: []) [])
         (define (zip [x xs ...] ys: [y ys ...]) (concat [[x y]] (zip xs ys: ys)))
         (define z (zip [1 2 3] ys: [4 5]))
         (define (both (ok a) right: (ok b)) (ok [a b]))
         (define (both (ok a) right) (error :not-ok))
         (define (both (error e) right) (error e))
         (define r (match (both (ok 1) right: (ok 2)) (ok [a b]) (+ a b) _ 0))",
    );
    assert_eq!(run(&cb, "z"), Value::List(vec![ints(&[1, 4]), ints(&[2, 5])]));
    // Clauses that overlap, neither narrower, are an error even where
    // they would agree: a set has no first.
    let e = error("(define (zip [] ys) []) (define (zip xs ys: []) []) (define (zip [x xs ...] ys: [y ys ...]) [])");
    assert!(e.contains("neither is narrower"), "{e}");
    assert_eq!(run(&cb, "r"), Value::Int(3));
    // A value in a column no clause matches anything in must be one of
    // the variants named there.
    let e = error(
        "(define (pick a b: :x) 1) (define (pick a b: :y) 2)
         (define v (pick 5 b: :z))",
    );
    assert!(e.contains(":z"), "{e}");
    let e = error("(define (p (ok a) b: :x) 1) (define (p (error a) b: :y) 2)");
    assert!(e.contains("do not cover every call"), "{e}");
}

#[test]
fn a_signature_for_clauses() {
    let cb = check(
        "(claim (len (list ?a)) : integer)
         (define (len []) 0)
         (define (len [_ rest ...]) (+ 1 (len rest)))",
    );
    // `?a` appears once, so it asks nothing: any list.
    assert_eq!(cb.type_of("len").unwrap(), "((\\ (list any)) : integer)");
    let e = error(
        "(claim (len (list ?a)) : text)
         (define (len []) 0)
         (define (len [_ rest ...]) (+ 1 (len rest)))",
    );
    assert!(e.contains("integer is not a text"), "{e}");
}

#[test]
fn review_findings() {
    // A keyword and a variant of one key are different values.
    let e = error(
        "(define (f :conduct.result/ok b: :x) 1)
         (define (f (ok v) b: :y) 2)",
    );
    assert!(e.contains("do not cover every call"), "{e}");
    // Records with different fields may be one record, where anything
    // may come.
    let e = error(
        "(type name : (t/name text)) (type age : (t/age integer))
         (define (f {name: n}) :name) (define (f {age: a}) :age) (define (f x) :other)",
    );
    assert!(e.contains("neither is narrower"), "{e}");
    // A field every clause names is a record's field, not only a variant.
    let cb = check(
        "(type name : (t/name text)) (type age : (t/age integer))
         (define (g {name: n} b: :x) 1) (define (g {name: n} b: :y) 2)
         (define r (g {name: \"a\" age: 3} b: :x))",
    );
    assert_eq!(run(&cb, "r"), Value::Int(1));
    // A variant's rank orders the record types it ranks.
    let cb = check(
        "(type description : (io.gozala.job/description text))
         (type started-at : (io.gozala.job/started-at integer))
         (type completed-at : (io.gozala.job/completed-at integer))
         (type idle : {description})
         (type pending : {description started-at})
         (type done : {description completed-at})
         (type job : [done pending idle])
         (define (report (idle description: d)) (str \"Idle: \" d))
         (define (report (done description: d)) (str \"Done: \" d))
         (define (report (pending description: d)) (str \"Pending: \" d))
         (define r (report {description: \"x\", started-at: 1, completed-at: 2}))",
    );
    assert_eq!(run(&cb, "r"), Value::Str("Done: x".into()));
}

#[test]
fn boolean_patterns() {
    let cb = check(
        "(define (word true) \"yes\")
         (define (word false) \"no\")
         (define (both true b: true) :both)
         (define (both a b) :not-both)
         (define r [(word true) (word false)])
         (define s (match (= 1 1) true 1 false 0))",
    );
    assert_eq!(cb.type_of("word").unwrap(), "((\\ boolean) : text)");
    assert_eq!(run(&cb, "r"), Value::List(vec![Value::Str("yes".into()), Value::Str("no".into())]));
    assert_eq!(run(&cb, "s"), Value::Int(1));
    assert!(error("(define (w true) 1)").contains("do not cover every call"));
    // Covering both, a Bool is all a parameter may be.
    assert!(error("(define (word true) 1) (define (word false) 0) (define x (word 5))").contains("boolean"));
    let cb = check("(define (w b) (match b true 1 false 0))");
    assert_eq!(cb.type_of("w").unwrap(), "((\\ boolean) : integer)");
}

#[test]
fn a_record_pattern_binds_the_rest() {
    let cb = check(
        "(type action : (t/action keyword)) (type value : (t/value text))
         (define (act {action: :shout params ...}) (str (value params \"\") \"!\"))
         (define (act {action: :whisper value: v}) v)
         (define (act command) \"\")
         (define r [(act {action: :shout, value: \"hi\"}) (act {action: :whisper, value: \"psst\"}) (act {action: :nap})])
         (define (rest c) (match c {action: :shout others ...} others _ {}))
         (define o (rest {action: :shout, value: \"x\"}))",
    );
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(&cb, "r"), strs(&["hi!", "psst", ""]));
    // The rest is the record without what the pattern named.
    assert_eq!(cb.type_of("o").unwrap(), "{value: text}");
    let Value::Record(fields) = run(&cb, "o") else { panic!() };
    assert_eq!(fields.len(), 1);
    let e = error(
        "(type action : (t/action keyword))
         (define (f {action: :x others ...}) (action others))
         (define (f c) :none)",
    );
    assert!(e.contains("has no `t/action`") || e.contains("has no"), "{e}");
    assert!(error("(define (f {a: 1 [x] ...}) 1)").contains("ends with a name for the rest of the record"));
}
