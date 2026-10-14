//! conduct.map, the persistent hash map written in conduct: checked
//! against a model, through collisions of whole hashes, and for leaks.

use std::collections::BTreeMap;

use conduct::codegen::{self, Value, decode};
use conduct::{Codebase, library};
use wasmtime::{Engine, Instance, Module, Store};

fn codebase(src: &str) -> Codebase {
    let mut cb = Codebase::new();
    cb.add_source(library::MAP).unwrap_or_else(|e| panic!("{e}"));
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    cb
}

/// Run `entry`, give its value back, and check nothing it made is left.
fn run(cb: &Codebase, entry: &str) -> Value {
    let wat = codegen::compile_exports(cb, &[(entry, cb.names[entry])]).unwrap();
    let wasm = wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let f = instance.get_typed_func::<(), i64>(&mut store, entry).unwrap();
    let out = f.call(&mut store, ()).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let value = decode(memory.data(&store), out);
    let drop = instance.get_typed_func::<i64, ()>(&mut store, "conduct_drop").unwrap();
    drop.call(&mut store, out).unwrap();
    let live = instance.get_typed_func::<(), i32>(&mut store, "conduct_live").unwrap();
    assert_eq!(live.call(&mut store, ()).unwrap(), 0, "{entry} leaked");
    value
}

/// The runtime's hash of an integer: FNV-1a over its tagged word.
fn hash(n: i64) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in ((n << 1) | 1).to_le_bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    h
}

/// The operations a test applies, as conduct reads them: `[0 k v]` puts
/// and `[1 k 0]` removes.
const APPLY: &str = "
(define (apply-ops m ops)
  (match ops
    [] m
    [[0 k v] rest ...] (apply-ops (put m key: k value: v) ops: rest)
    [[_ k _] rest ...] (apply-ops (remove m key: k) ops: rest)
    [_ rest ...] (apply-ops m ops: rest)))
(define (values-of m keys)
  (match keys
    [] []
    [k rest ...] (concat [(lookup m key: k or: -1)] (values-of m keys: rest))))";

#[derive(Clone, Copy)]
enum Op {
    Put(i64, i64),
    Remove(i64),
}

fn ops_source(ops: &[Op]) -> String {
    let items: Vec<String> = ops
        .iter()
        .map(|op| match op {
            Op::Put(k, v) => format!("[0 {k} {v}]"),
            Op::Remove(k) => format!("[1 {k} 0]"),
        })
        .collect();
    format!("[{}]", items.join(" "))
}

fn ints(value: &Value) -> Vec<i64> {
    let Value::List(items) = value else { panic!("not a list: {value:?}") };
    items
        .iter()
        .map(|v| match v {
            Value::Int(n) => *n,
            other => panic!("not an integer: {other:?}"),
        })
        .collect()
}

/// Apply `ops` to the empty map and to a model, then compare the size,
/// the entries, and every key's value, present or not.
fn check_against_model(ops: &[Op], probe: &[i64]) {
    let mut model = BTreeMap::new();
    for op in ops {
        match *op {
            Op::Put(k, v) => {
                model.insert(k, v);
            }
            Op::Remove(k) => {
                model.remove(&k);
            }
        }
    }
    let probes: Vec<String> = probe.iter().map(i64::to_string).collect();
    let src = format!(
        "{APPLY}
         (define m (apply-ops empty ops: {}))
         (define out [(size m) (map-entries m) (values-of m keys: [{}])])
         (define (map-entries m) (flatten (entries m)))
         (define (flatten es)
           (match es [] [] [e rest ...] (concat [(entry-key e) (entry-value e)] (flatten rest))))",
        ops_source(ops),
        probes.join(" ")
    );
    let cb = codebase(&src);
    let Value::List(out) = run(&cb, "out") else { panic!() };
    assert_eq!(out[0], Value::Int(model.len() as i64));
    let flat = ints(&out[1]);
    let entries: BTreeMap<i64, i64> = flat.chunks(2).map(|c| (c[0], c[1])).collect();
    assert_eq!(entries.len() * 2, flat.len(), "a key appears twice in the entries");
    assert_eq!(entries, model);
    let expected: Vec<i64> = probe.iter().map(|k| *model.get(k).unwrap_or(&-1)).collect();
    assert_eq!(ints(&out[2]), expected);
}

/// A small deterministic generator, so a failure repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> i64 {
        (self.next() % n) as i64
    }
}

/// Pairs of distinct integers whose hashes are equal, the first four
/// counting up from zero. FNV-1a spreads small integers well: no two
/// below 2^25 collide, so the pairs are written here rather than
/// searched for on every run.
const COLLIDING: [(i64, i64); 4] =
    [(28474252, 33554434), (30550932, 33554442), (30555032, 33554446), (32652204, 33554466)];

fn colliding_pairs(count: usize) -> Vec<(i64, i64)> {
    let pairs = COLLIDING[..count].to_vec();
    for (a, b) in &pairs {
        assert_eq!(hash(*a), hash(*b), "{a} and {b} no longer collide: the runtime's hash changed");
    }
    pairs
}

#[test]
fn puts_looks_up_and_removes() {
    let cb = codebase(
        "(define m (put (put (put empty key: :a value: 1) key: :b value: 2) key: :a value: 3))
         (define out [(size m) (lookup m key: :a or: 0) (lookup m key: :b or: 0) (lookup m key: :c or: 0)
                      (contains? m key: :b) (size (remove m key: :a)) (lookup (remove m key: :a) key: :a or: 0)
                      (size (remove m key: :c))])",
    );
    assert_eq!(
        run(&cb, "out"),
        Value::List(vec![
            Value::Int(2),
            Value::Int(3),
            Value::Int(2),
            Value::Int(0),
            Value::Bool(true),
            Value::Int(1),
            Value::Int(0),
            Value::Int(2),
        ])
    );
}

#[test]
fn keys_and_values_of_any_type() {
    let cb = codebase(
        "(define m (from-entries [{entry-key: \"one\", entry-value: [1]}
                                  {entry-key: \"two\", entry-value: [2 2]}]))
         (define out (lookup m key: \"two\" or: []))",
    );
    assert_eq!(run(&cb, "out"), Value::List(vec![Value::Int(2), Value::Int(2)]));
}

#[test]
fn many_puts_and_removes_agree_with_a_model() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut ops = Vec::new();
    for _ in 0..1500 {
        let k = rng.below(600);
        if rng.below(3) == 0 {
            ops.push(Op::Remove(k));
        } else {
            ops.push(Op::Put(k, rng.below(1000)));
        }
    }
    let probe: Vec<i64> = (0..650).collect();
    check_against_model(&ops, &probe);
}

#[test]
fn removing_everything_leaves_the_empty_map() {
    let keys: Vec<i64> = (0..300).collect();
    let mut ops: Vec<Op> = keys.iter().map(|k| Op::Put(*k, k * 2)).collect();
    ops.extend(keys.iter().rev().map(|k| Op::Remove(*k)));
    let src = format!("{APPLY} (define out (= (apply-ops empty ops: {}) empty))", ops_source(&ops));
    assert_eq!(run(&codebase(&src), "out"), Value::Bool(true));
}

#[test]
fn the_same_entries_make_the_same_map() {
    // Nodes are canonical: the order of puts, and removes along the way,
    // do not show in the map.
    let mut rng = Rng(7);
    let keys: Vec<i64> = (0..200).map(|_| rng.below(100_000)).collect();
    let forward: Vec<Op> = keys.iter().map(|k| Op::Put(*k, 1)).collect();
    let mut backward: Vec<Op> = keys.iter().rev().map(|k| Op::Put(*k, 1)).collect();
    backward.extend((0..50).map(|k| Op::Put(1_000_000 + k, 2)));
    backward.extend((0..50).map(|k| Op::Remove(1_000_000 + k)));
    let src = format!(
        "{APPLY} (define out (= (apply-ops empty ops: {}) (apply-ops empty ops: {})))",
        ops_source(&forward),
        ops_source(&backward)
    );
    assert_eq!(run(&codebase(&src), "out"), Value::Bool(true));
}

#[test]
fn keys_whose_hashes_collide() {
    let pairs = colliding_pairs(4);
    // The runtime hashes them alike too, so the trie really collides.
    let (a, b) = pairs[0];
    let cb = codebase(&format!("(define out [(hash {a}) (hash {b})])"));
    assert_eq!(ints(&run(&cb, "out")), vec![hash(a) as i64; 2]);
    let mut ops = Vec::new();
    for (i, (a, b)) in pairs.iter().enumerate() {
        let i = i as i64;
        ops.push(Op::Put(*a, 10 * i));
        ops.push(Op::Put(*b, 10 * i + 1));
    }
    // Overwrite one of a colliding pair, remove another, and remove both
    // of a third, which empties the node at the bottom of the trie.
    let (a0, _) = pairs[0];
    let (_, b1) = pairs[1];
    let (a2, b2) = pairs[2];
    ops.push(Op::Put(a0, 99));
    ops.push(Op::Remove(b1));
    ops.push(Op::Remove(a2));
    ops.push(Op::Remove(b2));
    let probe: Vec<i64> = pairs.iter().flat_map(|(a, b)| [*a, *b]).chain([0, 1, 2]).collect();
    check_against_model(&ops, &probe);
}

#[test]
fn a_collision_removed_down_to_one_entry_comes_back_up() {
    let (a, b) = colliding_pairs(1)[0];
    let ops = [Op::Put(a, 1), Op::Put(b, 2), Op::Remove(b)];
    let src =
        format!("{APPLY} (define out (= (apply-ops empty ops: {}) (put empty key: {a} value: 1)))", ops_source(&ops));
    assert_eq!(run(&codebase(&src), "out"), Value::Bool(true));
}

#[test]
fn colliding_keys_in_any_order_make_the_same_map() {
    // Whole-hash collisions are where a trie's shape could depend on the
    // order of puts; their entries are kept in key order, so it does not.
    let pairs = colliding_pairs(4);
    let forward: Vec<Op> = pairs.iter().flat_map(|(a, b)| [Op::Put(*a, 1), Op::Put(*b, 2)]).collect();
    let backward: Vec<Op> = pairs.iter().rev().flat_map(|(a, b)| [Op::Put(*b, 2), Op::Put(*a, 1)]).collect();
    let src = format!(
        "{APPLY}
         (define one (apply-ops empty ops: {}))
         (define other (apply-ops empty ops: {}))
         (define out [(= one other) (= (hash one) (hash other)) (compare one other)])",
        ops_source(&forward),
        ops_source(&backward)
    );
    assert_eq!(run(&codebase(&src), "out"), Value::List(vec![Value::Bool(true), Value::Bool(true), Value::Int(0)]));
}

#[test]
fn the_readme_example() {
    let cb = codebase(
        "(define m (put (put empty key: \"a\" value: 1) key: \"b\" value: 2))
         (define out [(lookup m key: \"a\" or: 0) (contains? m key: \"c\") (size (remove m key: \"a\"))
                      (= (from-entries (entries m)) m)])",
    );
    assert_eq!(run(&cb, "out"), Value::List(vec![Value::Int(1), Value::Bool(false), Value::Int(1), Value::Bool(true)]));
}

#[test]
fn colliding_keys_part_at_the_next_frame() {
    let pairs = colliding_pairs(4);
    let frames: Vec<String> = pairs
        .iter()
        // Six levels read frame 0; the seventh reads frame 1, `(hash [1 key])`.
        .map(|(a, b)| format!("[(frame-at {a} depth: 5) (hash {a}) (frame-at {a} depth: 6) (frame-at {b} depth: 6)]"))
        .collect();
    let cb = codebase(&format!("(define out [{}])", frames.join(" ")));
    let Value::List(rows) = run(&cb, "out") else { panic!() };
    for (row, (a, b)) in rows.iter().zip(&pairs) {
        let r = ints(row);
        assert_eq!(r[0], r[1], "frame 0 of {a} is its hash");
        assert_eq!(r[0], hash(*a) as i64);
        assert_ne!(r[2], r[3], "{a} and {b} collide at frame 0 but not at frame 1");
    }
}

#[test]
fn functions_are_keys() {
    // `hash` reads a function's code and what it captured, as `=` does, so
    // functions that differ only in what they captured part in the trie.
    let cb = codebase(
        "(define (adder n) (fn [x] (+ x n)))
         (define m (put (put (put empty key: (adder 1) value: :one) key: (adder 2) value: :two)
                        key: (adder 3) value: :three))
         (define out [(= (hash (adder 1)) (hash (adder 1)))
                      (= (hash (adder 1)) (hash (adder 2)))
                      (size m)
                      (lookup m key: (adder 2) or: :none)
                      (lookup m key: (adder 4) or: :none)
                      (size (remove m key: (adder 1)))])",
    );
    assert_eq!(
        run(&cb, "out"),
        Value::List(vec![
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(3),
            Value::Keyword("two".into()),
            Value::Keyword("none".into()),
            Value::Int(2),
        ])
    );
}
