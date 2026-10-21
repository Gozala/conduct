//! Macros that are functions: `(macro name f)`, with `f` run while
//! compiling on the form of each call.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

fn codebase(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

fn run(src: &str) -> Value {
    let cb = codebase(src);
    let wat = codegen::compile_exports(&cb, &[("out", cb.names["out"])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let out = instance.get_typed_func::<(), i64>(&mut store, "out").unwrap().call(&mut store, ()).unwrap();
    decode(instance.get_memory(&mut store, "memory").unwrap().data(&store), out)
}

#[test]
fn a_macro_function_writes_its_expansion() {
    // `(swap f a b)` is `(f b a)`.
    let out = run("(macro swap
                     (fn [call declared]
                       (match call
                         (form-list [_ f a b]) (ok `(~f ~b ~a))
                         _ (error \"`swap` takes a function and two arguments\"))))
                   (define out (swap - 1 10))");
    assert_eq!(out, Value::Int(9));
    let e = error("(macro swap (fn [call declared] (error \"no\"))) (define out (swap - 1 10))");
    assert!(e.contains("(swap ..)") && e.contains("no"), "{e}");
}

#[test]
fn quasiquote_splices_lists() {
    // `(sum-of a b c)` is `(+ a (+ b (+ c 0)))`, written by recursion over
    // the forms of the call.
    let out = run("(define (sum-form xs)
                     (match xs
                       [] `0
                       [x rest ...] `(+ ~x ~(sum-form rest))))
                   (macro sum-of (fn [call declared] (match call (form-list [_ xs ...]) (ok (sum-form xs)) _ (error \"\"))))
                   (macro listing (fn [call declared] (match call (form-list [_ xs ...]) (ok `[0 ~@xs 4]) _ (error \"\"))))
                   (define out [(sum-of 1 2 3 4) (listing 1 2 3) (sum-of)])");
    assert_eq!(out, Value::List(vec![Value::Int(10), Value::List((0..5).map(Value::Int).collect()), Value::Int(0)]));
}

#[test]
fn a_macro_function_reads_what_names_are_declared_as() {
    // `(describe x)` is the text of what `x` is declared as.
    let src = "(type x : (geo/x integer))
               (type title : (doc/title text))
               (define (show f)
                 (match f
                   (form-symbol s) s
                   (form-list items) (join [\"(\" (join (map items each: show) with: \" \") \")\"])
                   _ \"?\"))
               (macro describe
                 (fn [call declared]
                   (match call
                     (form-list [_ (form-symbol n)])
                     (match (declaration-of n in: declared)
                       :undeclared (error (join [\"`\" n \"` is not declared\"]))
                       f (ok (form-text (show f))))
                     _ (error \"`describe` takes a name\"))))
               (define out [(describe x) (describe title)])";
    assert_eq!(
        run(src),
        Value::List(vec![Value::Str("(geo/x integer)".into()), Value::Str("(doc/title text)".into())])
    );
    let e = error(&src.replace("(describe title)", "(describe nothing)"));
    assert!(e.contains("`nothing` is not declared"), "{e}");
}

#[test]
fn a_macro_function_is_checked_and_bounded() {
    // It must be a function of a form to a form.
    let e = error("(macro bad (fn [call declared] 3)) (define out (bad))");
    assert!(e.contains("bad#macro"), "{e}");
    // One that never stops is stopped.
    let e = error(
        "(define (forever n) (forever (+ n 1)))
                   (macro loop (fn [call declared] (if (= (forever 0) 0) (ok `1) (ok `2))))
                   (define out (loop))",
    );
    assert!(e.contains("ran too long"), "{e}");
    // It cannot reach the host: it is a function, and a function cannot
    // perform a task.
    let e = error(
        "(macro now (fn [call declared] (ok (form-integer (count (perform! (fetch \"GET\" \"/\" \"\" :x)))))))
                   (define out (now))",
    );
    assert!(e.contains("performs a task"), "{e}");
}
