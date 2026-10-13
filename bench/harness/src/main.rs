//! A native benchmark of the TodoMVC element components written in
//! conduct, Zena and Rust, and of the conduct and Zena compiler
//! components, all run on the Wasm Component Model Polyfill over its
//! Wasmtime core backend. See ../README.md.
//!
//! Run it with `cargo run --release [-- --root <bench dir>]`. It writes its
//! results as JSON and prints a table.
//!
//! Where it finds what it runs, all relative to the bench directory
//! (`--root`, or `BENCH_ROOT`, or the parent of this crate):
//!
//! | variable           | default                                     | what                              |
//! | ------------------ | ------------------------------------------- | --------------------------------- |
//! | (none)             | `../examples/*.cdt`                         | conduct sources, compiled here with `conduct::service::compile` |
//! | `CONDUCT_COMPILER` | `../compiler/target/conduct-compiler.wasm`  | the conduct compiler component    |
//! | `RUST_OUT`         | `rust/out`                                  | the Rust components, with `opt-z/` |
//! | `ZENA_DIR`         | none                                        | the Zena components and `zena-compiler.wasm`, from zena/build.sh |
//! | `ZENA_SOURCES`     | none                                        | `jobs/<name>/` and `stdlib/`, from zena/build.sh |
//! | `ZENA_STDLIB`      | `$ZENA_SOURCES/stdlib`                      | Zena's standard library           |
//! | `BENCH_OUT`        | `results.json`                              | where the results go              |
//!
//! Zena is skipped, with a message, when `ZENA_DIR` is unset or empty, and
//! its compiler when `ZENA_SOURCES` is too.
//!
//! Counts come from the environment, with these defaults:
//!
//! | variable          | default | what                                         |
//! | ----------------- | ------- | -------------------------------------------- |
//! | `PARSE_RUNS`      | 7       | `Component::new` runs per element            |
//! | `INSTANTIATE_RUNS`| 11      | link + instantiate runs per element          |
//! | `WORKLOAD_RUNS`   | 5       | workload runs, each in a fresh instance      |
//! | `N`               | 1000    | iterations of the item/input/footer workload |
//! | `N_APP`           | 100     | iterations of the todo-app workload          |
//! | `COMPILER_RUNS`   | 5       | parse, instantiate and warm compile runs     |
//! | `ONLY`            | both    | `elements` or `compilers`                    |
//! | `DUMP_DIR`        | none    | where to write each warm compile's output    |

mod support;
mod wasi;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use conduct::service::{self, World};

use serde_json::{Value, json};
use support::Host;
use wcmp::*;

type Res<T> = std::result::Result<T, String>;

/// Where the benchmark finds what it runs.
struct Config {
    root: PathBuf,
    examples: PathBuf,
    /// The conduct components, compiled from `examples` by the library.
    conduct: HashMap<String, Vec<u8>>,
    conduct_compiler: PathBuf,
    rust_out: PathBuf,
    /// The Zena components and compiler, or why there are none.
    zena_dir: Res<PathBuf>,
    /// The Zena compile jobs and the standard library, or why there are none.
    zena_sources: Res<(PathBuf, PathBuf)>,
    out: PathBuf,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

fn cfg() -> &'static Config {
    CONFIG.get().expect("the configuration is set first")
}

/// A path from the environment variable `name`, relative to `root` when
/// it is relative, or `default` under `root`.
fn path_var(root: &Path, name: &str, default: Option<&str>) -> Option<PathBuf> {
    let value = std::env::var(name).ok().filter(|v| !v.is_empty()).or(default.map(str::to_string))?;
    Some(root.join(value))
}

/// The six sources each compiler compiles, with their worlds.
const COMPILED: [(&str, &str); 6] = [
    ("todo-item", "element-component"),
    ("todo-input", "element-component"),
    ("todo-footer", "element-component"),
    ("todo-app", "element-component"),
    ("todos-route", "route-component"),
    ("todo-route", "route-component"),
];

impl Config {
    fn from_env() -> Res<Config> {
        let mut args = std::env::args().skip(1);
        let mut root = std::env::var("BENCH_ROOT").ok().map(PathBuf::from);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--root" => root = Some(PathBuf::from(args.next().ok_or("--root takes a directory")?)),
                other => return Err(format!("unknown argument {other}; the one flag is --root <bench dir>")),
            }
        }
        let root = root.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
        let root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
        let examples = root.join("../examples");
        let mut conduct = HashMap::new();
        for (name, world) in COMPILED {
            let path = examples.join(format!("{name}.cdt"));
            let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let world = World::named(world).expect("a world conduct knows");
            let bytes = service::compile(&source, world).map_err(|e| format!("{}: {e}", path.display()))?;
            conduct.insert(name.to_string(), bytes);
        }
        let zena_dir = match path_var(&root, "ZENA_DIR", None) {
            None => Err("ZENA_DIR is not set; build the Zena artifacts with zena/build.sh".to_string()),
            Some(dir) if !dir.join("todo-item.wasm").is_file() => {
                Err(format!("ZENA_DIR {} holds no todo-item.wasm; build it with zena/build.sh", dir.display()))
            }
            Some(dir) => Ok(dir),
        };
        let zena_sources = match path_var(&root, "ZENA_SOURCES", None) {
            None => Err("ZENA_SOURCES is not set; zena/build.sh writes it".to_string()),
            Some(dir) => {
                let stdlib = path_var(&root, "ZENA_STDLIB", None).unwrap_or_else(|| dir.join("stdlib"));
                if !dir.join("jobs").is_dir() {
                    Err(format!("ZENA_SOURCES {} holds no jobs/", dir.display()))
                } else if !stdlib.join("wit").is_dir() {
                    Err(format!("{} is not Zena's standard library (no wit/); set ZENA_STDLIB", stdlib.display()))
                } else {
                    Ok((dir, stdlib))
                }
            }
        };
        Ok(Config {
            conduct_compiler: path_var(&root, "CONDUCT_COMPILER", Some("../compiler/target/conduct-compiler.wasm")).unwrap(),
            rust_out: path_var(&root, "RUST_OUT", Some("rust/out")).unwrap(),
            out: path_var(&root, "BENCH_OUT", Some("results.json")).unwrap(),
            examples,
            conduct,
            zena_dir,
            zena_sources,
            root,
        })
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The median of `samples`, in milliseconds, and the samples.
fn stats(samples: &[Duration]) -> Value {
    let mut sorted: Vec<f64> = samples.iter().copied().map(ms).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if sorted.is_empty() {
        f64::NAN
    } else if sorted.len() % 2 == 1 {
        sorted[sorted.len() / 2]
    } else {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    };
    json!({
        "median_ms": round(median),
        "min_ms": round(*sorted.first().unwrap_or(&f64::NAN)),
        "max_ms": round(*sorted.last().unwrap_or(&f64::NAN)),
        "samples_ms": samples.iter().copied().map(|d| round(ms(d))).collect::<Vec<_>>(),
    })
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn median_of(v: &Value) -> f64 {
    v["median_ms"].as_f64().unwrap_or(f64::NAN)
}

fn engine() -> Engine {
    Engine::with_backend(wcmp_wasm_core_wasmtime::Wasmtime::new().expect("a Wasmtime backend")).expect("an engine")
}

fn imports(component: &Component, name: &str) -> bool {
    component.imports.iter().any(|import| import.name.to_string() == name)
}

// ---------------------------------------------------------------------------
// Rendering a node list as text, for the sanity checks (not timed).

fn field<'a>(v: &'a Val, name: &str) -> &'a Val {
    let Val::Record(fields) = v else { panic!("expected a record, found {v:?}") };
    &fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}")).value
}

fn string(v: &Val) -> String {
    match v {
        Val::String(s) => s.clone(),
        other => panic!("expected a string, found {other:?}"),
    }
}

fn pairs(v: &Val) -> Vec<(String, Val)> {
    let Val::List(items) = v else { panic!("expected a list, found {v:?}") };
    items
        .iter()
        .map(|t| {
            let Val::Tuple(parts) = t else { panic!("expected a tuple, found {t:?}") };
            (string(&parts[0]), parts[1].clone())
        })
        .collect()
}

/// A rendered node list as HTML-like text: the key as `#key`, properties
/// as `.name=value`, events as `@event=handler`.
fn html(rendered: &[Val]) -> String {
    let [Val::List(nodes)] = rendered else { panic!("render returned {rendered:?}") };
    fn go(nodes: &[Val], i: usize, out: &mut String) {
        let node = &nodes[i];
        let Val::Variant { discriminant, payload } = field(node, "kind") else { panic!() };
        let payload = payload.as_deref().unwrap();
        if discriminant == "text" {
            out.push_str(&string(payload));
            return;
        }
        let tag = string(field(payload, "tag"));
        out.push('<');
        out.push_str(&tag);
        if let Val::Option(Some(key)) = field(node, "key") {
            out.push_str(&format!(" #{}", string(key)));
        }
        for (name, value) in pairs(field(payload, "attributes")) {
            out.push_str(&format!(" {name}=\"{}\"", string(&value)));
        }
        for (name, value) in pairs(field(payload, "properties")) {
            let Val::Variant { discriminant, payload } = value else { panic!() };
            let shown = match (discriminant.as_str(), payload.as_deref()) {
                ("text", Some(v)) => format!("\"{}\"", string(v)),
                ("flag", Some(Val::Bool(b))) => b.to_string(),
                other => panic!("unexpected property {other:?}"),
            };
            out.push_str(&format!(" .{name}={shown}"));
        }
        for (event, handler) in pairs(field(payload, "events")) {
            out.push_str(&format!(" @{event}={}", string(&handler)));
        }
        out.push('>');
        for (j, child) in nodes.iter().enumerate() {
            if field(child, "parent") == &Val::Option(Some(Box::new(Val::U32(i as u32)))) {
                go(nodes, j, out);
            }
        }
        out.push_str(&format!("</{tag}>"));
    }
    if nodes.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    go(nodes, 0, &mut out);
    out
}

fn node_count(rendered: &[Val]) -> usize {
    match rendered {
        [Val::List(nodes)] => nodes.len(),
        _ => 0,
    }
}

fn event(kind: &str, value: Option<&str>, checked: Option<bool>, key: Option<&str>) -> Val {
    let opt = |s: Option<&str>| Val::Option(s.map(|s| Box::new(Val::String(s.into()))));
    Val::Record(
        vec![
            ValField { name: "kind".into(), value: Val::String(kind.into()) },
            ValField { name: "value".into(), value: opt(value) },
            ValField { name: "checked".into(), value: Val::Option(checked.map(|c| Box::new(Val::Bool(c)))) },
            ValField { name: "key".into(), value: opt(key) },
        ]
        .into(),
    )
}

// ---------------------------------------------------------------------------
// The languages, and an element instance.

#[derive(Clone, Copy)]
struct Lang {
    name: &'static str,
    /// todo-input's handler for the field's `keydown`: conduct and the
    /// Rust port name it `keydown`, Zena `key`.
    input_key: &'static str,
}

const LANGS: [Lang; 4] = [
    Lang { name: "conduct", input_key: "keydown" },
    Lang { name: "zena", input_key: "key" },
    Lang { name: "rust-s", input_key: "keydown" },
    Lang { name: "rust-z", input_key: "keydown" },
];

/// The component of `element` in `lang`, and where it came from.
fn element_bytes(lang: Lang, element: &str) -> Res<(Vec<u8>, String)> {
    let config = cfg();
    let path = match lang.name {
        "conduct" => {
            let bytes = config.conduct[element].clone();
            return Ok((bytes, format!("{}/{element}.cdt, compiled by conduct::service::compile", config.examples.display())));
        }
        "zena" => config.zena_dir.clone()?.join(format!("{element}.wasm")),
        "rust-s" => config.rust_out.join(format!("{element}.wasm")),
        "rust-z" => config.rust_out.join(format!("opt-z/{element}.wasm")),
        _ => unreachable!(),
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e} (run rust/build.sh or zena/build.sh)", path.display()))?;
    Ok((bytes, path.display().to_string()))
}

const ELEMENTS: [&str; 4] = ["todo-item", "todo-input", "todo-footer", "todo-app"];

/// Define everything an element component imports: `demo:element/host`,
/// `wasi:http/{types,client}` if imported, and Zena's WASI functions if
/// imported.
fn element_linker(engine: &Engine, component: &Component, store: &mut Store<Host>) -> Res<Linker<Host>> {
    let mut linker: Linker<Host> = Linker::new(engine);
    let types = support::define_http(&mut linker, component).map_err(|e| format!("define http: {e}"))?;
    store.data_mut().http.set_types(types);
    if imports(component, "demo:element/host") {
        let host: InterfaceIdentifier = "demo:element/host".parse().unwrap();
        linker
            .instance(&host)
            .func_wrap(
                "emit",
                |mut call: HostCall<'_, Host>, (id, name, detail): (u32, String, String)| -> wcmp::Result<()> {
                    call.data_mut().emitted.push((id, name, detail));
                    Ok(())
                },
            )
            .map_err(|e| format!("define emit: {e}"))?;
    }
    if imports(component, "wasi:cli/stdout@0.3.0") || imports(component, "wasi:clocks/monotonic-clock@0.3.0") {
        wasi::define(&mut linker, "element").map_err(|e| format!("define wasi: {e}"))?;
    }
    Ok(linker)
}

struct El {
    store: Store<Host>,
    create: Func,
    connected: Func,
    attribute_changed: Func,
    handle_event: Func,
    render: Func,
}

impl El {
    async fn new(engine: &Engine, component: &Component, host: Host) -> Res<El> {
        let mut store = Store::new(engine, host).map_err(|e| format!("store: {e}"))?;
        let linker = element_linker(engine, component, &mut store)?;
        let instance = linker.instantiate(&mut store, component).await.map_err(|e| format!("instantiate: {e}"))?;
        let iface = instance.exports().instance("demo:element/element").ok_or("no element interface")?;
        let f = |name: &str| iface.func(name).ok_or_else(|| format!("no export {name}"));
        Ok(El {
            create: f("create")?,
            connected: f("connected")?,
            attribute_changed: f("attribute-changed")?,
            handle_event: f("handle-event")?,
            render: f("render")?,
            store,
        })
    }

    async fn create(&mut self, attrs: &[(&str, &str)]) -> Res<u32> {
        let list = attrs
            .iter()
            .map(|(k, v)| Val::Tuple(vec![Val::String((*k).into()), Val::String((*v).into())].into()))
            .collect::<Vec<_>>();
        let out = self.create.call(&mut self.store, &[Val::List(list.into())]).await.map_err(|e| format!("create: {e}"))?;
        match &out[..] {
            [Val::U32(id)] => Ok(*id),
            other => Err(format!("create returned {other:?}")),
        }
    }

    async fn connected(&mut self, id: u32) -> Res<()> {
        self.connected.call(&mut self.store, &[Val::U32(id)]).await.map_err(|e| format!("connected: {e}"))?;
        Ok(())
    }

    async fn set_attr(&mut self, id: u32, name: &str, value: &str) -> Res<()> {
        let args = [Val::U32(id), Val::String(name.into()), Val::Option(Some(Box::new(Val::String(value.into()))))];
        self.attribute_changed.call(&mut self.store, &args).await.map_err(|e| format!("attribute-changed: {e}"))?;
        Ok(())
    }

    async fn handle(&mut self, id: u32, handler: &str, event: Val) -> Res<()> {
        let args = [Val::U32(id), Val::String(handler.into()), event];
        self.handle_event.call(&mut self.store, &args).await.map_err(|e| format!("handle-event {handler}: {e}"))?;
        Ok(())
    }

    async fn render(&mut self, id: u32) -> Res<Box<[Val]>> {
        self.render.call(&mut self.store, &[Val::U32(id)]).await.map_err(|e| format!("render: {e}"))
    }

    fn host(&mut self) -> &mut Host {
        self.store.data_mut()
    }
}

fn check(ok: bool, what: impl Into<String>, failures: &mut Vec<String>) {
    if !ok {
        failures.push(what.into());
    }
}

// ---------------------------------------------------------------------------
// The workloads. Each returns the time of its loop alone (setup, such as
// create and connected, is not timed) and a sanity report.

const FILTERS: [&str; 3] = ["all", "active", "completed"];

async fn todo_item(el: &mut El, _lang: Lang, n: usize) -> Res<(Duration, Value)> {
    let id = el.create(&[("todo-id", "1"), ("title", "Buy milk"), ("completed", "false")]).await?;
    el.connected(id).await?;
    let mut done = false;
    let mut first = None;
    let mut last = None;
    let mut nodes = 0;
    let started = Instant::now();
    for i in 0..n {
        done = !done;
        el.handle(id, "toggle", event("change", None, Some(done), None)).await?;
        el.set_attr(id, "completed", if done { "true" } else { "false" }).await?;
        let rendered = el.render(id).await?;
        nodes += node_count(&rendered);
        if i == 0 {
            first = Some(rendered);
        } else if i == n - 1 {
            last = Some(rendered);
        }
    }
    let elapsed = started.elapsed();
    let emitted = std::mem::take(&mut el.host().emitted);
    let (first, last) = (html(&first.unwrap()), html(&last.unwrap()));
    let mut failures = vec![];
    check(emitted.len() == n, format!("{} emitted, expected {n}", emitted.len()), &mut failures);
    check(
        emitted.iter().all(|(i, name, detail)| *i == id && name == "todo-toggle" && detail == "1"),
        "an emitted event is not (id, todo-toggle, 1)",
        &mut failures,
    );
    check(first.contains("Buy milk") && first.contains("completed"), "first render is not completed", &mut failures);
    let last_done = n % 2 == 1;
    check(last.contains("completed") == last_done, "last render's completed state is wrong", &mut failures);
    Ok((
        elapsed,
        json!({"emitted": emitted.len(), "nodes_rendered": nodes, "first_render": first, "last_render": last, "failures": failures}),
    ))
}

async fn todo_input(el: &mut El, lang: Lang, n: usize) -> Res<(Duration, Value)> {
    let id = el.create(&[]).await?;
    el.connected(id).await?;
    el.set_attr(id, "has-todos", "true").await?;
    let mut last = None;
    let mut nodes = 0;
    let titles: Vec<String> = (0..n).map(|i| format!("task {i}")).collect();
    let started = Instant::now();
    for (i, title) in titles.iter().enumerate() {
        el.handle(id, lang.input_key, event("keydown", Some("task"), None, Some("k"))).await?;
        el.handle(id, lang.input_key, event("keydown", Some(title), None, Some("Enter"))).await?;
        let rendered = el.render(id).await?;
        nodes += node_count(&rendered);
        if i == n - 1 {
            last = Some(rendered);
        }
    }
    let elapsed = started.elapsed();
    let emitted = std::mem::take(&mut el.host().emitted);
    let last = html(&last.unwrap());
    let mut failures = vec![];
    check(emitted.len() == n, format!("{} emitted, expected {n}", emitted.len()), &mut failures);
    check(
        emitted.iter().zip(&titles).all(|((i, name, detail), title)| *i == id && name == "todo-add" && detail == title),
        "an emitted event is not (id, todo-add, task i)",
        &mut failures,
    );
    check(last.contains("toggle-all"), "last render has no toggle-all", &mut failures);
    check(last.contains("value=\"\""), "last render does not clear the field", &mut failures);
    Ok((elapsed, json!({"emitted": emitted.len(), "nodes_rendered": nodes, "last_render": last, "failures": failures})))
}

async fn todo_footer(el: &mut El, _lang: Lang, n: usize) -> Res<(Duration, Value)> {
    let id = el.create(&[("active", "0"), ("completed", "0"), ("filter", "all")]).await?;
    el.connected(id).await?;
    let actives: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let completeds: Vec<String> = (0..n).map(|i| (i % 3).to_string()).collect();
    let mut second = None;
    let mut last = None;
    let mut nodes = 0;
    let started = Instant::now();
    for i in 0..n {
        el.set_attr(id, "active", &actives[i]).await?;
        el.set_attr(id, "completed", &completeds[i]).await?;
        el.set_attr(id, "filter", FILTERS[i % 3]).await?;
        let rendered = el.render(id).await?;
        nodes += node_count(&rendered);
        if i == 1 {
            second = Some(rendered);
        } else if i == n - 1 {
            last = Some(rendered);
        }
    }
    let elapsed = started.elapsed();
    let emitted = std::mem::take(&mut el.host().emitted);
    let (second, last) = (html(&second.unwrap()), html(&last.unwrap()));
    let mut failures = vec![];
    check(emitted.is_empty(), "the footer emitted", &mut failures);
    check(second.contains("<strong>1</strong>") && second.contains("item left"), "second render's count", &mut failures);
    check(second.contains("Clear completed"), "second render has no clear button", &mut failures);
    check(last.contains(&format!("<strong>{}</strong>", n - 1)), "last render's count", &mut failures);
    check(last.contains("Clear completed") == ((n - 1) % 3 != 0), "last render's clear button", &mut failures);
    Ok((elapsed, json!({"nodes_rendered": nodes, "second_render": second, "last_render": last, "failures": failures})))
}

async fn todo_app(el: &mut El, _lang: Lang, n: usize) -> Res<(Duration, Value)> {
    let id = el.create(&[("filter", "")]).await?;
    el.connected(id).await?;
    if el.host().requests.len() != 1 {
        return Err(format!("connected made requests {:?}", el.host().requests));
    }
    let titles: Vec<String> = (0..n).map(|i| format!("todo {i}")).collect();
    let mut last = None;
    let mut nodes = 0;
    let mut not_reloaded = 0;
    let started = Instant::now();
    for (i, title) in titles.iter().enumerate() {
        el.handle(id, "add", event("todo-add", Some(title), None, None)).await?;
        // The list has reloaded when the API has answered the POST and
        // the GET after it. Every language's export returns only then.
        let requests = &el.host().requests;
        if requests.len() != 3 + 2 * i || !requests[requests.len() - 1].starts_with("GET /api/todos") {
            not_reloaded += 1;
        }
        let rendered = el.render(id).await?;
        nodes += node_count(&rendered);
        if i == n - 1 {
            last = Some(rendered);
        }
    }
    let elapsed = started.elapsed();
    let last = html(&last.unwrap());
    let host = el.host();
    let mut failures = vec![];
    check(not_reloaded == 0, format!("{not_reloaded} adds returned before the list reloaded"), &mut failures);
    check(host.api.todos.len() == n, format!("the API holds {} todos", host.api.todos.len()), &mut failures);
    check(last.matches("<todo-item").count() == n, format!("last render shows {} items", last.matches("<todo-item").count()), &mut failures);
    check(last.contains(&format!("todo {}", n - 1)), "last render lacks the last title", &mut failures);
    check(last.contains(&format!("active=\"{n}\"")), "last render's footer count", &mut failures);
    let requests = host.requests.len();
    let sample: Vec<String> = host.requests.iter().take(3).cloned().collect();
    Ok((
        elapsed,
        json!({
            "requests": requests,
            "first_requests": sample,
            "nodes_rendered": nodes,
            "last_render_bytes": last.len(),
            "failures": failures,
        }),
    ))
}

async fn workload(element: &str, el: &mut El, lang: Lang, n: usize) -> Res<(Duration, Value)> {
    match element {
        "todo-item" => todo_item(el, lang, n).await,
        "todo-input" => todo_input(el, lang, n).await,
        "todo-footer" => todo_footer(el, lang, n).await,
        "todo-app" => todo_app(el, lang, n).await,
        _ => unreachable!(),
    }
}

async fn bench_element(lang: Lang, element: &str) -> Res<Value> {
    let (bytes, source) = element_bytes(lang, element)?;
    let parse_runs = env_usize("PARSE_RUNS", 7);
    let instantiate_runs = env_usize("INSTANTIATE_RUNS", 11);
    let workload_runs = env_usize("WORKLOAD_RUNS", 5);
    let n = if element == "todo-app" { env_usize("N_APP", 100) } else { env_usize("N", 1000) };

    // 1. parse: `Component::new` on a fresh engine.
    let mut parse = vec![];
    let mut parsed = None;
    for _ in 0..parse_runs {
        let engine = engine();
        let started = Instant::now();
        let component = Component::new(&engine, &bytes).await.map_err(|e| format!("parse: {e}"))?;
        parse.push(started.elapsed());
        parsed = Some((engine, component));
    }
    let (engine, component) = parsed.unwrap();

    // 2. instantiate: a fresh store, the linker's definitions, and
    //    `instantiate`.
    let mut instantiate = vec![];
    for _ in 0..instantiate_runs {
        let started = Instant::now();
        let mut store = Store::new(&engine, Host::default()).map_err(|e| format!("store: {e}"))?;
        let linker = element_linker(&engine, &component, &mut store)?;
        let instance = linker.instantiate(&mut store, &component).await.map_err(|e| format!("instantiate: {e}"))?;
        instantiate.push(started.elapsed());
        drop((instance, store));
    }

    // 3. the workload, each run in a fresh instance.
    let mut runs = vec![];
    let mut sanity = vec![];
    for _ in 0..workload_runs {
        let mut el = El::new(&engine, &component, Host::default()).await?;
        let (elapsed, report) = workload(element, &mut el, lang, n).await?;
        runs.push(elapsed);
        sanity.push(report);
    }
    let failures: Vec<Value> = sanity.iter().flat_map(|s| s["failures"].as_array().cloned().unwrap_or_default()).collect();
    let work = stats(&runs);
    let per_iteration_us = round(median_of(&work) * 1000.0 / n as f64);
    Ok(json!({
        "component": source,
        "bytes": bytes.len(),
        "parse": stats(&parse),
        "instantiate": stats(&instantiate),
        "workload": {"n": n, "total": work, "per_iteration_us": per_iteration_us},
        "sanity_ok": failures.is_empty(),
        "sanity_failures": failures,
        "sanity": sanity[0],
    }))
}

// ---------------------------------------------------------------------------
// The compilers.

/// What the Zena compiler's `read-source` answers from: the files of the
/// compile's job directory at their relative paths, and Zena's standard
/// library under `/stdlib`. Every path asked for is kept.
#[derive(Default)]
struct Sources {
    files: HashMap<String, String>,
    asked: Vec<(String, bool)>,
}

fn load_dir(root: &Path, prefix: &str, files: &mut HashMap<String, String>) {
    fn walk(dir: &Path, root: &Path, prefix: &str, files: &mut HashMap<String, String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, prefix, files);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                files.insert(format!("{prefix}{rel}"), text);
            }
        }
    }
    walk(root, root, prefix, files);
}

struct Job {
    name: String,
    dir: PathBuf,
    entry: String,
    world: String,
}

fn zena_job(name: &str) -> Job {
    let (sources, _) = cfg().zena_sources.as_ref().expect("Zena's sources are checked before a compile");
    let dir = sources.join("jobs").join(name);
    let job = std::fs::read_to_string(dir.join("job")).unwrap_or_else(|e| panic!("{}/job: {e}", dir.display()));
    let mut lines = job.lines();
    Job { name: name.to_string(), entry: lines.next().unwrap().to_string(), world: lines.next().unwrap().to_string(), dir }
}

fn zena_compiler_linker(engine: &Engine) -> Res<Linker<Sources>> {
    let mut linker: Linker<Sources> = Linker::new(engine);
    wasi::define(&mut linker, "zena compiler").map_err(|e| format!("define wasi: {e}"))?;
    linker
        .instance(&"wcmp:zena-compiler/host".parse().unwrap())
        .func_wrap("read-source", |mut call: HostCall<'_, Sources>, (path,): (String,)| {
            let sources = call.data_mut();
            let key = path.strip_prefix("./").unwrap_or(&path).to_string();
            let found = sources.files.get(&key).cloned();
            sources.asked.push((path, found.is_some()));
            Ok(found)
        })
        .map_err(|e| format!("define read-source: {e}"))?;
    Ok(linker)
}

fn bytes_of(list: &Val) -> Res<Vec<u8>> {
    let Val::List(items) = list else { return Err(format!("not a list: {list:?}")) };
    items.iter().map(|v| if let Val::U8(b) = v { Ok(*b) } else { Err(format!("not a byte: {v:?}")) }).collect()
}

/// How the output of a compile compares with the reference component.
fn compare(output: &[u8], reference: Res<(Vec<u8>, String)>) -> Value {
    match reference {
        Ok((reference, reference_path)) if reference == output => json!({"reference": reference_path, "identical": true}),
        Ok((reference, reference_path)) => {
            let first_difference = output.iter().zip(&reference).position(|(a, b)| a != b);
            json!({
                "reference": reference_path,
                "identical": false,
                "output_bytes": output.len(),
                "reference_bytes": reference.len(),
                "first_difference_at": first_difference,
            })
        }
        Err(e) => json!({"reference": null, "error": e}),
    }
}

trait CompilerKind {
    type Data: Send + 'static;
    const NAME: &'static str;
    /// The compiler component, or why there is none.
    fn path() -> Res<PathBuf>;
    fn linker(engine: &Engine) -> Res<Linker<Self::Data>>;
    fn data() -> Self::Data;
    /// The compile export.
    fn func(instance: &Instance) -> Res<Func>;
    /// Prepare the store for compiling `name` and return the arguments.
    fn prepare(store: &mut Store<Self::Data>, name: &str, world: &str) -> Res<Vec<Val>>;
    /// The bytes, or the compiler's error as text.
    fn output(result: &Val) -> Res<Vec<u8>>;
    /// The component the compile of `name` should return, and where it
    /// came from.
    fn reference(name: &str) -> Res<(Vec<u8>, String)>;
    fn after_cold(_store: &mut Store<Self::Data>) -> Value {
        Value::Null
    }
}

struct Conduct;

impl CompilerKind for Conduct {
    type Data = ();
    const NAME: &'static str = "conduct";
    fn path() -> Res<PathBuf> {
        let path = cfg().conduct_compiler.clone();
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("no conduct compiler component at {}; build it with compiler/build.sh", path.display()))
        }
    }
    fn linker(engine: &Engine) -> Res<Linker<()>> {
        Ok(Linker::new(engine))
    }
    fn data() {}
    fn func(instance: &Instance) -> Res<Func> {
        instance
            .exports()
            .instance("conduct:compiler/compiler")
            .and_then(|i| i.func("compile"))
            .ok_or_else(|| "no conduct:compiler/compiler#compile".into())
    }
    fn prepare(_store: &mut Store<()>, name: &str, world: &str) -> Res<Vec<Val>> {
        let path = cfg().examples.join(format!("{name}.cdt"));
        let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(vec![Val::String(source), Val::String(world.into())])
    }
    fn output(result: &Val) -> Res<Vec<u8>> {
        match result {
            Val::Result(Ok(Some(list))) => bytes_of(list),
            other => Err(format!("compile failed: {other:?}")),
        }
    }
    fn reference(name: &str) -> Res<(Vec<u8>, String)> {
        Ok((cfg().conduct[name].clone(), format!("{name}.cdt compiled natively by conduct::service::compile")))
    }
}

struct Zena;

impl CompilerKind for Zena {
    type Data = Sources;
    const NAME: &'static str = "zena";
    fn path() -> Res<PathBuf> {
        let config = cfg();
        let path = config.zena_dir.clone()?.join("zena-compiler.wasm");
        config.zena_sources.as_ref().map_err(Clone::clone)?;
        if path.is_file() { Ok(path) } else { Err(format!("no Zena compiler component at {}", path.display())) }
    }
    fn linker(engine: &Engine) -> Res<Linker<Sources>> {
        zena_compiler_linker(engine)
    }
    fn data() -> Sources {
        Sources::default()
    }
    fn func(instance: &Instance) -> Res<Func> {
        instance.get_func("compile").ok_or_else(|| "no compile export".into())
    }
    fn prepare(store: &mut Store<Sources>, name: &str, world: &str) -> Res<Vec<Val>> {
        let job = zena_job(name);
        if job.world != world {
            return Err(format!("{}: job world {} is not {world}", job.name, job.world));
        }
        let mut files = HashMap::new();
        let (_, stdlib) = cfg().zena_sources.as_ref().map_err(Clone::clone)?;
        load_dir(stdlib, "/stdlib/", &mut files);
        // The job's sources, without the outputs other runs left there.
        load_dir(&job.dir, "", &mut files);
        files.retain(|path, _| !path.ends_with(".wasm") && path != "job");
        // A directory of WIT reads as Zena's `readWitSource` makes it:
        // every `.wit` file in the directory, in name order, each followed
        // by a newline.
        let mut wit_dirs: HashMap<String, Vec<String>> = HashMap::new();
        for path in files.keys().filter(|p| p.ends_with(".wit")) {
            if let Some((dir, _)) = path.rsplit_once('/') {
                wit_dirs.entry(dir.to_string()).or_default().push(path.clone());
            }
        }
        for (dir, mut paths) in wit_dirs {
            paths.sort();
            let text: String = paths.iter().map(|p| format!("{}\n", files[p])).collect();
            files.insert(dir, text);
        }
        let glue = files.get(&job.entry).cloned().ok_or("no glue")?;
        let wit = files.get("world.wit").cloned().ok_or("no world.wit")?;
        let data = store.data_mut();
        data.files = files;
        data.asked.clear();
        Ok(vec![Val::String(glue), Val::String(job.entry), Val::String(wit), Val::String(job.world)])
    }
    fn output(result: &Val) -> Res<Vec<u8>> {
        match result {
            Val::Result(Ok(Some(list))) => bytes_of(list),
            Val::Result(Err(Some(text))) => Err(format!("compile failed: {}", string(text))),
            other => Err(format!("compile returned {other:?}")),
        }
    }
    fn reference(name: &str) -> Res<(Vec<u8>, String)> {
        let path = cfg().zena_dir.clone()?.join(format!("{name}.wasm"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((bytes, path.display().to_string()))
    }
    fn after_cold(store: &mut Store<Sources>) -> Value {
        let asked = &store.data().asked;
        let missing: Vec<&String> = asked.iter().filter(|(_, found)| !found).map(|(p, _)| p).collect();
        let mut unique: Vec<&String> = asked.iter().map(|(p, _)| p).collect();
        unique.sort();
        unique.dedup();
        json!({"reads": asked.len(), "distinct_paths": unique.len(), "missing": missing})
    }
}

async fn compiler_instance<K: CompilerKind>(engine: &Engine, component: &Component) -> Res<(Store<K::Data>, Func)> {
    let mut store = Store::new(engine, K::data()).map_err(|e| format!("store: {e}"))?;
    let linker = K::linker(engine)?;
    let instance = linker.instantiate(&mut store, component).await.map_err(|e| format!("instantiate: {e}"))?;
    let func = K::func(&instance)?;
    Ok((store, func))
}

async fn compile_once<K: CompilerKind>(store: &mut Store<K::Data>, func: &Func, args: &[Val]) -> Res<(Duration, Vec<u8>)> {
    let started = Instant::now();
    let result = func.call(&mut *store, args).await.map_err(|e| format!("compile call: {e}"))?;
    let elapsed = started.elapsed();
    let bytes = K::output(result.first().ok_or("no result")?)?;
    Ok((elapsed, bytes))
}

async fn bench_compiler<K: CompilerKind>() -> Res<Value> {
    let path = K::path()?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let runs = env_usize("COMPILER_RUNS", 5);
    let mut parse = vec![];
    let mut parsed = None;
    for _ in 0..runs {
        let engine = engine();
        let started = Instant::now();
        let component = Component::new(&engine, &bytes).await.map_err(|e| format!("parse: {e}"))?;
        parse.push(started.elapsed());
        parsed = Some((engine, component));
    }
    let (engine, component) = parsed.unwrap();
    let mut instantiate = vec![];
    for _ in 0..runs {
        let started = Instant::now();
        let instance = compiler_instance::<K>(&engine, &component).await?;
        instantiate.push(started.elapsed());
        drop(instance);
    }
    let mut sources = serde_json::Map::new();
    for (name, world) in COMPILED {
        eprintln!("  {} compiles {name}", K::NAME);
        let result: Res<Value> = async {
            // Cold: the first compile in a fresh instance.
            let (mut store, func) = compiler_instance::<K>(&engine, &component).await?;
            let args = K::prepare(&mut store, name, world)?;
            let (cold, output) = compile_once::<K>(&mut store, &func, &args).await?;
            let reads = K::after_cold(&mut store);
            let mut warm = vec![];
            let mut same = true;
            let mut warm_outputs = vec![];
            let dump = std::env::var("DUMP_DIR").ok();
            for run in 0..runs {
                let args = K::prepare(&mut store, name, world)?;
                let (elapsed, again) = compile_once::<K>(&mut store, &func, &args).await?;
                same &= again == output;
                warm.push(elapsed);
                if let Some(dir) = &dump {
                    let path = format!("{dir}/{}-{name}-warm{run}.wasm", K::NAME);
                    std::fs::write(&path, &again).map_err(|e| format!("{path}: {e}"))?;
                }
                warm_outputs.push(json!({
                    "bytes": again.len(),
                    "identical_to_cold": again == output,
                    "first_difference_at": again.iter().zip(&output).position(|(a, b)| a != b),
                }));
            }
            Ok(json!({
                "world": world,
                "cold_ms": round(ms(cold)),
                "warm": stats(&warm),
                "output_bytes": output.len(),
                "warm_outputs_identical_to_cold": same,
                "warm_outputs": warm_outputs,
                "matches_reference": compare(&output, K::reference(name)),
                "source_reads": reads,
            }))
        }
        .await;
        sources.insert(name.into(), result.unwrap_or_else(|e| json!({"error": e})));
    }
    Ok(json!({
        "component": path.display().to_string(),
        "bytes": bytes.len(),
        "parse": stats(&parse),
        "instantiate": stats(&instantiate),
        "parse_plus_instantiate_median_ms": round(median_of(&stats(&parse)) + median_of(&stats(&instantiate))),
        "sources": sources,
    }))
}

/// The results of compiler `K`, or why it was skipped.
async fn compiler_entry<K: CompilerKind>() -> Value {
    if let Err(e) = K::path() {
        eprintln!("{} compiler skipped: {e}", K::NAME);
        return json!({"skipped": e});
    }
    eprintln!("{} compiler", K::NAME);
    bench_compiler::<K>().await.unwrap_or_else(|e| json!({"error": e}))
}

// ---------------------------------------------------------------------------

fn print_tables(results: &Value) {
    if let Some(elements) = results.get("elements") {
        println!(
            "\n{:<12} {:<8} {:>8} {:>10} {:>12} {:>6} {:>12} {:>10} {}",
            "element", "lang", "bytes", "parse ms", "instant. ms", "N", "workload ms", "us/iter", "sanity"
        );
        for element in ELEMENTS {
            for lang in LANGS {
                let r = &elements[element][lang.name];
                if let Some(e) = r.get("skipped") {
                    println!("{element:<12} {:<8} skipped: {}", lang.name, e.as_str().unwrap_or_default());
                    continue;
                }
                if let Some(e) = r.get("error") {
                    println!("{element:<12} {:<8} error: {e}", lang.name);
                    continue;
                }
                println!(
                    "{element:<12} {:<8} {:>8} {:>10.2} {:>12.3} {:>6} {:>12.2} {:>10.1} {}",
                    lang.name,
                    r["bytes"],
                    median_of(&r["parse"]),
                    median_of(&r["instantiate"]),
                    r["workload"]["n"],
                    median_of(&r["workload"]["total"]),
                    r["workload"]["per_iteration_us"].as_f64().unwrap_or(f64::NAN),
                    if r["sanity_ok"] == true { "ok" } else { "FAILED" },
                );
            }
        }
    }
    if let Some(compilers) = results.get("compilers") {
        for (lang, r) in compilers.as_object().unwrap() {
            if let Some(e) = r.get("skipped") {
                println!("\n{lang} compiler: skipped: {}", e.as_str().unwrap_or_default());
                continue;
            }
            if let Some(e) = r.get("error") {
                println!("\n{lang} compiler: error: {e}");
                continue;
            }
            println!(
                "\n{lang} compiler: {} bytes, parse {:.1} ms, instantiate {:.2} ms",
                r["bytes"],
                median_of(&r["parse"]),
                median_of(&r["instantiate"])
            );
            println!("  {:<12} {:>10} {:>14} {:>10} {}", "source", "cold ms", "warm med. ms", "out bytes", "identical to reference");
            for (name, s) in r["sources"].as_object().unwrap() {
                if let Some(e) = s.get("error") {
                    println!("  {name:<12} error: {e}");
                    continue;
                }
                println!(
                    "  {name:<12} {:>10.1} {:>14.1} {:>10} {}",
                    s["cold_ms"].as_f64().unwrap_or(f64::NAN),
                    median_of(&s["warm"]),
                    s["output_bytes"],
                    s["matches_reference"]["identical"]
                );
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("element-bench: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = &config.zena_dir {
        eprintln!("Zena's elements and compiler are skipped: {e}");
    } else if let Err(e) = &config.zena_sources {
        eprintln!("Zena's compiler is skipped: {e}");
    }
    let conduct_commit = std::process::Command::new("git")
        .arg("-C")
        .arg(&config.root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    CONFIG.set(config).ok().expect("the configuration is set once");
    let only = std::env::var("ONLY").unwrap_or_default();
    let mut results = serde_json::Map::new();
    results.insert(
        "meta".into(),
        json!({
            "host": "native, release build of this harness (opt-level 3), tokio current-thread runtime",
            "polyfill": "dialog-db/wasm-component-model-polyfill eedcd22766b35cc34b3b81b4c753ab4f08a7c09a, Wasmtime core backend",
            "wasmtime": "49.0.0-rc.1",
            "conduct_commit": conduct_commit,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            "runs": {
                "parse": env_usize("PARSE_RUNS", 7),
                "instantiate": env_usize("INSTANTIATE_RUNS", 11),
                "workload": env_usize("WORKLOAD_RUNS", 5),
                "compiler": env_usize("COMPILER_RUNS", 5),
            },
            "n": env_usize("N", 1000),
            "n_app": env_usize("N_APP", 100),
        }),
    );
    if only.is_empty() || only == "elements" {
        let mut elements = serde_json::Map::new();
        for element in ELEMENTS {
            let mut by_lang = serde_json::Map::new();
            for lang in LANGS {
                let r = match &cfg().zena_dir {
                    Err(e) if lang.name == "zena" => json!({"skipped": e}),
                    _ => {
                        eprintln!("{element} in {}", lang.name);
                        bench_element(lang, element).await.unwrap_or_else(|e| json!({"error": e}))
                    }
                };
                by_lang.insert(lang.name.into(), r);
            }
            elements.insert(element.into(), by_lang.into());
        }
        results.insert("elements".into(), elements.into());
    }
    if only.is_empty() || only == "compilers" {
        let mut compilers = serde_json::Map::new();
        for (name, result) in [
            ("conduct", compiler_entry::<Conduct>().await),
            ("zena", compiler_entry::<Zena>().await),
        ] {
            compilers.insert(name.into(), result);
        }
        results.insert("compilers".into(), compilers.into());
    }
    let lines = wasi::LINES.lock().map(|l| l.clone()).unwrap_or_default();
    results.insert(
        "guest_output".into(),
        json!({"lines": lines.len(), "first": lines.iter().take(20).map(|(l, e)| json!({"line": l, "stderr": e})).collect::<Vec<_>>()}),
    );
    let results = Value::Object(results);
    let out = &cfg().out;
    std::fs::write(out, serde_json::to_string_pretty(&results).unwrap() + "\n")
        .unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    print_tables(&results);
    eprintln!("\nwrote {}", out.display());
}
