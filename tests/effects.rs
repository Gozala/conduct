//! Roc-style effects: purity is inferred, a function's type says which
//! capabilities it uses, names end with `!` exactly when they use some,
//! and only the host provides the functions that use them.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use conduct::types::Ty;
use wasmtime::{Caller, Engine, Linker, Module, Store, Val};

/// A codebase with two host functions: `log!` (the `log` capability)
/// and `now!` (the `clock` capability).
fn host() -> Codebase {
    let mut cb = Codebase::new();
    cb.add_host("log!", "log", vec![Ty::str()], Ty::nil()).unwrap();
    cb.add_host("now!", "clock", vec![], Ty::int()).unwrap();
    cb
}

fn ty(src: &str, name: &str) -> String {
    let mut cb = host();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb.lookup(name).unwrap().type_string()
}

fn error(src: &str) -> String {
    host().add_source(src).unwrap_err().to_string()
}

#[test]
fn effects_are_inferred_from_calls() {
    assert_eq!(ty("(def x 1)", "x"), "Int");
    assert_eq!(ty("(defn greet! [name] (log! (str \"hi \" name)))", "greet!"), "(Any) => nil ! {log}");
    // Capabilities add up, through `let` and helper functions.
    let src = "(defn stamp! [msg] (let [t (now!)] (log! (str t \": \" msg))))
               (defn twice! [msg] (let [_ (stamp! msg)] (stamp! msg)))";
    assert_eq!(ty(src, "twice!"), "(Any) => nil ! {clock, log}");
    // A function given to `map` lends `map` its effect.
    assert_eq!(ty("(defn all! [xs] (map (fn [x] (log! x)) xs))", "all!"), "(List[String]) => List[nil] ! {log}");
    // A function that only creates an effectful function is pure.
    assert_eq!(ty("(defn logger [prefix] (fn [m] (log! (str prefix m))))", "logger"), "(Any) -> (Any) => nil ! {log}");
}

#[test]
fn effects_stay_polymorphic() {
    // `apply`'s effect is its argument's: pure with a pure function,
    // effectful with an effectful one, and named either way.
    let src = "(defn apply [f x] (f x))
               (defn pure-use [n] (apply (fn [x] (+ x 1)) n))
               (defn loud! [s] (apply log! s))";
    assert_eq!(ty(src, "pure-use"), "(Int) -> Int");
    assert_eq!(ty(src, "loud!"), "(String) => nil ! {log}");
}

#[test]
fn names_say_whether_a_function_uses_capabilities() {
    let e = error("(defn greet [name] (log! name))");
    assert!(e.contains("it uses `log`, so its name must end with `!`"), "{e}");
    let e = error("(defn shout! [s] (str s \"!\"))");
    assert!(e.contains("it uses no capabilities, so its name must not end with `!`"), "{e}");
}

#[test]
fn top_level_values_are_pure() {
    let e = error("(def started (now!))");
    assert!(e.contains("this uses `clock`, where only pure code is allowed"), "{e}");
}

#[test]
fn elements_stay_pure() {
    // An element's `update` returns effects as data. One that used a
    // capability itself could not be called `update`, so the element
    // contract never sees it.
    let e = error(
        "(defn init [a] {:n 0})
         (defn update [m h e a] (let [_ (log! h)] {:model m :effects []}))
         (defn view [m a] (h \"p\" {} [] {}))",
    );
    assert!(e.contains("in `update`: it uses `log`, so its name must end with `!`"), "{e}");
}

/// Run `entry!` with the host functions linked: `log!` records its text,
/// `now!` answers 42.
fn run(src: &str, entry: &str) -> (Value, Vec<String>) {
    let mut cb = host();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let hash = cb.names[entry];
    let wat = codegen::compile_exports(&cb, &[(entry, hash)]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}\n{wat}"))).unwrap();
    let mut linker = Linker::<Vec<String>>::new(&engine);
    linker
        .func_wrap("conduct:host", "log!", |mut caller: Caller<'_, Vec<String>>, s: i64| -> i64 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let text = match decode(memory.data(&caller), s) {
                Value::Str(t) => t,
                other => panic!("log! got {other:?}"),
            };
            caller.data_mut().push(text);
            0x02
        })
        .unwrap();
    linker.func_wrap("conduct:host", "now!", || -> i64 { codegen::int(42) }).unwrap();
    let mut store = Store::new(&engine, Vec::new());
    let instance = linker.instantiate(&mut store, &module).unwrap();
    let f = instance.get_func(&mut store, entry).unwrap();
    let mut results = [Val::I64(0)];
    f.call(&mut store, &[Val::I64(codegen::int(0))], &mut results).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let value = decode(memory.data(&store), results[0].unwrap_i64());
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, results[0].unwrap_i64()).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "{entry} leaked");
    (value, std::mem::take(store.data_mut()))
}

#[test]
fn effects_run_directly() {
    let src = "(defn stamp! [msg] (let [t (now!)] (log! (str t \": \" msg))))
               (defn main! [n] (let [_ (stamp! \"start\")
                                     xs (map (fn [x] (log! (str \"item \" x))) [1 2])]
                                 (count xs)))";
    let (value, logged) = run(src, "main!");
    assert_eq!(value, Value::Int(2));
    assert_eq!(logged, ["42: start", "item 1", "item 2"]);
}

#[test]
fn host_functions_can_take_optional_arguments() {
    // `t | nil` bounds what a parameter takes: a string, nil, or a value
    // that is one or the other, but nothing else.
    let mut cb = host();
    cb.add_host("title!", "model", vec![Ty::maybe(Ty::str())], Ty::maybe(Ty::int())).unwrap();
    assert_eq!(cb.lookup("title!").unwrap().type_string(), "(String | nil) => Int | nil ! {model}");
    let src = "(defn a! [] (title! \"x\"))
               (defn b! [] (title! nil))
               (defn c! [d] (title! (get d \"k\")))
               (defn d! [] (+ 1 (?? (title! nil) 0)))";
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cb.lookup("c!").unwrap().type_string(), "(Dict[String | nil]) => Int | nil ! {model}");
    let e = cb.add_source("(defn e! [] (title! 5))").unwrap_err().to_string();
    assert!(e.contains("Int is not a String"), "{e}");
    let e = cb.add_source("(defn f! [] (+ 1 (title! nil)))").unwrap_err().to_string();
    assert!(e.contains("nil is not a Int"), "{e}");
}
