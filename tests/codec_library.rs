//! conduct.codec: one description of a value, for JSON both ways.

use conduct::codegen::{self, Value, decode};
use conduct::{Codebase, library};
use wasmtime::{Engine, Instance, Module, Store};

fn codebase(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(library::PARSER).unwrap();
    cb.add_source(library::JSON).unwrap();
    cb.add_source(library::CODEC).unwrap_or_else(|e| panic!("{e}"));
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

fn texts(src: &str) -> Vec<String> {
    let Value::List(items) = run(src) else { panic!() };
    items
        .into_iter()
        .map(|v| match v {
            Value::Str(s) => s,
            other => panic!("{other:?}"),
        })
        .collect()
}

/// Decode each JSON text with `codec`, and write back what it decoded,
/// or why it did not.
fn decoded(prelude: &str, codec: &str, inputs: &[&str]) -> Vec<String> {
    let quoted: Vec<String> = inputs.iter().map(|t| format!("{t:?}")).collect();
    texts(&format!(
        "{prelude}
         (define (again t)
           (match (decode-text t with: {codec})
             (ok v) (encode-text v with: {codec})
             (error (not-json _)) \"not json\"
             (error e) (error-text e)))
         (define out (conduct/map [{}] each: again))",
        quoted.join(" ")
    ))
}

#[test]
fn primitives_both_ways() {
    let out = texts(
        "(define out [(encode-text \"hé\\\"\" with: codec/text) (encode-text true with: codec/boolean)
                      (encode-text -7 with: codec/integer) (encode-text 7 with: codec/natural)
                      (encode-text [1 2 3] with: (codec/list codec/natural))
                      (encode-text [[] [\"a\"]] with: (codec/list (codec/list codec/text)))])",
    );
    assert_eq!(out, ["\"hé\\\"\"", "true", "-7", "7", "[1,2,3]", "[[],[\"a\"]]"]);
    assert_eq!(
        decoded("", "codec/natural", &["0", "42", "-1", "1.5", "\"1\""]),
        [
            "0",
            "42",
            "the value at `json` fails with :expecting-natural: -1",
            "the value at `json` fails with :expecting-natural: 1.5",
            "the value at `json` fails with :expecting-natural: \"1\"",
        ]
    );
    assert_eq!(
        decoded("", "(codec/list codec/boolean)", &["[true, false]", "[true, 1]", "{}", "[tru"]),
        [
            "[true,false]",
            "the value at `json[1]` fails with :expecting-boolean: 1",
            "the value at `json` fails with :expecting-array: {}",
            "not json",
        ]
    );
}

const POINT: &str = "
(type x : (geo/x integer))
(type y : (geo/y integer))
(type label : (geo/label text))
(define point (record-codec [x \"x\" codec/integer] [y \"y\" codec/integer] [label \"label\" codec/text :optional]))";

#[test]
fn records_by_their_attributes() {
    let out = texts(&format!(
        "{POINT}
         (define out [(encode-text {{x: 1, y: -2}} with: point)
                      (encode-text {{x: 1, y: 2, label: \"origin\"}} with: point)])"
    ));
    assert_eq!(out, ["{\"x\":1,\"y\":-2}", "{\"x\":1,\"y\":2,\"label\":\"origin\"}"]);
    assert_eq!(
        decoded(
            POINT,
            "point",
            &[
                "{\"y\": 2, \"x\": 1}",
                "{\"x\": 1, \"y\": 2, \"label\": \"here\", \"extra\": null}",
                "{\"x\": 1}",
                "{\"x\": 1, \"y\": \"2\"}",
                "{\"x\": 1, \"y\": 2, \"label\": 3}",
                "[1, 2]",
            ]
        ),
        [
            "{\"x\":1,\"y\":2}",
            "{\"x\":1,\"y\":2,\"label\":\"here\"}",
            "the value at `json.y` fails with :missing-member: {\"x\":1}",
            "the value at `json.y` fails with :expecting-integer: \"2\"",
            "the value at `json.label` fails with :expecting-text: 3",
            "the value at `json` fails with :expecting-object: [1,2]",
        ]
    );
    // What a record codec decodes is the record its attributes make.
    let cb = codebase(&format!("{POINT} (define (decoded t) (decode-text t with: point))"));
    let t = cb.type_of("decoded").unwrap();
    assert!(t.contains("(ok {label?: text, x: integer, y: integer})"), "{t}");
}

const SHAPE: &str = "
(type (circle ?r) : {(geo/circle ?r)})
(type (square ?s) : {(geo/square ?s)})
(define shape
  (variant-codec [circle \"Circle\" codec/natural] [square \"Square\" (codec/list codec/natural)] [:empty \"Empty\"]))";

#[test]
fn variants_as_elm_codec_writes_them() {
    let out = texts(&format!(
        "{SHAPE}
         (define out [(encode-text (circle 2) with: shape) (encode-text (square [1 2]) with: shape)
                      (encode-text :empty with: shape)])"
    ));
    assert_eq!(
        out,
        [
            "{\"tag\":\"Circle\",\"args\":[2]}",
            "{\"tag\":\"Square\",\"args\":[[1,2]]}",
            "{\"tag\":\"Empty\",\"args\":[]}",
        ]
    );
    assert_eq!(
        decoded(
            SHAPE,
            "shape",
            &[
                "{\"tag\": \"Circle\", \"args\": [5]}",
                "{\"args\": [], \"tag\": \"Empty\"}",
                "{\"tag\": \"Square\", \"args\": [[3]]}",
                "{\"tag\": \"Triangle\", \"args\": []}",
                "{\"tag\": \"Circle\", \"args\": [\"5\"]}",
                "{\"tag\": \"Circle\", \"args\": []}",
                "{\"tag\": \"Circle\"}",
            ]
        ),
        [
            "{\"tag\":\"Circle\",\"args\":[5]}",
            "{\"tag\":\"Empty\",\"args\":[]}",
            "{\"tag\":\"Square\",\"args\":[[3]]}",
            "the value at `json` fails with :unknown-tag: {\"tag\":\"Triangle\",\"args\":[]}",
            "the value at `json.args[0]` fails with :expecting-natural: \"5\"",
            "the value at `json.args` fails with :expecting-argument: []",
            "the value at `json.args` fails with :expecting-array: {\"tag\":\"Circle\"}",
        ]
    );
}

#[test]
fn mapped_enumerated_and_alternative_codecs() {
    let prelude = "
        (define celsius (codec/map codec/integer to: (fn [c] (+ c 273)) from: (fn [k] (- k 273))))
        (define color (codec/enum codec/text cases: [{enum-encoded: \"r\", enum-decoded: :red}
                                                     {enum-encoded: \"g\", enum-decoded: :green}]))
        (define lenient (codec/one-of codec/natural fallbacks: [(codec/map codec/text to: (fn [_] 0) from: (fn [n] \"\"))]))";
    assert_eq!(decoded(prelude, "celsius", &["20"]), ["20"]);
    let out =
        texts(&format!("{prelude} (define out [(encode-text 300 with: celsius) (encode-text :green with: color)])"));
    assert_eq!(out, ["27", "\"g\""]);
    assert_eq!(
        decoded(prelude, "color", &["\"r\"", "\"b\"", "1"]),
        [
            "\"r\"",
            "the value at `json` fails with :unknown-case: \"b\"",
            "the value at `json` fails with :expecting-text: 1",
        ]
    );
    assert_eq!(
        decoded(prelude, "lenient", &["4", "\"four\"", "true"]),
        [
            "4",
            "0",
            "none of 2 ways to decode the value at `json` fits: the value at `json` fails with :expecting-natural: true; \
         the value at `json` fails with :expecting-text: true",
        ]
    );
}

#[test]
fn a_constant_member_is_written_and_checked() {
    let prelude = "
        (type x : (geo/x integer))
        (define tagged-point
          (build-record (field (constant-field (start-record {}) name: \"type\" value: \"point\" codec: codec/text)
                               name: \"x\" get: (fn [r] (x r)) set: (fn [r] (fn [v] (assoc r x: v))) codec: codec/integer)))";
    assert_eq!(
        decoded(prelude, "tagged-point", &["{\"type\": \"point\", \"x\": 1}", "{\"type\": \"line\", \"x\": 1}"]),
        ["{\"type\":\"point\",\"x\":1}", "the value at `json.type` fails with :unexpected-value: \"line\"",]
    );
}

#[test]
fn a_codec_of_itself() {
    let prelude = "
        (type (leaf ?v) : {(t.tree/leaf ?v)})
        (type (node ?c) : {(t.tree/node ?c)})
        (define tree
          (codec/recursive (fn [tree] (variant-codec [leaf \"Leaf\" codec/natural] [node \"Node\" (codec/list tree)]))))";
    let deep =
        format!("{}{{\"tag\":\"Leaf\",\"args\":[1]}}{}", "{\"tag\":\"Node\",\"args\":[[".repeat(50), "]]}".repeat(50));
    let out = decoded(
        prelude,
        "tree",
        &[
            "{\"tag\":\"Node\",\"args\":[[{\"tag\":\"Leaf\",\"args\":[1]},{\"tag\":\"Node\",\"args\":[[]]}]]}",
            &deep,
            "{\"tag\":\"Node\",\"args\":[[{\"tag\":\"Leaf\",\"args\":[-1]}]]}",
        ],
    );
    assert_eq!(
        out[0],
        "{\"tag\":\"Node\",\"args\":[[{\"tag\":\"Leaf\",\"args\":[1]},{\"tag\":\"Node\",\"args\":[[]]}]]}"
    );
    assert_eq!(out[1], deep);
    assert_eq!(out[2], "the value at `json.args[0][0].args[0]` fails with :expecting-natural: -1");
}
