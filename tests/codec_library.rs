//! conduct.codec: one description of a value, for JSON and CBOR both ways.

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
            "the value at `value` fails with :expecting-natural: -1",
            "the value at `value` fails with :expecting-natural: 1.5",
            "the value at `value` fails with :expecting-natural: \"1\"",
        ]
    );
    assert_eq!(
        decoded("", "(codec/list codec/boolean)", &["[true, false]", "[true, 1]", "{}", "[tru"]),
        [
            "[true,false]",
            "the value at `value[1]` fails with :expecting-boolean: 1",
            "the value at `value` fails with :expecting-list: {}",
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
            "the value at `value.y` fails with :missing-member: {\"x\":1}",
            "the value at `value.y` fails with :expecting-integer: \"2\"",
            "the value at `value.label` fails with :expecting-text: 3",
            "the value at `value` fails with :expecting-fields: [1,2]",
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
            "the value at `value` fails with :unknown-tag: {\"tag\":\"Triangle\",\"args\":[]}",
            "the value at `value.args[0]` fails with :expecting-natural: \"5\"",
            "the value at `value.args` fails with :expecting-argument: []",
            "the value at `value.args` fails with :expecting-list: {\"tag\":\"Circle\"}",
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
            "the value at `value` fails with :unknown-case: \"b\"",
            "the value at `value` fails with :expecting-text: 1",
        ]
    );
    assert_eq!(
        decoded(prelude, "lenient", &["4", "\"four\"", "true"]),
        [
            "4",
            "0",
            "none of 2 ways to decode the value at `value` fits: the value at `value` fails with :expecting-natural: true; \
         the value at `value` fails with :expecting-text: true",
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
        ["{\"type\":\"point\",\"x\":1}", "the value at `value.type` fails with :unexpected-value: \"line\"",]
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
    assert_eq!(out[2], "the value at `value.args[0][0].args[0]` fails with :expecting-natural: -1");
}

/// Encode each value with `codec` as CBOR, and decode the bytes back:
/// the bytes, and what decoding them gave, written as JSON text.
fn cbor_both_ways(prelude: &str, codec: &str, values: &[&str]) -> Vec<(Vec<u8>, String)> {
    let Value::List(items) = run(&format!(
        "{prelude}
         (define (both v)
           (let [b (encode-cbor v with: {codec})]
             [b (match (decode-cbor b with: {codec}) (ok back) (encode-text back with: {codec}) (error e) (error-text e))]))
         (define out [{}])",
        values.iter().map(|v| format!("(both {v})")).collect::<Vec<_>>().join(" ")
    )) else {
        panic!()
    };
    items
        .into_iter()
        .map(|pair| match pair {
            Value::List(mut p) => {
                let Value::Str(back) = p.pop().unwrap() else { panic!() };
                (bytes_of(&p.pop().unwrap()), back)
            }
            other => panic!("{other:?}"),
        })
        .collect()
}

fn bytes_of(v: &Value) -> Vec<u8> {
    match v {
        Value::List(items) => items
            .iter()
            .map(|i| match i {
                Value::Int(n) => *n as u8,
                other => panic!("{other:?}"),
            })
            .collect(),
        other => panic!("{other:?}"),
    }
}

fn conduct_bytes(bs: &[u8]) -> String {
    format!("[{}]", bs.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" "))
}

/// Decode each CBOR encoding with `codec`: what it decoded, as JSON text,
/// or why it did not.
fn from_cbor(prelude: &str, codec: &str, inputs: &[&[u8]]) -> Vec<String> {
    texts(&format!(
        "{prelude}
         (define (back b) (match (decode-cbor b with: {codec}) (ok v) (encode-text v with: {codec}) (error e) (error-text e)))
         (define out [{}])",
        inputs.iter().map(|i| format!("(back {})", conduct_bytes(i))).collect::<Vec<_>>().join(" ")
    ))
}

/// CBOR as `ciborium` writes it: the reference these tests check against.
fn reference(value: &ciborium::Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).unwrap();
    out
}

#[test]
fn primitives_as_cbor() {
    use ciborium::Value as C;
    let out = cbor_both_ways(
        "",
        "codec/integer",
        &[
            "0",
            "23",
            "24",
            "255",
            "256",
            "65535",
            "65536",
            "4294967296",
            "-1",
            "-24",
            "-25",
            "-1000",
            "2305843009213693951",
        ],
    );
    let expected: Vec<i64> =
        vec![0, 23, 24, 255, 256, 65535, 65536, 4294967296, -1, -24, -25, -1000, 2305843009213693951];
    for ((bytes, back), n) in out.iter().zip(&expected) {
        assert_eq!(*bytes, reference(&C::Integer((*n).into())), "{n}");
        assert_eq!(*back, n.to_string());
    }
    let out = cbor_both_ways("", "codec/text", &["\"\"", "\"hé\"", &format!("{:?}", "x".repeat(30))]);
    assert_eq!(out[0].0, reference(&C::Text(String::new())));
    assert_eq!(out[1].0, reference(&C::Text("hé".into())));
    assert_eq!(out[2].0, reference(&C::Text("x".repeat(30))));
    assert_eq!(out[1].1, "\"hé\"");
    let out = cbor_both_ways("", "(codec/list codec/boolean)", &["[true false]", "[]"]);
    assert_eq!(out[0].0, reference(&C::Array(vec![C::Bool(true), C::Bool(false)])));
    assert_eq!(out[1], (vec![0x80], "[]".into()));
    let out = cbor_both_ways("", "codec/bytes", &["(utf8 \"ab\")"]);
    assert_eq!(out[0].0, reference(&C::Bytes(b"ab".to_vec())));
    assert_eq!(out[0].1, "[97,98]");
    let out =
        cbor_both_ways("", "codec/raw", &["(data-list [:null (data-decimal \"1.5\") (data-decimal \"-2.50e3\")])"]);
    // A decimal is a decimal fraction, tag 4 of [exponent mantissa].
    assert_eq!(
        out[0].0,
        reference(&C::Array(vec![
            C::Null,
            C::Tag(4, Box::new(C::Array(vec![C::Integer((-1).into()), C::Integer(15.into())]))),
            C::Tag(4, Box::new(C::Array(vec![C::Integer(1.into()), C::Integer((-250).into())]))),
        ]))
    );
    assert_eq!(out[0].1, "[null,15e-1,-250e1]");
}

#[test]
fn cbor_from_elsewhere_and_what_it_cannot_hold() {
    use ciborium::Value as C;
    // Integers in longer forms than they need, and keys in any order, read.
    let long = [0x1b, 0, 0, 0, 0, 0, 0, 0, 5];
    let point = reference(&C::Map(vec![
        (C::Text("y".into()), C::Integer(2.into())),
        (C::Text("x".into()), C::Integer(1.into())),
    ]));
    assert_eq!(from_cbor(POINT, "point", &[&point]), ["{\"x\":1,\"y\":2}"]);
    assert_eq!(from_cbor("", "codec/integer", &[&long]), ["5"]);
    assert_eq!(
        from_cbor(
            "",
            "codec/integer",
            &[
                &[],
                &[0x19, 0x01],
                &[0x01, 0x02],
                &[0x1b, 0x20, 0, 0, 0, 0, 0, 0, 0],
                &[0x1f],
                &[0xf9, 0x3c, 0x00],
                &[0xc1, 0x01],
                &[0x1c],
            ]
        ),
        [
            "the bytes of `value` fail at offset 0 with :unexpected-end",
            "the bytes of `value` fail at offset 2 with :unexpected-end",
            "the bytes of `value` fail at offset 1 with :trailing-bytes",
            "the bytes of `value` fail at offset 0 with :integer-too-large",
            "the bytes of `value` fail at offset 0 with :indefinite-length",
            "the bytes of `value` fail at offset 0 with :unsupported-float",
            "the bytes of `value` fail at offset 0 with :unsupported-tag",
            "the bytes of `value` fail at offset 0 with :reserved-argument",
        ]
    );
    assert_eq!(
        from_cbor("", "codec/text", &[&[0x62, 0xc3], &[0x61, 0xc3], &[0x62, 0xc3, 0xa9], &[0x01]]),
        [
            "the bytes of `value` fail at offset 2 with :unexpected-end",
            "the bytes of `value` fail at offset 1 with :invalid-utf8",
            "\"é\"",
            "the value at `value` fails with :expecting-text: 1",
        ]
    );
    let int_key = reference(&C::Map(vec![(C::Integer(1.into()), C::Integer(2.into()))]));
    assert_eq!(
        from_cbor(POINT, "point", &[&int_key]),
        ["the bytes of `value` fail at offset 1 with :expecting-text-key"]
    );
}

#[test]
fn records_and_variants_as_cbor() {
    use ciborium::Value as C;
    let text = |t: &str| C::Text(t.into());
    let int = |n: i64| C::Integer(n.into());
    // Keys in their encodings' bytewise order: shorter first, so `label`
    // comes after `x` and `y`, wherever the codec lists it.
    let out = cbor_both_ways(POINT, "point", &["{x: 1, y: -2}", "{x: 1, y: 2, label: \"o\"}"]);
    assert_eq!(out[0].0, reference(&C::Map(vec![(text("x"), int(1)), (text("y"), int(-2))])));
    assert_eq!(
        out[1].0,
        reference(&C::Map(vec![(text("x"), int(1)), (text("y"), int(2)), (text("label"), text("o"))]))
    );
    assert_eq!(out[1].1, "{\"x\":1,\"y\":2,\"label\":\"o\"}");
    let out = cbor_both_ways(SHAPE, "shape", &["(square [1 2])", ":empty"]);
    // `tag` is shorter than `args`, so it comes first.
    assert_eq!(
        out[0].0,
        reference(&C::Map(vec![
            (text("tag"), text("Square")),
            (text("args"), C::Array(vec![C::Array(vec![int(1), int(2)])]))
        ]))
    );
    assert_eq!(out[0].1, "{\"tag\":\"Square\",\"args\":[[1,2]]}");
    assert_eq!(out[1].1, "{\"tag\":\"Empty\",\"args\":[]}");
    let missing = reference(&C::Map(vec![(text("x"), int(1))]));
    assert_eq!(
        from_cbor(POINT, "point", &[&missing]),
        ["the value at `value.y` fails with :missing-member: {\"x\":1}"]
    );
}

#[test]
fn the_same_codec_reads_json_and_cbor_alike() {
    let prelude = format!(
        "{POINT}
         (type (leaf ?v) : {{(t.tree/leaf ?v)}})
         (type (node ?c) : {{(t.tree/node ?c)}})
         (define tree
           (codec/recursive (fn [tree] (variant-codec [leaf \"Leaf\" point] [node \"Node\" (codec/list tree)]))))"
    );
    let value = "(node [(leaf {x: 1, y: 2}) (node []) (leaf {x: -3, y: 4, label: \"é\"})])";
    let out = texts(&format!(
        "{prelude}
         (define v {value})
         (define out [(encode-text v with: tree)
                      (match (decode-cbor (encode-cbor v with: tree) with: tree) (ok w) (encode-text w with: tree) (error e) (error-text e))
                      (match (decode-text (encode-text v with: tree) with: tree)
                        (ok w) (if (= (encode-cbor w with: tree) (encode-cbor v with: tree)) \"same\" \"different\")
                        (error e) (error-text e))])"
    ));
    assert_eq!(out[0], out[1]);
    assert_eq!(out[2], "same");
    // What a codec writes is bytes, kept one to a byte.
    let src = format!("{POINT} (define out (encode-cbor {{x: 1, y: 2, label: \"hi\"}} with: point))");
    let cb = codebase(&src);
    let wat = codegen::compile_exports(&cb, &[("out", cb.names["out"])]).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let out = instance.get_typed_func::<(), i64>(&mut store, "out").unwrap().call(&mut store, ()).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let kind = u32::from_le_bytes(memory.data(&store)[out as usize..out as usize + 4].try_into().unwrap());
    assert_eq!(kind, codegen::KIND_BYTES);
}

/// A small deterministic generator, for the property below.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random data item, as conduct source and as the CBOR value it is,
/// with a map's keys in deterministic order.
fn random_data(rng: &mut Rng, depth: u32) -> (String, ciborium::Value) {
    use ciborium::Value as C;
    let leaf = depth == 0 || rng.below(3) == 0;
    match if leaf { rng.below(5) } else { 5 + rng.below(2) } {
        0 => (":null".into(), C::Null),
        1 => {
            let b = rng.below(2) == 1;
            (format!("(data-boolean {b})"), C::Bool(b))
        }
        2 => {
            // Around every boundary between the integer's sizes.
            let bounds = [0i64, 23, 24, 255, 256, 65535, 65536, 4294967295, 4294967296, 2305843009213693951];
            let b = bounds[rng.below(bounds.len() as u64) as usize];
            let n = if rng.below(2) == 0 { b } else { -b - 1 };
            (format!("(data-integer {n})"), C::Integer(n.into()))
        }
        3 => {
            let len = [0, 1, 23, 24, 300][rng.below(5) as usize];
            let t: String = (0..len).map(|i| ['a', 'é', '€', '😀'][(i + rng.below(4) as usize) % 4]).collect();
            (format!("(data-text {t:?})"), C::Text(t))
        }
        4 => {
            let len = [0, 3, 24][rng.below(3) as usize];
            let bs: Vec<u8> = (0..len).map(|_| rng.below(256) as u8).collect();
            (format!("(data-bytes {})", conduct_bytes(&bs)), C::Bytes(bs))
        }
        5 => {
            let n = rng.below(4);
            let items: Vec<_> = (0..n).map(|_| random_data(rng, depth - 1)).collect();
            (
                format!("(data-list [{}])", items.iter().map(|i| i.0.clone()).collect::<Vec<_>>().join(" ")),
                C::Array(items.into_iter().map(|i| i.1).collect()),
            )
        }
        _ => {
            let names = ["a", "b", "id", "zz", "name", "kind", "é", "aa"];
            let mut keys: Vec<&str> = names.iter().copied().filter(|_| rng.below(2) == 0).collect();
            keys.dedup();
            let fields: Vec<(&str, (String, ciborium::Value))> =
                keys.iter().map(|k| (*k, random_data(rng, depth - 1))).collect();
            let source = fields
                .iter()
                .map(|(k, (src, _))| format!("{{field-name: {k:?}, field-value: {src}}}"))
                .collect::<Vec<_>>()
                .join(" ");
            let mut sorted: Vec<(Vec<u8>, ciborium::Value, ciborium::Value)> =
                fields.into_iter().map(|(k, (_, v))| (reference(&C::Text(k.into())), C::Text(k.into()), v)).collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            (format!("(data-fields [{source}])"), C::Map(sorted.into_iter().map(|(_, k, v)| (k, v)).collect()))
        }
    }
}

#[test]
fn cbor_agrees_with_a_reference_both_ways() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let cases: Vec<(String, ciborium::Value)> = (0..60).map(|_| random_data(&mut rng, 4)).collect();
    let expected: Vec<Vec<u8>> = cases.iter().map(|(_, v)| reference(v)).collect();
    // Writing each, and reading the reference's bytes and writing them
    // again, give the reference's bytes.
    let Value::List(out) = run(&format!(
        "(define (again b) (match (from-cbor b) (ok d) (to-cbor d) (error e) []))
         (define out [{} {}])",
        cases.iter().map(|(src, _)| format!("(to-cbor {src})")).collect::<Vec<_>>().join(" "),
        expected.iter().map(|b| format!("(again {})", conduct_bytes(b))).collect::<Vec<_>>().join(" ")
    )) else {
        panic!()
    };
    let (written, reread) = out.split_at(cases.len());
    for (i, b) in expected.iter().enumerate() {
        assert_eq!(bytes_of(&written[i]), *b, "writing {}", cases[i].0);
        assert_eq!(bytes_of(&reread[i]), *b, "reading back {}", cases[i].0);
    }
}

#[test]
fn a_record_codec_takes_its_attributes_codecs_from_their_declarations() {
    let prelude = "
        (type x : (geo/x integer))
        (type y : (geo/y integer))
        (type tags : (geo/tags (list text)))
        (type label : (geo/label text))
        (define point (record-codec x y tags [label \"name\" codec/text :optional]))";
    let out = texts(&format!(
        "{prelude}
         (define out [(encode-text {{x: 1, y: -2, tags: [\"a\"]}} with: point)
                      (encode-text {{x: 1, y: 2, tags: [], label: \"o\"}} with: point)])"
    ));
    assert_eq!(out, ["{\"x\":1,\"y\":-2,\"tags\":[\"a\"]}", "{\"x\":1,\"y\":2,\"tags\":[],\"name\":\"o\"}"]);
    let cb = codebase(&format!("{prelude} (define (decoded t) (decode-text t with: point))"));
    let t = cb.type_of("decoded").unwrap();
    assert!(t.contains("(ok {label?: text, tags: (list text), x: integer, y: integer})"), "{t}");
    let e = |src: &str| {
        let mut cb = Codebase::new();
        cb.add_source(library::PARSER).unwrap();
        cb.add_source(library::JSON).unwrap();
        cb.add_source(library::CODEC).unwrap();
        cb.add_source(src).unwrap_err().to_string()
    };
    let msg = e("(type f : (t/f (dict text))) (define c (record-codec f))");
    assert!(msg.contains("`f` has a type with no codec to take: write `[f \"f\" codec]`"), "{msg}");
    assert!(e("(define c (record-codec nobody))").contains("`nobody` is not a declared attribute"));
}
