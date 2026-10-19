//! Methods: a claim with no definition, each type's definition of it in
//! its own domain, and calls that find it by the type of what they are
//! called on, at compile time.

use conduct::Codebase;
use conduct::codegen::{self, Value, decode};
use wasmtime::{Engine, Instance, Module, Store};

/// The method this file's examples call, and `thing`, a variant that
/// defines it by what it holds.
const ORD: &str = "
(domain ord
  (claim (compare ?a vs: ?a) : integer))

(domain demo.thing
  (type (thing ?a) : {(./thing ?a)})
  (claim (content (thing ?a)) : ?a)
  (define (content (thing of)) of)
  (define (compare a vs: b) (ord/compare (content a) vs: (content b))))

(domain conduct.natural
  (define (compare a vs: b) (conduct/compare a b)))";

fn codebase(sources: &[&str]) -> Codebase {
    let mut cb = Codebase::new();
    for s in sources {
        cb.add_source(s).unwrap_or_else(|e| panic!("{e}"));
    }
    cb
}

fn error(sources: &[&str]) -> String {
    let mut cb = Codebase::new();
    let (last, first) = sources.split_last().unwrap();
    for s in first {
        cb.add_source(s).unwrap_or_else(|e| panic!("{e}"));
    }
    match cb.add_source(last) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.to_string(),
    }
}

fn run(cb: &Codebase) -> Value {
    let wat = codegen::compile_exports(cb, &[("out", cb.names["out"])]).unwrap();
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

fn ints(v: Value) -> Vec<i64> {
    let Value::List(items) = v else { panic!("{v:?}") };
    items.into_iter().map(|v| if let Value::Int(n) = v { n } else { panic!("{v:?}") }).collect()
}

#[test]
fn a_call_finds_the_definition_for_the_type_it_is_called_on() {
    let cb = codebase(&[
        ORD,
        "(define out [(ord/compare (thing 1) vs: (thing 2))
                      (ord/compare (thing 7) vs: (thing 7))
                      (ord/compare 9 vs: 3)])",
    ]);
    assert_eq!(ints(run(&cb)), [-1, 0, 1]);
}

#[test]
fn a_definition_that_calls_a_method_on_what_it_holds_asks_for_it() {
    let cb = codebase(&[ORD]);
    assert_eq!(
        cb.type_of("demo.thing/compare").unwrap(),
        "((\\ (thing ?a) vs: (thing ?a)) : integer) where ?a has ord/compare"
    );
}

#[test]
fn a_function_of_a_type_its_callers_say_is_passed_the_method() {
    let cb = codebase(&[
        ORD,
        "(define (smaller a than) (if (< (ord/compare a vs: than) 0) a than))
         (define out [(content (smaller (thing 5) than: (thing 3))) (smaller 4 than: 8)])",
    ]);
    assert_eq!(cb.type_of("smaller").unwrap(), "((\\ ?a than: ?a) : ?a) where ?a has ord/compare");
    assert_eq!(ints(run(&cb)), [3, 4]);
}

#[test]
fn a_claim_says_the_type_a_method_is_needed_for() {
    let cb = codebase(&[
        ORD,
        "(claim (smaller ?a than: ?a) : ?a)
         (define (smaller a than) (if (< (ord/compare a vs: than) 0) a than))
         (define out [(content (smaller (thing 5) than: (thing 3)))])",
    ]);
    assert_eq!(cb.type_of("smaller").unwrap(), "((\\ ?a than: ?a) : ?a) where ?a has ord/compare");
    assert_eq!(ints(run(&cb)), [3]);
}

#[test]
fn a_recursive_function_passes_on_what_it_is_passed() {
    let cb = codebase(&[
        ORD,
        "(define (insert x into)
           (match into
             [] [x]
             [y rest ...] (if (<= (ord/compare x vs: y) 0) (concat [x] into) (concat [y] (insert x into: rest)))))
         (define (sorted xs)
           (match xs
             [] []
             [x rest ...] (insert x into: (sorted rest))))
         (define out (map (sorted [(thing 3) (thing 1) (thing 2)]) each: (fn [t] (content t))))",
    ]);
    assert_eq!(cb.type_of("sorted").unwrap(), "((\\ (list ?a)) : (list ?a)) where ?a has ord/compare");
    assert_eq!(ints(run(&cb)), [1, 2, 3]);
}

#[test]
fn methods_nest_as_the_types_do() {
    // A thing of things of naturals: each level's method needs the next.
    let cb = codebase(&[
        ORD,
        "(define out [(ord/compare (thing (thing 2)) vs: (thing (thing 1)))
                      (ord/compare (thing (thing (thing 1))) vs: (thing (thing (thing 1))))])",
    ]);
    assert_eq!(ints(run(&cb)), [1, 0]);
}

#[test]
fn a_method_of_a_recursive_type_calls_itself() {
    let cb = codebase(&[
        ORD,
        "(domain demo.tree
           (type label : (demo.tree/label ?l))
           (type kids : (demo.tree/kids ?k))
           (type (tree ?a) : {(./tree {label: ?a, kids: (list (tree ?a))})})
           (claim (compare (tree ?a) vs: (tree ?a)) : integer)
           (define (compare a vs: b)
             (match a
               (tree {label: x kids: xs})
               (match b
                 (tree {label: y kids: ys})
                 (let [c (ord/compare x vs: y)]
                   (if (not= c 0) c
                     (match xs
                       [] (match ys [] 0 [_ _ ...] -1)
                       [p _ ...] (match ys [] 1 [q _ ...] (ord/compare p vs: q)))))))))
         (define (leaf n) (tree {label: n, kids: []}))
         (define (node n kids) (tree {label: n, kids: kids}))
         (define out [(ord/compare (node 1 kids: [(leaf 2)]) vs: (node 1 kids: [(leaf 3)]))
                      (ord/compare (leaf 1) vs: (node 1 kids: [(leaf 3)]))
                      (ord/compare (node 1 kids: [(leaf 4)]) vs: (node 1 kids: [(leaf 3)]))])",
    ]);
    assert_eq!(ints(run(&cb)), [-1, -1, 1]);
}

#[test]
fn a_definition_that_asks_for_methods_is_a_value_too() {
    let cb = codebase(&[
        ORD,
        "(define (same? a) (= (ord/compare a vs: a) 0))
         (define (insert x into)
           (match into
             [] [x]
             [y rest ...] (if (<= (ord/compare x vs: y) 0) (concat [x] into) (concat [y] (insert x into: rest)))))
         (define (sort-with xs by)
           (match xs
             [] []
             [x rest ...] (let [sorted (sort-with rest by: by)] (by x into: sorted))))
         (define out [(count (filter [(thing 1) (thing 2)] where: (fn [t] (same? t))))
                      (count (map [(thing 1)] each: same?))
                      (content (first (sort-with [(thing 2) (thing 1)] by: insert) (thing 0)))])",
    ]);
    assert_eq!(ints(run(&cb)), [2, 1, 1]);
}

#[test]
fn a_method_passed_as_a_value_is_the_definition_for_its_type() {
    let cb = codebase(&[
        ORD,
        "(define (sort-with xs by)
           (match xs
             [] []
             [x rest ...] (let [s (sort-with rest by: by)]
                            (concat (filter s where: (fn [y] (< (by y vs: x) 0)))
                                    (concat [x] (filter s where: (fn [y] (>= (by y vs: x) 0))))))))
         (define out (map (sort-with [(thing 3) (thing 1) (thing 2)] by: ord/compare) each: (fn [t] (content t))))",
    ]);
    assert_eq!(ints(run(&cb)), [1, 2, 3]);
}

#[test]
fn which_definition_a_call_finds_is_part_of_its_hash() {
    let by = |body: &str| {
        let thing = format!(
            "(domain ord (claim (compare ?a vs: ?a) : integer))
             (domain demo.thing
               (type (thing ?a) : {{(./thing ?a)}})
               (define (compare a vs: b) {body}))"
        );
        let cb = codebase(&[&thing, "(define out (ord/compare (thing 1) vs: (thing 2)))"]);
        cb.names["out"]
    };
    assert_ne!(by("1"), by("2"));
    assert_eq!(by("1"), by("1"));
}

#[test]
fn a_number_s_method_is_looked_for_in_what_it_is_a_view_of() {
    // A byte is a natural: with no `conduct.byte/compare`, a byte is
    // compared as a natural. An integer is not one.
    let cb = codebase(&[ORD, "(define out [(ord/compare 200 vs: 100)])"]);
    assert_eq!(ints(run(&cb)), [1]);
    let e = error(&[ORD, "(define out (ord/compare -1 vs: 3))"]);
    assert!(
        e.contains("`integer` has no `compare`: `ord/compare` is defined for it as `conduct.integer/compare`"),
        "{e}"
    );
}

#[test]
fn what_cannot_be_found_says_why() {
    let e = error(&[ORD, "(define out (ord/compare {} vs: {}))"]);
    assert!(e.contains("`ord/compare` is called on `{}`, which has no methods"), "{e}");
    let e = error(&[
        ORD,
        "(type (box ?a) : {(demo.box/box ?a)})
         (define (pick c) (ord/compare (if c (thing 1) (box 2)) vs: (thing 1)))",
    ]);
    assert!(e.contains("one of `(box ..)`, `(thing ..)`"), "{e}");
    let e = error(&[ORD, "(type (box ?a) : {(demo.box/box ?a)}) (define out (ord/compare (box 1) vs: (box 2)))"]);
    assert!(e.contains("`(box ..)` has no `compare`: `ord/compare` is defined for it as `demo.box/compare`"), "{e}");
    let e = error(&[
        ORD,
        "(domain demo.box
           (type (box ?a) : {(./box ?a)})
           (define (compare a vs: b) \"no\"))
         (define out (ord/compare (box 1) vs: (box 2)))",
    ]);
    assert!(e.contains("`demo.box/compare` does not fit the claim of `ord/compare`"), "{e}");
    let e = error(&[
        ORD,
        "(domain demo.box
           (type (box ?a) : {(./box ?a)})
           (define (compare a with) 0))
         (define out (ord/compare (box 1) vs: (box 2)))",
    ]);
    assert!(e.contains("`demo.box/compare` takes `with:`, and the claim of `ord/compare` `vs:`"), "{e}");
    let e = error(&[ORD, "(define (both x) (let [f (fn [a] (ord/compare a vs: a))] (f x)))"]);
    assert!(e.contains("inside a function a `let` binds"), "{e}");
}

#[test]
fn nothing_needs_no_method() {
    // An empty list holds nothing to compare: no caller is asked, and
    // the call that would compare is never made.
    let cb = codebase(&[
        ORD,
        "(define (insert x into)
           (match into
             [] [x]
             [y rest ...] (if (<= (ord/compare x vs: y) 0) (concat [x] into) (concat [y] (insert x into: rest)))))
         (define (sorted xs)
           (match xs
             [] []
             [x rest ...] (insert x into: (sorted rest))))
         (define out [(count (sorted []))])",
    ]);
    assert_eq!(ints(run(&cb)), [0]);
}

#[test]
fn a_type_that_holds_itself_needs_its_claim() {
    let tree = "(domain demo.tree
       (type label : (demo.tree/label ?l))
       (type kids : (demo.tree/kids ?k))
       (type (tree ?a) : {(./tree {label: ?a, kids: (list (tree ?a))})})
       (define (compare a vs: b)
         (match a
           (tree {label: x kids: xs})
           (match b
             (tree {label: y kids: ys})
             (match xs
               [] (ord/compare x vs: y)
               [p _ ...] (match ys [] 1 [q _ ...] (ord/compare p vs: q)))))))";
    let cb = codebase(&[ORD, tree]);
    assert_eq!(
        cb.type_of("demo.tree/compare").unwrap(),
        "((\\ {tree: {kids: (list ?b), label: ?a}} vs: {tree: {kids: (list ?b), label: ?a}}) : integer) where ?a has \
         ord/compare, ?b has ord/compare"
    );
    // Finite trees are compared without it: what each level holds is
    // known to the level below, down to an empty list.
    let cb = codebase(&[
        ORD,
        tree,
        "(define out [(ord/compare (tree {label: 1, kids: [(tree {label: 2, kids: []})]})
                                   vs: (tree {label: 1, kids: [(tree {label: 3, kids: []})]}))])",
    ]);
    assert_eq!(ints(run(&cb)), [-1]);
    // A tree of any depth is of the type that holds itself.
    let e = error(&[
        ORD,
        tree,
        "(claim (single natural) : (tree natural))
         (define (single n) (tree {label: n, kids: []}))
         (define out (ord/compare (single 1) vs: (single 2)))",
    ]);
    assert!(e.contains("needs its claim"), "{e}");
}

#[test]
fn what_a_host_calls_is_passed_no_methods() {
    let cb = codebase(&[ORD, "(define (smaller a than) (if (< (ord/compare a vs: than) 0) a than))"]);
    let e = codegen::compile_exports(&cb, &[("smaller", cb.names["smaller"])]).unwrap_err().to_string();
    assert!(e.contains("`smaller` asks for `ord/compare`, and what calls an export passes no methods"), "{e}");
}
