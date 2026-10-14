//! conduct.parser, the port of elm/parser: what it parses, where it says
//! it failed, how `one-of` backtracks, and inputs long enough that a loop
//! must not grow the stack.

use conduct::codegen::{self, Value, decode, name_fields};
use conduct::{Codebase, library};
use wasmtime::{Engine, Instance, Module, Store};

fn codebase(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(library::PARSER).unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

/// Run `out`, give its value back, and check nothing it made is left.
fn run(src: &str) -> Value {
    let cb = codebase(src);
    let wat = codegen::compile_exports(&cb, &[("out", cb.names["out"])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, "out").unwrap();
    let out = f.call(&mut store, ()).unwrap_or_else(|e| panic!("{e:?}"));
    let value = decode(instance.get_memory(&mut store, "memory").unwrap().data(&store), out);
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, out).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "leaked");
    value
}

/// Every attribute these tests read, by its whole name.
const NAMES: &[&str] = &[
    "conduct.result/ok",
    "conduct.result/error",
    "conduct.parser/row",
    "conduct.parser/col",
    "conduct.parser/reason",
    "conduct.parser/context-stack",
    "conduct.parser/context",
];

/// A record's field by the end of its attribute's name.
fn field(v: &Value, name: &str) -> Value {
    let fields = name_fields(v.clone(), NAMES);
    fields.into_iter().find(|(n, _)| n.ends_with(name)).unwrap_or_else(|| panic!("no {name} in {v:?}")).1
}

/// The value of an `(ok v)`, or a panic showing the `(error ..)`.
fn ok(v: Value) -> Value {
    let fields = name_fields(v.clone(), NAMES);
    match &fields[..] {
        [(name, value)] if name == "conduct.result/ok" => value.clone(),
        _ => panic!("not ok: {fields:?}"),
    }
}

/// The dead ends of an `(error ..)`, each as `(row, col, problem)`.
fn dead_ends(v: Value) -> Vec<(i64, i64, Value)> {
    let Value::List(ends) = field(&v, "result/error") else { panic!("not an error: {v:?}") };
    ends.iter()
        .map(|end| {
            let (Value::Int(r), Value::Int(c)) = (field(end, "/row"), field(end, "/col")) else { panic!() };
            (r, c, field(end, "/reason"))
        })
        .collect()
}

fn ints(v: &[i64]) -> Value {
    Value::List(v.iter().map(|n| Value::Int(*n)).collect())
}

fn kw(k: &str) -> Value {
    Value::Keyword(k.into())
}

/// A point `(x, y)` of naturals, with spaces anywhere.
const POINT: &str = "
(define number (int :expecting-int invalid: :invalid))
(define point
  (parsing [_ (symbol \"(\") _ spaces x number _ spaces _ (symbol \",\") _ spaces y number _ spaces _ (symbol \")\")]
    (succeed [x y])))";

#[test]
fn a_pipeline_parses_what_it_binds() {
    assert_eq!(ok(run(&format!("{POINT} (define out (run point text: \"( 3, 42 )\"))"))), ints(&[3, 42]));
    // `skip` and `keep` are the pipeline without names.
    let src = format!(
        "{POINT} (define out (run (skip (symbol \"<\") then: (keep number ignoring: (symbol \">\"))) text: \"<7>\"))"
    );
    assert_eq!(ok(run(&src)), Value::Int(7));
}

#[test]
fn a_dead_end_says_where_and_why() {
    let ends = dead_ends(run(&format!("{POINT} (define out (run point text: \"(3,\\n  x)\"))")));
    assert_eq!(ends, vec![(2, 3, kw("expecting-int"))]);
    let ends = dead_ends(run(&format!("{POINT} (define out (run point text: \"(1.5, 2)\"))")));
    assert_eq!(ends, vec![(1, 2, kw("invalid"))]);
}

#[test]
fn one_of_backtracks_only_without_progress() {
    // `let` as a keyword does not match the start of `letter`, and the
    // dead ends of every branch that failed without progress are kept.
    let src = "
      (define word (get-chomped (chomp-while alpha?)))
      (define let-or-name
        (one-of [(skip (keyword \"let\" expecting: :let) then: (succeed :let))
                 (map-value (chomp-if digit? expecting: :digit) with: (fn [_] :digit))]))
      (define committed (one-of [(skip (symbol \"ab\") then: (symbol \"c\")) (succeed :fallback)]))
      (define undone (one-of [(backtrackable (skip (symbol \"ab\") then: (symbol \"c\"))) (succeed :fallback)]))
      (define out [(run let-or-name text: \"let\") (run let-or-name text: \"letter\")
                   (run committed text: \"abd\") (run undone text: \"abd\")])";
    let Value::List(results) = run(src) else { panic!() };
    assert_eq!(ok(results[0].clone()), kw("let"));
    assert_eq!(dead_ends(results[1].clone()), vec![(1, 1, kw("let")), (1, 1, kw("digit"))]);
    // Having consumed `ab`, the first branch's failure is final ...
    assert_eq!(dead_ends(results[2].clone()).len(), 1);
    // ... unless it is backtrackable.
    assert_eq!(ok(results[3].clone()), kw("fallback"));
}

#[test]
fn names_and_reserved_words() {
    let src = "
      (define name (variable lower? inner: (fn [c] (or (alpha-num? c) (= c 95))) reserved: [\"if\" \"then\"] expecting: :name))
      (define out [(run name text: \"foo_bar1 rest\") (run name text: \"then\") (run name text: \"Foo\")])";
    let Value::List(results) = run(src) else { panic!() };
    assert_eq!(ok(results[0].clone()), Value::Str("foo_bar1".into()));
    assert_eq!(dead_ends(results[1].clone()), vec![(1, 1, kw("name"))]);
    assert_eq!(dead_ends(results[2].clone()), vec![(1, 1, kw("name"))]);
}

#[test]
fn sequences_and_their_trailing_separators() {
    let src = format!(
        "{POINT}
         (define (list-of trailing)
           (sequence number open: (symbol \"[\") separator: (symbol \",\") close: (symbol \"]\") spaces: spaces trailing: trailing))
         (define out [(run (list-of :forbidden) text: \"[1, 2 ,3]\") (run (list-of :forbidden) text: \"[]\")
                      (run (list-of :forbidden) text: \"[1,2,]\") (run (list-of :optional) text: \"[1,2,]\")
                      (run (list-of :optional) text: \"[1,2]\") (run (list-of :mandatory) text: \"[1,2,]\")
                      (run (list-of :mandatory) text: \"[1,2]\")])"
    );
    let Value::List(r) = run(&src) else { panic!() };
    assert_eq!(ok(r[0].clone()), ints(&[1, 2, 3]));
    assert_eq!(ok(r[1].clone()), ints(&[]));
    assert!(!dead_ends(r[2].clone()).is_empty(), "a trailing separator is forbidden");
    assert_eq!(ok(r[3].clone()), ints(&[1, 2]));
    assert_eq!(ok(r[4].clone()), ints(&[1, 2]));
    assert_eq!(ok(r[5].clone()), ints(&[1, 2]));
    assert!(!dead_ends(r[6].clone()).is_empty(), "a trailing separator is mandatory");
}

#[test]
fn recursion_through_a_function_of_the_state() {
    // Nested lists of naturals: `value` refers to itself, as a function of
    // the parse state, where elm/parser would use `lazy`.
    let src = format!(
        "{POINT}
         (define (value s)
           ((one-of [(map-value number with: (fn [n] [n]))
                     (map-value (sequence value open: (symbol \"[\") separator: (symbol \",\") close: (symbol \"]\")
                                          spaces: spaces trailing: :forbidden)
                                with: (fn [items] (flatten items)))]) s))
         (define (flatten items) (match items [] [] [x rest ...] (concat x (flatten rest))))
         (define out (run value text: \"[1, [2, [3, 4]], [], 5]\"))"
    );
    assert_eq!(ok(run(&src)), ints(&[1, 2, 3, 4, 5]));
}

#[test]
fn comments_positions_and_context() {
    let src = "
      (define after-comment
        (skip (multi-comment \"/*\" close: \"*/\") then: (skip (line-comment \"//\") then: (skip spaces then: get-position))))
      (define in-a-list (in-context (symbol \"]\") context: :list))
      (define out [(run after-comment text: \"/* a\\nbc */// rest\\n  x\") (run in-a-list text: \"x\")
                   (run (chomp-until \"*/\" expecting: :unclosed) text: \"/* a\\nb\")])";
    let Value::List(r) = run(src) else { panic!() };
    // Past the comments and spaces: row 3, column 3.
    let pos = ok(r[0].clone());
    assert_eq!((field(&pos, "/row"), field(&pos, "/col")), (Value::Int(3), Value::Int(3)));
    // A dead end carries the context it was in.
    let Value::List(ends) = field(&r[1], "result/error") else { panic!() };
    let Value::List(stack) = field(&ends[0], "/context-stack") else { panic!() };
    assert_eq!(stack.len(), 1);
    assert_eq!(field(&stack[0], "/context"), kw("list"));
    // An unclosed comment fails at the end of the source.
    assert_eq!(dead_ends(r[2].clone()), vec![(2, 2, kw("unclosed"))]);
}

#[test]
fn a_long_input_does_not_grow_the_stack() {
    let items: Vec<String> = (0..20_000).map(|n| (n % 10).to_string()).collect();
    let src = format!(
        "{POINT}
         (define list-of (sequence number open: (symbol \"[\") separator: (symbol \",\") close: (symbol \"]\") spaces: spaces trailing: :forbidden))
         (define out (count (match (run list-of text: \"[{}]\") (ok xs) xs (error _) [])))",
        items.join(",")
    );
    assert_eq!(run(&src), Value::Int(20_000));
}
