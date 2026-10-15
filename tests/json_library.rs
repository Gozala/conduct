//! conduct.json: JSON read with conduct.parser and written back, in
//! conduct rather than the runtime.

use conduct::codegen::{self, Value, decode};
use conduct::{Codebase, library};
use wasmtime::{Engine, Instance, Module, Store};

fn run(src: &str) -> Value {
    let mut cb = Codebase::new();
    cb.add_source(library::PARSER).unwrap();
    cb.add_source(library::JSON).unwrap_or_else(|e| panic!("{e}"));
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

/// Read each text and write it back: what conduct.json makes of it,
/// or `error` where it does not read.
fn round_trip(inputs: &[&str]) -> Vec<String> {
    let quoted: Vec<String> = inputs.iter().map(|t| format!("{t:?}")).collect();
    let src = format!(
        "(define (again t) (match (from-text t) (ok j) (to-text j) (error _) \"error\"))
         (define out (map [{}] each: again))",
        quoted.join(" ")
    );
    let Value::List(items) = run(&src) else { panic!() };
    items
        .into_iter()
        .map(|v| match v {
            Value::Str(s) => s,
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn values_read_and_write_back() {
    assert_eq!(
        round_trip(&[
            "null",
            " true ",
            "false",
            "0",
            "-42",
            "1234567890",
            "1.5",
            "-0.25e+3",
            "\"\"",
            "\"plain\"",
            "[]",
            "[1, [2, []], {\"a\": null}]",
            "{ \"k\" : \"v\" , \"n\" : -1 }",
        ]),
        vec![
            "null",
            "true",
            "false",
            "0",
            "-42",
            "1234567890",
            "1.5",
            "-0.25e+3",
            "\"\"",
            "\"plain\"",
            "[]",
            "[1,[2,[]],{\"a\":null}]",
            "{\"k\":\"v\",\"n\":-1}",
        ]
    );
}

#[test]
fn strings_escape_both_ways() {
    let out = round_trip(&[
        r#""quote \" backslash \\ slash \/ tab \t newline \n""#,
        r#""é€""#,
        r#""😀""#,
        r#""lone \ud800 surrogate""#,
        "\"é😀\"",
        r#""\u0001""#,
    ]);
    assert_eq!(out[0], r#""quote \" backslash \\ slash / tab \t newline \n""#);
    assert_eq!(out[1], "\"é€\"");
    assert_eq!(out[2], "\"😀\"", "a surrogate pair is one code point");
    assert_eq!(out[3], "\"lone \u{fffd} surrogate\"");
    assert_eq!(out[4], "\"é😀\"");
    assert_eq!(out[5], r#""\u0001""#, "a control character is escaped");
}

#[test]
fn what_is_not_json_does_not_read() {
    let out = round_trip(&[
        "",
        "tru",
        "01",
        "1.",
        "-",
        "1e",
        "[1,]",
        "{\"a\" 1}",
        "{a: 1}",
        "\"open",
        "\"tab\there\"",
        "[1] 2",
        "nul",
    ]);
    assert!(out.iter().all(|o| o == "error"), "{out:?}");
}

#[test]
fn members_are_read_by_name() {
    let src = "(define j (match (from-text \"{\\\"name\\\": \\\"Ada\\\", \\\"age\\\": 36}\") (ok j) j (error _) :null))
               (define out [(member j name: \"name\" or: :null) (member j name: \"missing\" or: :null)
                            (member (number 1) name: \"name\" or: :null)])";
    let Value::List(items) = run(src) else { panic!() };
    assert_eq!(items[1], Value::Keyword("null".into()));
    assert_eq!(items[2], Value::Keyword("null".into()));
    assert!(format!("{:?}", items[0]).contains("Ada"));
}

#[test]
fn large_and_deep_documents() {
    let big = format!("[{}]", (0..10_000).map(|n| n.to_string()).collect::<Vec<_>>().join(","));
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let out = round_trip(&[&big, &deep]);
    assert_eq!(out[0], big);
    assert_eq!(out[1], deep);
}
