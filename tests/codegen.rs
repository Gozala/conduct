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
    let src = "(define (fact n) (if (< n 1) 1 (* n (fact (- n 1)))))";
    assert_eq!(run(src, "fact", &[int(10)]), Value::Int(3628800));
    assert_eq!(run("(define x (quot (- 7 20) 2))", "x", &[]), Value::Int(-6));
}

#[test]
fn mutual_recursion() {
    let src = "(define (even? n) (if (= n 0) true (odd? (- n 1))))
               (define (odd? n) (if (= n 0) false (even? (- n 1))))";
    assert_eq!(run(src, "even?", &[int(10)]), Value::Bool(true));
    assert_eq!(run(src, "odd?", &[int(7)]), Value::Bool(true));
}

#[test]
fn closures_capture() {
    let src = "(define (adder n) (fn [x] (+ x n)))
               (define xs (map [1 2 3] each: (adder 10)))";
    assert_eq!(run(src, "xs", &[]), Value::List(vec![Value::Int(11), Value::Int(12), Value::Int(13)]));
    let src = "(define ys (let [k 5 f (fn [x] (fn [y] (+ (+ x y) k)))] ((f 1) 2)))";
    assert_eq!(run(src, "ys", &[]), Value::Int(8));
}

#[test]
fn strings_and_defaults() {
    let src = "(define d {\"title\" \"Buy milk\"})
               (define t (str (get d \"title\" \"untitled\") \" / \" (get d \"nope\" \"none\") \" \" 42 \" \" true))";
    assert_eq!(run(src, "t", &[]), Value::Str("Buy milk / none 42 true".into()));
    // An attribute read with a default, and a list's first with one.
    let src = "(claim title : (t/title ?title))
               (claim id : (t/id ?id))
               (define (title-of r) (title r \"untitled\"))
               (define n [(title-of {title: \"a\", id: 1}) (title-of {id: 2}) (first [] \"none\") (first [\"x\"] \"none\")])";
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "n", &[]), strs(&["a", "untitled", "none", "x"]));
}

#[test]
fn records_and_equality() {
    let src = "(claim a : (t/a ?a)) (claim b : (t/b ?b))
               (define r {b: 2, a: \"x\"}) (define same (= r {a: \"x\", b: 2})) (define bv (b r))";
    assert_eq!(run(src, "same", &[]), Value::Bool(true));
    assert_eq!(run(src, "bv", &[]), Value::Int(2));
    let v = run(src, "r", &[]);
    let mut fields = name_fields(v, &["t/a", "t/b"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("t/a".into(), Value::Str("x".into())), ("t/b".into(), Value::Int(2))]);
    assert_eq!(run("(define e (= [1 \"a\" :x] [1 \"a\" :x]))", "e", &[]), Value::Bool(true));
    assert_eq!(run("(define e (= {\"a\" 1 \"b\" 2} {\"b\" 2 \"a\" 1}))", "e", &[]), Value::Bool(true));
}

#[test]
fn lists_and_dicts() {
    let src = "(define xs (filter (conj (concat [1 2] [3]) 4) where: (fn [x] (> x 1))))
               (define n (count (rest xs)))
               (define d (dissoc (assoc (assoc {} \"a\" 1) \"b\" 2) \"a\"))";
    assert_eq!(run(src, "xs", &[]), Value::List(vec![Value::Int(2), Value::Int(3), Value::Int(4)]));
    assert_eq!(run(src, "n", &[]), Value::Int(2));
    assert_eq!(run(src, "d", &[]), Value::Dict(vec![("b".into(), Value::Int(2))]));
}

#[test]
fn views() {
    let src = "(define v (h \"li\" {\"class\" \"done\"} [\"hi\" (h \"b\")] {\"click\" \"go\"}))";
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
    let src = "(define (pick b xs ys) (if b (count xs) (count ys)))
               (define (ignore x y) 1)
               (define a (pick true xs: [\"a\" \"b\"] ys: [\"c\"]))
               (define b (ignore [1 2] y: (let [unused [3 4]] \"kept\")))";
    assert_eq!(run(src, "a", &[]), Value::Int(2));
    assert_eq!(run(src, "b", &[]), Value::Int(1));

    // A value shared by several uses and a closure that outlives it.
    let src = "(define (twice xs) (concat xs xs))
               (define (keep xs) (fn [ys] (concat xs ys)))
               (define c (count (twice (twice [\"x\" \"y\"]))))
               (define d ((keep [\"l\"]) [\"r\"]))";
    assert_eq!(run(src, "c", &[]), Value::Int(8));
    assert_eq!(run(src, "d", &[]), Value::List(vec![Value::Str("l".into()), Value::Str("r".into())]));

    // Defaults: a default not taken is dropped, and a bound attribute
    // its arm ignores is dropped.
    let src = "(claim k : (t/k ?k)) (claim n : (t/n ?n))
               (define (f d fallback) (get d \"k\" (str fallback \"!\")))
               (define (g r) (match r {k: x} 1 _ 0))
               (define (h r fallback) (k r (str fallback \"!\")))
               (define e (f {\"k\" \"v\"} fallback: \"unused\"))
               (define gk (g {k: [\"heap\"], n: 1}))
               (define m (h {k: \"v\", n: 1} fallback: \"unused\"))";
    assert_eq!(run(src, "e", &[]), Value::Str("v".into()));
    assert_eq!(run(src, "gk", &[]), Value::Int(1));
    assert_eq!(run(src, "m", &[]), Value::Str("v".into()));

    // Short circuits drop what the skipped side would have consumed.
    let src = "(define (both a xs) (and a (= (count xs) 2)))
               (define i (both false xs: [\"p\" \"q\"]))";
    assert_eq!(run(src, "i", &[]), Value::Bool(false));

    // Recursion building and tearing down a structure.
    let src = "(define (range n acc) (if (= n 0) acc (range (- n 1) acc: (conj acc (str n)))))
               (define j (count (filter (range 50 acc: []) where: (fn [s] (not= s \"3\")))))";
    assert_eq!(run(src, "j", &[]), Value::Int(49));
}

#[test]
fn record_update_and_trim() {
    let src = "(claim n : (t/n ?n)) (claim label : (t/label ?label))
               (claim a : (t/a ?a)) (claim b : (t/b ?b))
               (define (bump m) (assoc m n: (+ (n m) 1)))
               (define r (bump (bump {n: 0, label: \"x\"})))
               (define added (assoc {b: 2} a: \"new\"))
               (define t (trim \"  padded \\n\"))
               (define same (= (assoc {a: 1, b: 2} a: 1) {b: 2, a: 1}))";
    let mut fields = name_fields(run(src, "r", &[]), &["t/n", "t/label"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("t/label".into(), Value::Str("x".into())), ("t/n".into(), Value::Int(2))]);
    let mut fields = name_fields(run(src, "added", &[]), &["t/a", "t/b"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, vec![("t/a".into(), Value::Str("new".into())), ("t/b".into(), Value::Int(2))]);
    assert_eq!(run(src, "t", &[]), Value::Str("padded".into()));
    assert_eq!(run(src, "same", &[]), Value::Bool(true));
}

#[test]
fn json_round_trips() {
    let doc = r#"{\"todos\":[{\"id\":\"a\",\"title\":\"Buy \\\"oat\\\" milk \\u00e9\\ud83d\\ude00\",\"completed\":true}],\"counts\":{\"active\":2,\"completed\":-1.5e3}}"#;
    let src = format!(
        "(define doc (parse-json \"{doc}\"))
         (define first-title (json-str (json-get (first (json-items (json-get doc \"todos\")) doc) \"title\")))
         (define done (json-bool (json-get (first (json-items (json-get doc \"todos\")) doc) \"completed\")))
         (define active (json-int (json-get (json-get doc \"counts\") \"active\")))
         (define completed (json-int (json-get (json-get doc \"counts\") \"completed\")))
         (define missing (json-int (json-get doc \"nope\")))
         (define bad (json-str (parse-json \"{{\\\"a\\\": }}\")))
         (define encoded (to-json {{\"title\" \"say \\\"hi\\\"\\n\" \"completed\" false \"tags\" [1 (parse-json \"null\") true]}}))
         (define again (json-str (json-get (parse-json encoded) \"title\")))"
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
    let src = "(define parts (split \"7\\nBuy milk\\nnow\" \"\\n\"))
               (define whole (split \"abc\" \"\"))
               (define edges (split \",a,\" \",\"))";
    let s = |x: &str| Value::Str(x.into());
    assert_eq!(run(src, "parts", &[]), Value::List(vec![s("7"), s("Buy milk"), s("now")]));
    assert_eq!(run(src, "whole", &[]), Value::List(vec![s("abc")]));
    assert_eq!(run(src, "edges", &[]), Value::List(vec![s(""), s("a"), s("")]));
}

#[test]
fn keywords_and_match() {
    let src = r#"
(claim status : (t/status ?status))
(claim since : (t/since ?since))
(claim k : (t/k ?k))
(define (label s) (match s :active "Active" :suspended "Paused"))
(define (handle h n) (match h "inc" (+ n 1) "dec" (- n 1) _ n))
(define labels [(label :active) (label :suspended)])
(define handled [(handle "inc" n: 1) (handle "dec" n: 1) (handle "other" n: 1)])
(define shown (str :active " " (to-json [:done "x"])))
(define same [(= :a :a) (= :a :b) (= :a "a")])
(define (status-of job) (match (status job) :active (str "Since " (since job)) :suspended "paused"))
(define described [(status-of {status: :active, since: "May"}) (status-of {status: :suspended, since: ""})])
(define kept (match {k: [1 2]} :x [] _ [3]))
"#;
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "labels", &[]), strs(&["Active", "Paused"]));
    assert_eq!(run(src, "handled", &[]), Value::List(vec![Value::Int(2), Value::Int(0), Value::Int(1)]));
    assert_eq!(run(src, "shown", &[]), Value::Str(r#":active ["done","x"]"#.into()));
    assert_eq!(run(src, "same", &[]), Value::List(vec![Value::Bool(true), Value::Bool(false), Value::Bool(false)]));
    assert_eq!(run(src, "described", &[]), strs(&["Since May", "paused"]));
    assert_eq!(run(src, "kept", &[]), Value::List(vec![Value::Int(3)]));
    assert_eq!(run("(define k :ok)", "k", &[]), Value::Keyword("ok".into()));
}

#[test]
fn matching_shapes() {
    let src = r#"
(claim started-at : (t/started-at ?started-at))
(claim id : (t/id ?id))
(claim status : (t/status ?status))
(claim since : (t/since ?since))
(claim owner : (t/owner ?owner))
(claim name : (t/name ?name))
(define (since-of job) (match job {started-at: at} (str "Since " at) _ "not started"))
(define (describe job)
  (match job
    {status: :active, owner: {name: "ann"}} "ann's"
    {status: :active} (str "Since " (since job))
    {status: :suspended} "paused"
    {status: _} "other"))
(define a [(since-of {id: 1}) (since-of {id: 2, started-at: "May"})])
(define b [(describe {status: :active, since: "May", owner: {name: "ann"}})
        (describe {status: :active, since: "May", owner: {name: "bo"}})
        (describe {status: :suspended, since: "", owner: {name: ""}})
        (describe {status: :gone, since: "", owner: {name: ""}})])
"#;
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "a", &[]), strs(&["not started", "Since May"]));
    assert_eq!(run(src, "b", &[]), strs(&["ann's", "Since May", "paused", "other"]));
}

/// Call `entry` and also answer how many blocks it allocated.
fn run_counting(src: &str, entry: &str) -> (Value, i32) {
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let hash = cb.names[entry];
    let wat = codegen::compile_exports(&cb, &[(entry, hash)]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let allocs = instance.get_typed_func::<(), i32>(&mut store, "conduct_allocs").unwrap();
    let f = instance.get_func(&mut store, entry).unwrap();
    let mut results = [Val::I64(0)];
    f.call(&mut store, &[], &mut results).unwrap();
    let n = allocs.call(&mut store, ()).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let value = decode(memory.data(&store), results[0].unwrap_i64());
    (value, n)
}

#[test]
fn unique_values_are_updated_in_place() {
    // A thousand updates of a record held by nothing else: the record is
    // changed in place, so the loop allocates nothing per step.
    let src = "(claim n : (t/n ?n)) (claim tag : (t/tag ?tag))
               (define (bump m times) (if (= times 0) m (bump (assoc m n: (+ (n m) 1)) times: (- times 1))))
               (define r (bump {n: 0, tag: \"x\"} times: 1000))";
    let (value, allocs) = run_counting(src, "r");
    let mut fields = name_fields(value, &["t/n", "t/tag"]);
    fields.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(fields, [("t/n".into(), Value::Int(1000)), ("t/tag".into(), Value::Str("x".into()))]);
    assert!(allocs < 5, "{allocs} allocations");
    // A list built by `conj` grows in place, doubling when it is full.
    let src = "(define (build xs n) (if (= n 0) xs (build (conj xs n) n: (- n 1))))
               (define l (build [] n: 1000))";
    let (value, allocs) = run_counting(src, "l");
    let Value::List(items) = value else { panic!() };
    assert_eq!(items.len(), 1000);
    assert_eq!(items[0], Value::Int(1000));
    assert!(allocs < 30, "{allocs} allocations");
    // `map` and `filter` over a list nothing else holds reuse it.
    let src = "(define l (filter (map [1 2 3 4] each: (fn [x] (* x 2))) where: (fn [x] (> x 2))))";
    let (value, allocs) = run_counting(src, "l");
    assert_eq!(value, Value::List(vec![Value::Int(4), Value::Int(6), Value::Int(8)]));
    // The literal list and the two closures: nothing per element.
    assert_eq!(allocs, 3, "allocations");
}

#[test]
fn shared_values_are_copied() {
    // The original is still used after the update, so it must not change.
    let src = "(claim n : (t/n ?n))
               (define (both m) [(assoc m n: 2) m]) (define r (both {n: 1}))
               (define (grow xs) [(conj xs 3) xs]) (define l (grow [1 2]))
               (define (dbl xs) [(map xs each: (fn [x] (* x 2))) xs]) (define m (dbl [1 2]))
               (define (keep xs) [(filter xs where: (fn [x] (> x 1))) xs]) (define f (keep [1 2]))
               (define (put d) [(assoc d \"a\" 2) (dissoc d \"a\") d]) (define d (put {\"a\" 1}))";
    let ints = |v: &[i64]| Value::List(v.iter().map(|n| Value::Int(*n)).collect());
    let Value::List(r) = run(src, "r", &[]) else { panic!() };
    assert_eq!(name_fields(r[1].clone(), &["t/n"]), [("t/n".into(), Value::Int(1))]);
    assert_eq!(run(src, "l", &[]), Value::List(vec![ints(&[1, 2, 3]), ints(&[1, 2])]));
    assert_eq!(run(src, "m", &[]), Value::List(vec![ints(&[2, 4]), ints(&[1, 2])]));
    assert_eq!(run(src, "f", &[]), Value::List(vec![ints(&[2]), ints(&[1, 2])]));
    let Value::List(d) = run(src, "d", &[]) else { panic!() };
    assert_eq!(d[0], Value::Dict(vec![("a".into(), Value::Int(2))]));
    assert_eq!(d[1], Value::Dict(vec![]));
    assert_eq!(d[2], Value::Dict(vec![("a".into(), Value::Int(1))]));
}

#[test]
fn record_fields_are_evaluated_in_source_order() {
    // Fields are stored by id but evaluated in the order ownership was
    // worked out in; `j`'s last use must come last.
    let src = r#"(claim id : (t/id ?id)) (claim t : (t/t ?t))
                 (define (f j) {id: (json-str (json-get j "id")), t: (json-str (json-get j "t"))})
                 (define r (f (parse-json "{\"id\":\"a\"}")))"#;
    run(src, "r", &[]);
}

#[test]
fn variants_match_and_bind() {
    let src = r#"
(claim title : (t/title ?title))
(claim n : (t/n ?n))
(define (check t) (if (= t "") (error :empty-title) (ok {title: t, n: (count (split t " "))})))
(define (find id) (if (= id "x") (error :not-found) (check id)))
(define (show r)
  (match r
    (ok {title: t, n: n}) (str t " (" n ")")
    (error :not-found) "missing"
    (error e) (str "bad " e)))
(define (done x) (if x :ok (error "late")))
(define (status r) (match r :ok "fine" (error why) why))
(define a [(show (find "buy milk")) (show (find "x")) (show (find "")) (status (done true)) (status (done false))])
(define b (map [(find "a") (find "") :other {n: 1}] each: (fn [r] (match r (ok {title: t}) t _ "?"))))
"#;
    let strs = |v: &[&str]| Value::List(v.iter().map(|s| Value::Str(s.to_string())).collect());
    assert_eq!(run(src, "a", &[]), strs(&["buy milk (2)", "missing", "bad :empty-title", "fine", "late"]));
    assert_eq!(run(src, "b", &[]), strs(&["a", "?", "?", "?"]));
}
