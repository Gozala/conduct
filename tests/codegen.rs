//! Compile definitions to core Wasm and run them on Wasmtime.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode, name_fields};
use wasmtime::{Engine, Instance, Module, Store, Val};

fn run(src: &str, entry: &str, args: &[i64]) -> Value {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let hash = cb.names[entry];
    let wat = codegen::compile_exports(&cb, &[(entry, hash)]).unwrap();
    let wasm = wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}\n{wat}"));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_func(&mut store, entry).unwrap();
    let params: Vec<Val> = args.iter().map(|a| Val::I64(*a)).collect();
    let mut results = [Val::I64(0)];
    f.call(&mut store, &params, &mut results).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    decode(memory.data(&store), results[0].unwrap_i64())
}

fn int(n: i64) -> i64 {
    codegen::int(n)
}

#[test]
fn arithmetic_and_recursion() {
    let src = "(defn fact [n] (if (< n 1) 1 (* n (fact (- n 1)))))";
    assert_eq!(run(src, "fact", &[int(10)]), Value::Int(3628800));
    assert_eq!(run("(def x (quot (- 7 20) 2))", "x", &[]), Value::Int(-6));
}

#[test]
fn mutual_recursion() {
    let src = "(defn even? [n] (if (= n 0) true (odd? (- n 1))))
               (defn odd? [n] (if (= n 0) false (even? (- n 1))))";
    assert_eq!(run(src, "even?", &[int(10)]), Value::Bool(true));
    assert_eq!(run(src, "odd?", &[int(7)]), Value::Bool(true));
}

#[test]
fn closures_capture() {
    let src = "(defn adder [n] (fn [x] (+ x n)))
               (def xs (map (adder 10) [1 2 3]))";
    assert_eq!(run(src, "xs", &[]), Value::List(vec![Value::Int(11), Value::Int(12), Value::Int(13)]));
    let src = "(def ys (let [k 5 f (fn [x] (fn [y] (+ (+ x y) k)))] ((f 1) 2)))";
    assert_eq!(run(src, "ys", &[]), Value::Int(8));
}

#[test]
fn strings_and_optionals() {
    let src = "(def d {\"title\" \"Buy milk\"})
               (def t (str (?? (get d \"title\") \"untitled\") \" / \" (?? (get d \"nope\") \"none\") \" \" 42 \" \" true))";
    assert_eq!(run(src, "t", &[]), Value::Str("Buy milk / none 42 true".into()));
    let src = "(def n (if-some [x (get {\"a\" 2} \"a\")] (+ x 1) 0))";
    assert_eq!(run(src, "n", &[]), Value::Int(3));
}

#[test]
fn records_and_equality() {
    let src = "(def r {:b 2 :a \"x\"}) (def same (= r {:a \"x\" :b 2})) (def b (:b r))";
    assert_eq!(run(src, "same", &[]), Value::Bool(true));
    assert_eq!(run(src, "b", &[]), Value::Int(2));
    let v = run(src, "r", &[]);
    let mut fields = name_fields(v, &["a", "b"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("a".into(), Value::Str("x".into())), ("b".into(), Value::Int(2))]);
    assert_eq!(run("(def e (= [1 \"a\" nil] [1 \"a\" nil]))", "e", &[]), Value::Bool(true));
    assert_eq!(run("(def e (= {\"a\" 1 \"b\" 2} {\"b\" 2 \"a\" 1}))", "e", &[]), Value::Bool(true));
}

#[test]
fn lists_and_dicts() {
    let src = "(def xs (filter (fn [x] (> x 1)) (conj (concat [1 2] [3]) 4)))
               (def n (count (rest xs)))
               (def d (dissoc (assoc (assoc {} \"a\" 1) \"b\" 2) \"a\"))";
    assert_eq!(run(src, "xs", &[]), Value::List(vec![Value::Int(2), Value::Int(3), Value::Int(4)]));
    assert_eq!(run(src, "n", &[]), Value::Int(2));
    assert_eq!(run(src, "d", &[]), Value::Dict(vec![("b".into(), Value::Int(2))]));
}

#[test]
fn views() {
    let src = "(def v (h \"li\" {\"class\" \"done\"} [\"hi\" (h \"b\")] {\"click\" \"go\"}))";
    let Value::View { tag, attrs, children, events } = run(src, "v", &[]) else { panic!() };
    assert_eq!(tag, "li");
    assert_eq!(attrs, vec![("class".into(), Value::Str("done".into()))]);
    assert_eq!(children.len(), 2);
    assert_eq!(events, vec![("click".into(), Value::Str("go".into()))]);
}
