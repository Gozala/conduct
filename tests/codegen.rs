//! Compile definitions to core Wasm and run them on Wasmtime.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode, name_fields};
use wasmtime::{Engine, Instance, Module, Store, Val};

/// Compile `entry`, call it, decode the result, then give the result
/// back and check that every block the call allocated was freed.
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
    f.call(&mut store, &params, &mut results).unwrap_or_else(|e| panic!("{e:?}\n{wat}"));
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let value = decode(memory.data(&store), results[0].unwrap_i64());
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, results[0].unwrap_i64()).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "blocks leaked by {entry}");
    value
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

#[test]
fn ownership_edge_cases() {
    // A list used in one branch only, an unused parameter, and an
    // unused binding: each must be freed exactly once.
    let src = "(defn pick [b xs ys] (if b (count xs) (count ys)))
               (defn ignore [x y] 1)
               (def a (pick true [\"a\" \"b\"] [\"c\"]))
               (def b (ignore [1 2] (let [unused [3 4]] \"kept\")))";
    assert_eq!(run(src, "a", &[]), Value::Int(2));
    assert_eq!(run(src, "b", &[]), Value::Int(1));

    // A value shared by several uses and a closure that outlives it.
    let src = "(defn twice [xs] (concat xs xs))
               (defn keep [xs] (fn [ys] (concat xs ys)))
               (def c (count (twice (twice [\"x\" \"y\"]))))
               (def d ((keep [\"l\"]) [\"r\"]))";
    assert_eq!(run(src, "c", &[]), Value::Int(8));
    assert_eq!(run(src, "d", &[]), Value::List(vec![Value::Str("l".into()), Value::Str("r".into())]));

    // Optionals: the bound value is dropped when its arm ignores it,
    // and the fallback's values are dropped when it is not taken.
    let src = "(defn f [d fallback] (?? (get d \"k\") (str fallback \"!\")))
               (defn g [d] (if-some [x (get d \"k\")] 1 0))
               (def e (f {\"k\" \"v\"} \"unused\"))
               (def k (g {\"k\" [\"heap\"]}))";
    assert_eq!(run(src, "e", &[]), Value::Str("v".into()));
    assert_eq!(run(src, "k", &[]), Value::Int(1));

    // Short circuits drop what the skipped side would have consumed.
    let src = "(defn both [a xs] (and a (= (count xs) 2)))
               (def i (both false [\"p\" \"q\"]))";
    assert_eq!(run(src, "i", &[]), Value::Bool(false));

    // Recursion building and tearing down a structure.
    let src = "(defn range [n acc] (if (= n 0) acc (range (- n 1) (conj acc (str n)))))
               (def j (count (filter (fn [s] (not= s \"3\")) (range 50 []))))";
    assert_eq!(run(src, "j", &[]), Value::Int(49));
}

#[test]
fn record_update_and_trim() {
    let src = "(defn bump [m] (assoc m :n (+ (:n m) 1)))
               (def r (bump (bump {:n 0 :label \"x\"})))
               (def added (assoc {:b 2} :a \"new\"))
               (def t (trim \"  padded \\n\"))
               (def same (= (assoc {:a 1 :b 2} :a 1) {:b 2 :a 1}))";
    let mut fields = name_fields(run(src, "r", &[]), &["n", "label"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("label".into(), Value::Str("x".into())), ("n".into(), Value::Int(2))]);
    let mut fields = name_fields(run(src, "added", &[]), &["a", "b"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("a".into(), Value::Str("new".into())), ("b".into(), Value::Int(2))]);
    assert_eq!(run(src, "t", &[]), Value::Str("padded".into()));
    assert_eq!(run(src, "same", &[]), Value::Bool(true));
}
