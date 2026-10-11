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

#[test]
fn json_round_trips() {
    let doc = r#"{\"todos\":[{\"id\":\"a\",\"title\":\"Buy \\\"oat\\\" milk \\u00e9\\ud83d\\ude00\",\"completed\":true}],\"counts\":{\"active\":2,\"completed\":-1.5e3}}"#;
    let src = format!(
        "(def doc (parse-json \"{doc}\"))
         (def first-title (json-str (json-get (first (json-items (json-get doc \"todos\"))) \"title\")))
         (def done (json-bool (json-get (first (json-items (json-get doc \"todos\"))) \"completed\")))
         (def active (json-int (json-get (json-get doc \"counts\") \"active\")))
         (def completed (json-int (json-get (json-get doc \"counts\") \"completed\")))
         (def missing (json-int (json-get doc \"nope\")))
         (def bad (json-str (parse-json \"{{\\\"a\\\": }}\")))
         (def encoded (to-json {{\"title\" \"say \\\"hi\\\"\\n\" \"completed\" false \"tags\" [1 nil true]}}))
         (def again (json-str (json-get (parse-json encoded) \"title\")))"
    );
    assert_eq!(run(&src, "first-title", &[]), Value::Str("Buy \"oat\" milk é😀".into()));
    assert_eq!(run(&src, "done", &[]), Value::Bool(true));
    assert_eq!(run(&src, "active", &[]), Value::Int(2));
    assert_eq!(run(&src, "completed", &[]), Value::Int(-1));
    assert_eq!(run(&src, "missing", &[]), Value::Int(0));
    assert_eq!(run(&src, "bad", &[]), Value::Str("".into()));
    assert_eq!(
        run(&src, "encoded", &[]),
        Value::Str(r#"{"title":"say \"hi\"\u000a","completed":false,"tags":[1,null,true]}"#.into())
    );
    assert_eq!(run(&src, "again", &[]), Value::Str("say \"hi\"\n".into()));
}

#[test]
fn split_strings() {
    let src = "(def parts (split \"7\\nBuy milk\\nnow\" \"\\n\"))
               (def whole (split \"abc\" \"\"))
               (def edges (split \",a,\" \",\"))";
    let s = |x: &str| Value::Str(x.into());
    assert_eq!(run(src, "parts", &[]), Value::List(vec![s("7"), s("Buy milk"), s("now")]));
    assert_eq!(run(src, "whole", &[]), Value::List(vec![s("abc")]));
    assert_eq!(run(src, "edges", &[]), Value::List(vec![s(""), s("a"), s("")]));
}

#[test]
fn keywords_and_match() {
    let src = r#"
(defn label [s] (match s :active "Active" :suspended "Paused"))
(defn handle [h n] (match h "inc" (+ n 1) "dec" (- n 1) :else n))
(def labels [(label :active) (label :suspended)])
(def handled [(handle "inc" 1) (handle "dec" 1) (handle "other" 1)])
(def shown (str :active " " (to-json [:done "x"])))
(def same [(= :a :a) (= :a :b) (= :a "a")])
(defn status [job] (match (:status job) :active (str "Since " (:since job)) :suspended "paused"))
(def described [(status {:status :active :since "May"}) (status {:status :suspended :since ""})])
(def kept (match {:k [1 2]} :x [] :else [3]))
"#;
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "labels", &[]), strs(&["Active", "Paused"]));
    assert_eq!(run(src, "handled", &[]), Value::List(vec![Value::Int(2), Value::Int(0), Value::Int(1)]));
    assert_eq!(run(src, "shown", &[]), Value::Str(r#":active ["done","x"]"#.into()));
    assert_eq!(run(src, "same", &[]), Value::List(vec![Value::Bool(true), Value::Bool(false), Value::Bool(false)]));
    assert_eq!(run(src, "described", &[]), strs(&["Since May", "paused"]));
    assert_eq!(run(src, "kept", &[]), Value::List(vec![Value::Int(3)]));
    assert_eq!(run("(def k :ok)", "k", &[]), Value::Keyword("ok".into()));
}

#[test]
fn matching_shapes() {
    let src = r#"
(defn since [job] (match (:started-at job) nil "not started" some? (str "Since " (:started-at job))))
(defn describe [job]
  (match job
    {:status :active :owner {:name "ann"}} "ann's"
    {:status :active} (str "Since " (:since job))
    {:status :suspended} "paused"
    :else "other"))
(def a [(since {:started-at nil}) (since {:started-at "May"})])
(def b [(describe {:status :active :since "May" :owner {:name "ann"}})
        (describe {:status :active :since "May" :owner {:name "bo"}})
        (describe {:status :suspended :since "" :owner {:name ""}})
        (describe {:status :gone :since "" :owner {:name ""}})])
"#;
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "a", &[]), strs(&["not started", "Since May"]));
    assert_eq!(run(src, "b", &[]), strs(&["ann's", "Since May", "paused", "other"]));
}
