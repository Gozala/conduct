//! conduct.route: one description parses a path into typed parameters
//! and formats them back, so links agree with routes.

use conduct::codegen::{self, Value, decode, name_fields};
use conduct::{Codebase, library};
use wasmtime::{Engine, Instance, Module, Store};

fn run(src: &str) -> Value {
    let mut cb = Codebase::new();
    cb.add_source(library::PARSER).unwrap();
    cb.add_source(library::ROUTE).unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
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

const NAMES: &[&str] = &["conduct.result/ok", "conduct.result/error", "calc/a", "calc/b", "blog/user", "blog/post"];

/// A record as `(name, value)` pairs, sorted by name.
fn fields(v: &Value) -> Vec<(String, Value)> {
    // `{}` is a record and a dictionary alike, and decodes as either.
    if matches!(v, Value::Dict(d) if d.is_empty()) {
        return vec![];
    }
    let mut fs = name_fields(v.clone(), NAMES);
    fs.sort_by(|a, b| a.0.cmp(&b.0));
    fs
}

fn ok(v: &Value) -> Vec<(String, Value)> {
    match &fields(v)[..] {
        [(n, inner)] if n == "conduct.result/ok" => fields(inner),
        other => panic!("not ok: {other:?}"),
    }
}

fn is_error(v: &Value) -> bool {
    matches!(&fields(v)[..], [(n, _)] if n == "conduct.result/error")
}

const ROUTES: &str = "
(type a : (calc/a natural)) (type b : (calc/b natural))
(type user : (blog/user text)) (type post : (blog/post natural))
(define calc (route \"/calculator/\" a \"/+/\" b))
(define posts (route \"/users/\" user \"/posts/\" (post natural-segment)))
(define home (route \"/\"))";

fn pair(name: &str, v: Value) -> (String, Value) {
    (name.to_string(), v)
}

#[test]
fn a_route_parses_its_parameters() {
    let out = run(&format!(
        "{ROUTES}
         (define out [(match-path calc path: \"/calculator/313/+/3\")
                      (match-path posts path: \"/users/ada/posts/7\")
                      (match-path home path: \"/\")])"
    ));
    let Value::List(r) = out else { panic!() };
    assert_eq!(ok(&r[0]), vec![pair("calc/a", Value::Int(313)), pair("calc/b", Value::Int(3))]);
    assert_eq!(ok(&r[1]), vec![pair("blog/post", Value::Int(7)), pair("blog/user", Value::Str("ada".into()))]);
    assert_eq!(ok(&r[2]), vec![]);
}

#[test]
fn a_path_it_does_not_describe_fails() {
    let out = run(&format!(
        "{ROUTES}
         (define out [(match-path calc path: \"/calculator/313/+/bob\")
                      (match-path calc path: \"/calculator/1/+/2/more\")
                      (match-path calc path: \"/calculator/1/-/2\")
                      (match-path posts path: \"/users//posts/1\")])"
    ));
    let Value::List(r) = out else { panic!() };
    assert!(r.iter().all(is_error), "{r:?}");
}

#[test]
fn a_route_formats_what_it_parses() {
    let out = run(&format!(
        "{ROUTES}
         (define link (format-path calc params: {{a: 313, b: 3}}))
         (define out [link (format-path posts params: {{user: \"ada\", post: 7}}) (format-path home params: {{}})
                      (= (match-path calc path: link) (ok {{a: 313, b: 3}}))])"
    ));
    assert_eq!(
        out,
        Value::List(vec![
            Value::Str("/calculator/313/+/3".into()),
            Value::Str("/users/ada/posts/7".into()),
            Value::Str("/".into()),
            Value::Bool(true),
        ])
    );
}

#[test]
fn a_router_answers_with_the_first_route_that_matches() {
    let out = run(&format!(
        "{ROUTES}
         (define routes [(on calc handler: (fn [p] (+ (a p) (b p))))
                         (on posts handler: (fn [p] (post p)))
                         (on home handler: (fn [_] 0))])
         (define out [(dispatch routes path: \"/calculator/2/+/40\") (dispatch routes path: \"/users/x/posts/9\")
                      (dispatch routes path: \"/\") (dispatch routes path: \"/nowhere\")])"
    ));
    let Value::List(r) = out else { panic!() };
    let value = |v: &Value| fields(v)[0].1.clone();
    assert_eq!(value(&r[0]), Value::Int(42));
    assert_eq!(value(&r[1]), Value::Int(9));
    assert_eq!(value(&r[2]), Value::Int(0));
    assert!(is_error(&r[3]));
}

#[test]
fn text_segments_are_percent_encoded() {
    let out = run(&format!(
        "{ROUTES}
         (define out [(match-path posts path: \"/users/J%C3%A9r%C3%B4me%20B/posts/1\")
                      (match-path posts path: \"/users/a%2Fb/posts/1\")
                      (match-path posts path: \"/users/%E9/posts/1\")
                      (match-path posts path: \"/users/%4/posts/1\")
                      (format-path posts params: {{user: \"Jérôme B/x?#%\", post: 1}})
                      (format-path posts params: {{user: \"a-b_c.d~e:f@g!$&'()*+,;=\", post: 2}})])"
    ));
    let Value::List(r) = out else { panic!() };
    assert_eq!(ok(&r[0]), vec![pair("blog/post", Value::Int(1)), pair("blog/user", Value::Str("Jérôme B".into()))]);
    assert_eq!(ok(&r[1]), vec![pair("blog/post", Value::Int(1)), pair("blog/user", Value::Str("a/b".into()))]);
    assert!(is_error(&r[2]), "not UTF-8: {:?}", r[2]);
    assert!(is_error(&r[3]), "one hex digit: {:?}", r[3]);
    assert_eq!(r[4], Value::Str("/users/J%C3%A9r%C3%B4me%20B%2Fx%3F%23%25/posts/1".into()));
    assert_eq!(r[5], Value::Str("/users/a-b_c.d~e:f@g!$&'()*+,;=/posts/2".into()));
    // What a route formats, it parses back.
    let out = run(&format!(
        "{ROUTES}
         (define p {{user: \"Jérôme B/x?#%\", post: 1}})
         (define out (= (match-path posts path: (format-path posts params: p)) (ok p)))"
    ));
    assert_eq!(out, Value::Bool(true));
}

#[test]
fn a_route_takes_the_segment_of_its_attributes_type() {
    // `a` is declared a natural, so it is captured as one.
    let src = format!("{ROUTES} (define out (match-path calc path: \"/calculator/1/+/x\"))");
    assert!(is_error(&run(&src)));
    let e = |src: &str| {
        let mut cb = Codebase::new();
        cb.add_source(library::PARSER).unwrap();
        cb.add_source(library::ROUTE).unwrap();
        cb.add_source(src).unwrap_err().to_string()
    };
    assert!(e("(define r (route \"/\" nobody))").contains("`nobody` is not a declared attribute"));
    let msg = e("(type when : (t/when boolean)) (define r (route \"/\" when))");
    assert!(msg.contains("`when` is neither a natural nor text"), "{msg}");
}
