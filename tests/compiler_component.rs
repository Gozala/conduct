//! The compiler as a component (`compiler/`), run on the polyfill: it
//! compiles each example to the bytes the compiler compiles natively.
//!
//! The component is built for `wasm32-unknown-unknown` by `compiler/build.sh`.
//! Without it the test has nothing to run and says so, unless
//! `CONDUCT_COMPILER_REQUIRED` is set, as CI sets it.

use std::time::Instant;

use conduct::service::{self, World};
use wcmp::*;

fn component_bytes() -> Option<Vec<u8>> {
    let path = std::env::var("CONDUCT_COMPILER")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/compiler/target/conduct-compiler.wasm").into());
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if std::env::var_os("CONDUCT_COMPILER_REQUIRED").is_none() => {
            eprintln!("skipped: no compiler component at {path} ({e}); run compiler/build.sh");
            None
        }
        Err(e) => panic!("no compiler component at {path}: {e}"),
    }
}

struct Compiler {
    store: Store<()>,
    compile: Func,
    check: Func,
}

impl Compiler {
    async fn new(bytes: &[u8]) -> Compiler {
        let engine = Engine::with_backend(wcmp_wasm_core_wasmtime::Wasmtime::new().unwrap()).unwrap();
        let mut store = Store::new(&engine, ()).unwrap();
        let component = Component::new(&engine, bytes).await.unwrap();
        let linker: Linker<()> = Linker::new(&engine);
        let instance = linker.instantiate(&mut store, &component).await.unwrap();
        let iface = instance.exports().instance("conduct:compiler/compiler").expect("the compiler interface");
        Compiler { compile: iface.func("compile").unwrap(), check: iface.func("check").unwrap(), store }
    }

    async fn call(&mut self, check: bool, source: &str, world: &str) -> Val {
        let f = if check { &self.check } else { &self.compile };
        let args = [Val::String(source.into()), Val::String(world.into())];
        let mut results = f.call(&mut self.store, &args).await.unwrap().into_vec();
        results.remove(0)
    }
}

fn bytes(list: &Val) -> Vec<u8> {
    let Val::List(items) = list else { panic!("not a list: {list:?}") };
    items.iter().map(|v| if let Val::U8(b) = v { *b } else { panic!("not a byte: {v:?}") }).collect()
}

fn field<'a>(record: &'a Val, name: &str) -> &'a Val {
    let Val::Record(fields) = record else { panic!("not a record: {record:?}") };
    &fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}")).value
}

#[tokio::test(flavor = "current_thread")]
async fn compiles_as_it_does_natively() {
    let Some(component) = component_bytes() else { return };
    let started = Instant::now();
    let mut compiler = Compiler::new(&component).await;
    eprintln!("compiler: {} bytes, ready in {:?}", component.len(), started.elapsed());
    let examples = [
        ("todo-item", include_str!("../examples/todo-item.cdt"), World::Element),
        ("todo-input", include_str!("../examples/todo-input.cdt"), World::Element),
        ("todo-footer", include_str!("../examples/todo-footer.cdt"), World::Element),
        ("todo-app", include_str!("../examples/todo-app.cdt"), World::Element),
        ("todos-route", include_str!("../examples/todos-route.cdt"), World::Route),
        ("todo-route", include_str!("../examples/todo-route.cdt"), World::Route),
    ];
    for (name, source, world) in examples {
        let world_name = if world == World::Element { "element-component" } else { "route-component" };
        let started = Instant::now();
        let result = compiler.call(false, source, world_name).await;
        let elapsed = started.elapsed();
        let Val::Result(Ok(Some(list))) = &result else { panic!("{name}: {result:?}") };
        let compiled = bytes(list);
        eprintln!("{name}: {} bytes in {elapsed:?}", compiled.len());
        assert_eq!(compiled, service::compile(source, world).unwrap(), "{name} compiles as it does natively");
    }
    // A macro function runs inside the component as it does natively: the
    // interpreter that runs it is compiled in.
    let source = format!(
        "{}
         (macro twice-of (fn [call declared] (match call (form-list [_ x]) (ok `(* 2 ~x)) _ (error \"one form\"))))
         (type answer : (t/answer natural))
         (define doubled (twice-of 21))",
        include_str!("../examples/todo-item.cdt")
    );
    let result = compiler.call(false, &source, "element-component").await;
    let Val::Result(Ok(Some(list))) = &result else { panic!("with a macro function: {result:?}") };
    assert_eq!(bytes(list), service::compile(&source, World::Element).unwrap());
}

#[tokio::test(flavor = "current_thread")]
async fn checks_and_places_problems() {
    let Some(component) = component_bytes() else { return };
    let mut compiler = Compiler::new(&component).await;
    let result = compiler.call(true, "(define (twice x) (* 2 x))", "element-component").await;
    let Val::Result(Ok(Some(list))) = &result else { panic!("{result:?}") };
    let Val::List(shown) = list.as_ref() else { panic!("{list:?}") };
    assert_eq!(field(&shown[0], "name"), &Val::String("twice".into()));
    assert_eq!(field(&shown[0], "type"), &Val::String("((\\ (& ?a integer)) : [?a natural])".into()));
    assert_eq!(field(&shown[0], "start"), &Val::U32(9));

    let result = compiler.call(true, "(define (f x) (g x))", "element-component").await;
    let Val::Result(Err(Some(problem))) = &result else { panic!("{result:?}") };
    assert_eq!(field(problem, "start"), &Val::U32(15));
    let Val::String(message) = field(problem, "message") else { panic!() };
    assert!(message.contains("`g` is not defined"), "{message}");

    let result = compiler.call(false, "(define one 1)", "nowhere").await;
    assert!(matches!(result, Val::Result(Err(_))), "{result:?}");
}
