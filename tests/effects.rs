//! Effects as tasks: calls are pure, a definition marked `!` makes a
//! task, `!` at a call performs it, a task's capabilities are inferred,
//! and an environment can provide them.

use conduct::codegen::{self, Value, decode};
use conduct::types::Ty;
use conduct::{Codebase, element};
use wasmtime::{Caller, Engine, Linker, Module, Store, Val};

/// A codebase with two host functions: `log` (the `log` capability) and
/// `now` (the `clock` capability, a task with no parameters).
fn host() -> Codebase {
    with_host(Codebase::new())
}

/// The same host functions, added to `cb`.
fn with_host(mut cb: Codebase) -> Codebase {
    cb.add_host("log", "log", vec![Ty::str()], Ty::record([])).unwrap();
    cb.add_host("now", "clock", vec![], Ty::int()).unwrap();
    cb
}

fn ty(src: &str, name: &str) -> String {
    let mut cb = host();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb.type_of(name).unwrap()
}

fn error(src: &str) -> String {
    host().add_source(src).unwrap_err().to_string()
}

#[test]
fn a_definition_marked_bang_makes_a_task() {
    assert_eq!(ty("(define x 1)", "x"), "integer");
    assert_eq!(ty("(define (greet! name) (log! (str \"hi \" name)))", "greet"), "((\\ any) : (task {} with: {log}))");
    // Capabilities add up, through `let`, `do`, and other tasks.
    let src = "(define (stamp! msg) (let [t (now!)] (log! (str t \": \" msg))))
               (define (twice! msg) (stamp! msg) (stamp! msg))";
    assert_eq!(ty(src, "twice"), "((\\ any) : (task {} with: {clock log}))");
    // A task of no parameters is the task itself.
    assert_eq!(ty("(define (later!) (log! \"later\"))", "later"), "(task {} with: {log})");
    // A host function is a function answering a task, or a task.
    assert_eq!(ty("(define f log)", "f"), "((\\ text) : (task {} with: {log}))");
    assert_eq!(ty("(define f now)", "f"), "(task integer with: {clock})");
}

#[test]
fn calls_are_pure() {
    // Calling without `!` answers the task and performs nothing, so a
    // list of tasks needs no effect polymorphism.
    assert_eq!(ty("(define (all xs) (map xs each: log))", "all"), "((\\ (list text)) : (list (task {} with: {log})))");
    assert_eq!(
        ty("(define (logger prefix) (fn [m] (log (str prefix m))))", "logger"),
        "((\\ any) : ((\\ any) : (task {} with: {log})))"
    );
    // A task held in a value is performed with `(t!)`.
    let src = "(define (run! t) (t!))
               (define (both! a b) (a!) (b!))";
    assert_eq!(ty(src, "run"), "((\\ (task ?a with: ?b)) : (task ?a with: ?b))");
    assert_eq!(ty(src, "both"), "((\\ (task any with: ?a) b: (task ?b with: ?a)) : (task ?b with: ?a))");
}

#[test]
fn only_a_task_performs() {
    let e = error("(define (greet name) (log! name))");
    assert!(e.contains("`!` performs, and only a task's body may"), "{e}");
    // A function inside a task is a function: its body is pure too.
    let e = error("(define (each! xs) (map xs each: (fn [x] (log! x))))");
    assert!(e.contains("only a task's body may"), "{e}");
    // `fn!` makes one that performs.
    assert_eq!(
        ty("(define (each xs) (map xs each: (fn! [x] (log! x))))", "each"),
        "((\\ (list text)) : (list (task {} with: {log})))"
    );
    let e = error("(define started (now!))");
    assert!(e.contains("only a task's body may"), "{e}");
    let e = error("(define x log!)");
    assert!(e.contains("as a value this is `log`"), "{e}");
    let e = error("(define (f x) (x!))");
    assert!(e.contains("only a task's body may"), "{e}");
}

#[test]
fn elements_stay_pure() {
    // An element's `update` returns effects as data; it cannot perform.
    let e = with_host(element::codebase())
        .add_source(
            "(type n : (t/n integer))
             (define (init attrs) {n: 0})
             (define (update model handler event attrs) (do (log! handler) {model: model, effects: []}))
             (define (view model attrs) (h \"p\" {} [] {}))",
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("in `update`") && e.contains("only a task's body may"), "{e}");
}

#[test]
fn claims_of_tasks() {
    let mut cb = host();
    cb.add_source("(claim (greet! text) with: {log} : {}) (define (greet! who) (log! who))").unwrap();
    assert_eq!(cb.type_of("greet").unwrap(), "((\\ text) : (task {} with: {log}))");
    let e =
        cb.add_source("(claim (quiet! text) with: {} : {}) (define (quiet! who) (log! who))").unwrap_err().to_string();
    assert!(e.contains("this uses `log`, where only pure code is allowed"), "{e}");
    // Left out, the capabilities are what the body performs.
    cb.add_source("(claim (hi! text) : {}) (define (hi! who) (log! who))").unwrap();
    assert_eq!(cb.type_of("hi").unwrap(), "((\\ text) : (task {} with: {log}))");
    let e = cb.add_source("(claim (plain text) : {}) (define (plain! who) (log! who))").unwrap_err().to_string();
    assert!(e.contains("the definition is marked `!`, so its claim is too"), "{e}");
    let e = cb.add_source("(claim (f text) with: {log} : {}) (define (f x) {})").unwrap_err().to_string();
    assert!(e.contains("only a task needs capabilities"), "{e}");
}

#[test]
fn an_environment_provides_capabilities() {
    let src = "(define (stamp! msg) (let [t (now!)] (log! (str t \": \" msg))))";
    // Providing `clock` leaves `log`.
    assert_eq!(
        ty(&format!("{src} (define (fixed msg) (perform (stamp msg) with: {{clock: {{now: 7}}}}))"), "fixed"),
        "((\\ any) : (task {} with: {log}))"
    );
    let e = error(&format!("{src} (define (f msg) (perform (stamp msg) with: {{clock: {{}}}}))"));
    assert!(e.contains("an environment that provides `clock` implements `now`"), "{e}");
    let e = error(&format!("{src} (define (f msg) (perform (stamp msg) with: {{log: {{log: (fn [x] 7)}}}}))"));
    assert!(e.contains("the implementation of `log`"), "{e}");
    let e = error(&format!("{src} (define (f msg) (perform (stamp msg) with: {{disk: {{read: (fn [x] 7)}}}}))"));
    assert!(e.contains("no host function uses the capability `disk`"), "{e}");
}

/// Run `entry!`, given an Int, with the host functions linked: `log`
/// records its text, `now` answers 42.
fn run(src: &str, entry: &str) -> (Value, Vec<String>) {
    let mut cb = host();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let hash = cb.names[entry];
    let mut b = codegen::Builder::new(&cb);
    let call = b.perform(hash, &["(local.get $n)".into()]).unwrap();
    b.add_func(format!("  (func (export \"{entry}\") (param $n i64) (result i64) {call})"));
    let wat = codegen::finish_exports(b).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}\n{wat}"))).unwrap();
    let mut linker = Linker::<Vec<String>>::new(&engine);
    linker
        .func_wrap("conduct:host", "log", |mut caller: Caller<'_, Vec<String>>, s: i64| -> i64 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let text = match decode(memory.data(&caller), s) {
                Value::Str(t) => t,
                other => panic!("log got {other:?}"),
            };
            caller.data_mut().push(text);
            0x02
        })
        .unwrap();
    linker.func_wrap("conduct:host", "now", || -> i64 { codegen::int(42) }).unwrap();
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
fn tasks_run_in_order() {
    let src = "(define (stamp! msg) (let [t (now!)] (log! (str t \": \" msg))))
               (define (all! tasks)
                 (match (count tasks)
                   0 {}
                   _ (let [t (first tasks (log \"none\"))] (t!) (all! (rest tasks)))))
               (define (main! n)
                 (stamp! \"start\")
                 (let [tasks (map [1 2] each: (fn [x] (log (str \"item \" x))))]
                   (all! tasks)
                   (count tasks)))";
    let (value, logged) = run(src, "main");
    assert_eq!(value, Value::Int(2));
    assert_eq!(logged, ["42: start", "item 1", "item 2"]);
}

#[test]
fn a_task_held_is_performed_later() {
    // Making a task performs nothing; performing it twice does it twice.
    let src = "(define (main! n)
                 (let [t (log \"once\")
                       u (log \"never\")]
                   (t!)
                   (t!)
                   n))";
    let (value, logged) = run(src, "main");
    assert_eq!(value, Value::Int(0));
    assert_eq!(logged, ["once", "once"]);
}

#[test]
fn an_environment_answers_in_the_host_place() {
    let src = "(define (stamp! msg) (let [t (now!)] (log! (str t \": \" msg))))
               (define (main! n)
                 (perform! (stamp \"fixed\") with: {clock: {now: 7}})
                 (perform! (stamp \"quiet\") with: {log: {log: (fn [line] {})}})
                 (stamp! \"real\")
                 n)";
    let (_, logged) = run(src, "main");
    assert_eq!(logged, ["7: fixed", "42: real"]);
}

#[test]
fn host_functions_take_optional_attributes() {
    // A host function with optional parameters takes one record of them:
    // an attribute may be left out, but if it is there it must fit.
    let mut cb = host();
    let params = Ty::record([("t/id".to_string(), Ty::str()), ("t/title".to_string(), Ty::opt(Ty::str()))]);
    cb.add_host("rename", "model", vec![params], Ty::record([])).unwrap();
    assert_eq!(
        cb.lookup("rename").unwrap().type_string(),
        "((\\ {t/id: text, t/title?: text}) : (task {} with: {model}))"
    );
    // The attributes are claimed with unconstrained values, so only the
    // host function's type constrains them.
    let src = "(type id : (t/id ?id))
               (type title : (t/title ?title))
               (define (a! i) (rename! {id: i, title: \"y\"}))
               (define (b! i) (rename! {id: i}))
               (define (c! t) (rename! (if (= t \"\") {id: \"x\"} (assoc {id: \"x\"} title: t))))";
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let e = cb.add_source("(define (d! i) (rename! {id: i, title: 5}))").unwrap_err().to_string();
    assert!(e.contains("integer is not a text"), "{e}");
    let e = cb.add_source("(define (e! t) (rename! {title: t}))").unwrap_err().to_string();
    assert!(e.contains("has no attribute `t/id`"), "{e}");
}

#[test]
fn implementations_are_pure_and_given_once() {
    let src = "(define (stamp! msg) (let [t (now!)] (log! (str t \": \" msg))))";
    // An implementation that performs would run in the environment it
    // belongs to.
    let e = error(&format!("{src} (define (main! n) (perform! (stamp \"x\") with: {{clock: {{now: (now!)}}}}) n)"));
    assert!(e.contains("only a task's body may"), "{e}");
    let e =
        error(&format!("{src} (define (f msg) (perform (stamp msg) with: {{clock: {{now: 1}}, clock: {{now: 2}}}}))"));
    assert!(e.contains("the capability `clock:` appears twice"), "{e}");
    let e = error(&format!("{src} (define (f msg) (perform (stamp msg) with: {{clock: {{now: 1, now: 2}}}}))"));
    assert!(e.contains("the operation `now:` appears twice"), "{e}");
    // An operation's implementation follows the host function, whatever
    // else is later named like it.
    let mut cb = host();
    cb.add_source(src).unwrap();
    cb.add_source("(define (now x) x)").unwrap();
    cb.add_source("(define (main! n) (perform! (stamp \"x\") with: {clock: {now: 7}}) n)").unwrap();
}

#[test]
fn a_call_with_no_arguments_is_the_value() {
    assert_eq!(ty("(define r (now))", "r"), "(task integer with: {clock})");
    assert_eq!(ty("(define (f x) x) (define g (f))", "g"), "((\\ ?a) : ?a)");
}
