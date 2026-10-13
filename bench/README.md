# bench

The polyfill demo's four TodoMVC elements, written three times, in
conduct, in Zena and in Rust, run on the [Wasm Component Model
Polyfill] over its Wasmtime core backend, natively. Each language's
elements are components of the same `demo:element/element-component`
world and render the same views, so the benchmark compares what one
element costs to load and to run in each. It also times the conduct and
Zena compilers, each a component on the same polyfill, compiling the
demo's six sources.

The numbers below are from one run on a 2 vCPU Intel Xeon at 2.10 GHz,
and are in [`results.json`](results.json).

## What is compared

- **conduct**: [`examples/`](../examples), compiled by the library
  (`conduct::service::compile`, the bytes `conduct element` writes), at
  commit 7d76b9b.
- **Zena**: the polyfill demo's
  [`zena/app`](https://github.com/dialog-db/wasm-component-model-polyfill/tree/eedcd22766b35cc34b3b81b4c753ab4f08a7c09a/rust/wcmp-demo/zena/app)
  sources, compiled by Zena at 1bbe472f4c34d3faf12c74f9876afcfe541c4ac3
  with the glue the demo writes around them.
- **Rust**: [`rust/`](rust), a port of the conduct elements that
  renders and emits what they do, with `wit-bindgen` 0.62.0 on rustc
  1.97.0, built at `opt-level = "s"` and again at `"z"`.

The polyfill is dialog-db/wasm-component-model-polyfill at
eedcd22766b35cc34b3b81b4c753ab4f08a7c09a, which pins Wasmtime
49.0.0-rc.1. The Rust components were encoded and validated with
wasm-tools 1.261.0.

## Building and running

```
(cd ../compiler && ./build.sh)    # the conduct compiler component
(cd rust && ./build.sh)           # rust/out/*.wasm and rust/out/opt-z/*.wasm
zena/build.sh <zena checkout> <polyfill checkout> <out>
(cd harness && ZENA_DIR=<out>/components ZENA_SOURCES=<out>/sources cargo run --release)
```

The harness compiles the conduct elements itself. It skips Zena, and
says so, when `ZENA_DIR` and `ZENA_SOURCES` are not set. It writes
`results.json` here and prints a table. The doc comment at the top of
[`harness/src/main.rs`](harness/src/main.rs) lists every variable it
reads, and the counts below are its defaults.

`zena/build.sh` builds what the demo builds: it bootstraps Zena as Zena's
DEVELOPMENT.md does (`zena-run`, then the self-hosted compiler from the
checked-in bootstrap), writes each compile's glue with
[`gluegen`](zena/gluegen), which compiles the demo's own glue module from
the polyfill checkout, compiles each of the six components with Zena's
command line compiler, and builds the compiler component from the
polyfill's `zena/compiler` entry module. Its outputs are byte for byte
the ones measured here. The compiler component takes a few minutes and
about 6 GB of memory to build.

`rust/tests` drives the Rust elements through the polyfill and checks
them against the conduct elements' renders, events and styles:
`cd rust/tests && cargo test`.

## Workloads

Each element is measured three ways, on a fresh `Engine` each time it is
parsed:

- **parse**: `Component::new`, which compiles the core modules with
  Cranelift. Median of 7.
- **instantiate**: a fresh store, the linker's definitions, and
  `instantiate`. Median of 11.
- **workload**: N iterations in a fresh instance, timing the loop only.
  Creating and connecting the element is not timed. Median of 5.

The iterations, the same behaviour in each language:

- **todo-item** (N = 1000): created with todo-id, title, and completed
  `false`, and connected. Each iteration handles `toggle` (a `change`
  event, `checked` flipping), sets `completed` to match, and renders.
- **todo-input** (N = 1000): `has-todos` is set to `true` once. Each
  iteration handles a keystroke `keydown`, then an Enter `keydown` with
  the value `task <i>`, which emits `todo-add`, and renders.
- **todo-footer** (N = 1000): each iteration sets `active` to i,
  `completed` to i mod 3, and `filter` to all, active and completed in
  turn, and renders.
- **todo-app** (N = 100): against an in-process copy of the demo's API,
  through the demo page's `wasi:http/types` host. Each iteration handles
  `add` with a new title and renders. In every language the export
  returns only after the POST and the reload's GET, which the harness
  checks after each add.

After each run the harness checks that the work was done: the events
emitted and their details, the first and last renders, the API's todos,
and the number of items in the last render. Every run passed.

The languages differ in a few names and details, which the harness
follows. Zena's todo-input names its field's handler `key`, where
conduct and Rust name it `keydown`. Zena clears its "empty the field"
flag inside `render`, and conduct and Rust on the next update; each
iteration renders an empty field in all three. Zena does not key its
two inputs. Attribute order in a render differs. The request sequences
and JSON bodies of todo-app are the same.

## Results

Sizes, in bytes:

| | conduct | gzip -9 | Zena | gzip -9 | Rust "s" | gzip -9 | Rust "z" | gzip -9 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| todo-item | 28,966 | 12,386 | 67,902 | 27,229 | 59,476 | 23,104 | 58,159 | 22,837 |
| todo-input | 26,333 | 11,278 | 64,308 | 26,248 | 56,458 | 21,288 | 55,255 | 21,397 |
| todo-footer | 26,446 | 11,277 | 67,258 | 27,075 | 56,865 | 21,948 | 55,719 | 21,857 |
| todo-app | 51,317 | 20,931 | 146,121 | 58,731 | 173,079 | 66,768 | 165,324 | 65,505 |
| todos-route | 30,608 | 12,381 | 105,430 | 43,841 | | | | |
| todo-route | 29,846 | 12,180 | 101,292 | 42,489 | | | | |
| compiler | 3,808,196 | 1,135,771 | 2,439,483 | 1,010,786 | | | | |

Elements, in milliseconds; "first" is the first instantiation on an
engine, which the median leaves out, and "per iteration" is in
microseconds:

| element | language | parse | instantiate | first | workload | range | per iteration |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| todo-item | conduct | 25.8 | 0.041 | 4.1 | 36.7 | 35.7 to 45.4 | 37 |
| | Zena | 153.4 | 0.429 | 6.9 | 84.6 | 81.2 to 85.4 | 85 |
| | Rust "s" | 42.0 | 0.058 | 3.0 | 37.3 | 34.6 to 39.2 | 37 |
| | Rust "z" | 44.7 | 0.054 | 3.4 | 33.9 | 30.7 to 48.3 | 34 |
| todo-input | conduct | 25.0 | 0.033 | 3.4 | 38.4 | 37.8 to 43.8 | 38 |
| | Zena | 153.8 | 0.571 | 8.1 | 74.3 | 67.2 to 90.3 | 74 |
| | Rust "s" | 43.7 | 0.066 | 4.7 | 52.8 | 35.1 to 58.6 | 53 |
| | Rust "z" | 43.7 | 0.055 | 3.9 | 37.8 | 37.3 to 42.2 | 38 |
| todo-footer | conduct | 19.9 | 0.030 | 3.1 | 63.4 | 58.2 to 70.9 | 63 |
| | Zena | 172.9 | 0.496 | 7.5 | 113.6 | 103.0 to 139.8 | 114 |
| | Rust "s" | 42.8 | 0.058 | 3.9 | 52.0 | 46.8 to 71.5 | 52 |
| | Rust "z" | 49.1 | 0.066 | 3.9 | 57.0 | 48.9 to 57.7 | 57 |
| todo-app | conduct | 37.5 | 0.483 | 4.7 | 99.7 | 80.4 to 105.9 | 997 |
| | Zena | 316.6 | 0.715 | 6.0 | 219.2 | 172.7 to 256.8 | 2192 |
| | Rust "s" | 106.9 | 0.394 | 5.9 | 72.2 | 70.1 to 73.8 | 722 |
| | Rust "z" | 106.6 | 0.424 | 5.2 | 74.4 | 67.2 to 83.3 | 744 |

conduct and Rust are within the run-to-run variance of each other on
todo-item, todo-input and todo-footer, and Rust is ahead on todo-app in
this run; in an earlier run with the same components the two were even
on todo-app. Zena takes two to three times as long on every workload, in
every run. Rust's two optimization levels trade places from run to run.

Compilers, in milliseconds. A cold compile is the first in a fresh
instance; warm is the median of the next 5 in the same instance:

| | conduct | Zena |
| --- | ---: | ---: |
| parse | 2,380 | 4,518 |
| instantiate | 0.06 (first 5.9) | 4.8 (first 13.5) |

| source | conduct cold | conduct warm | Zena cold | Zena warm |
| --- | ---: | ---: | ---: | ---: |
| todo-item | 15.4 | 11.5 | 9,906 | 1,642 |
| todo-input | 11.6 | 10.3 | 9,079 | 1,600 |
| todo-footer | 10.3 | 10.1 | 9,723 | 1,654 |
| todo-app | 18.2 | 16.9 | 13,549 | 5,418 |
| todos-route | 15.6 | 12.1 | 12,867 | 2,124 |
| todo-route | 12.6 | 11.7 | 22,398 | 2,157 |

Every compile by the conduct compiler component returns the bytes the
library compiles natively. Zena's compiler component returns, on its
first compile in an instance, the bytes Zena's command line compiler
writes.

## Caveats

- **Native, not a browser.** These are Wasmtime's numbers. In a browser
  the polyfill runs core modules on the browser's engine, and each
  memory access of the canonical ABI is a JavaScript call, so the ratios
  may not carry over. Parse is Cranelift compiling the core modules, on 2
  vCPUs.
- **The polyfill's share.** Each call crosses the polyfill's dynamic
  `Val` path, which costs the same in every language. The harness does
  not measure that floor, so the differences between languages are
  smaller than the ratios of the totals.
- **Variance.** Workloads of 25 to 60 ms vary by about 20% from run to
  run, and the first run on an engine is often the slowest. Zena's warm
  compile of todo-app ranged from 2.9 to 10.9 seconds in this run.
  Zena's cold compile of todo-route took 20 to 22 seconds in every run,
  against about 13 for todos-route; why was not looked into.
- **Memory is not compared.** The polyfill exposes no way to read a
  guest's memory, and Zena allocates on the Wasm GC heap, while conduct
  and Rust use linear memory, so the two would not compare anyway.
- **Zena's glue.** A Zena element is its author's source plus the glue
  the demo writes around it, which exports the element interface through
  the authoring library. gluegen writes that glue from the demo's own
  code, so the components are the demo's.
- **Rust's standard library is not tuned for size.** It is built from
  source with `panic = "abort"` and LTO, but without `panic_immediate_abort`
  or other size options. Much of each Rust element's code is the standard
  library's formatting and panic code and the `wit-bindgen` async
  runtime.
- **Zena's repeated compiles.** Zena's compiler component keeps the
  parsed standard library from one compile to the next, which is why its
  warm compiles are faster. Its second and later compiles in an instance
  return slightly different bytes from the first: the GC types in the
  core module's type section come in a different order, and the size
  changes by up to about 230 bytes. They validate, and the four elements
  among them pass the workloads' checks.
- **No route workload.** The routes are compiled but not run.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
