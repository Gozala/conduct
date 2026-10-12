//! The element's memory stays flat however long it runs: drive its core
//! module through the canonical ABI by hand, as a host would, and count
//! the blocks it holds.

use conduct::element;
use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

struct Core {
    store: Store<Vec<String>>,
    instance: Instance,
    memory: Memory,
    realloc: TypedFunc<(i32, i32, i32, i32), i32>,
}

impl Core {
    fn new(source: &str) -> Core {
        let mut cb = element::codebase();
        cb.add_source(source).unwrap_or_else(|e| panic!("{e}"));
        let wat = element::core_wat(&cb).unwrap();
        let engine = Engine::default();
        let module = Module::new(&engine, wat::parse_str(&wat).unwrap()).unwrap();
        let mut linker = Linker::<Vec<String>>::new(&engine);
        linker
            .func_wrap(
                "demo:element/host",
                "emit",
                |mut caller: Caller<'_, Vec<String>>, _id: i32, np: i32, nn: i32, dp: i32, dn: i32| {
                    let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let data = memory.data(&caller);
                    let name = String::from_utf8(data[np as usize..(np + nn) as usize].to_vec()).unwrap();
                    let detail = String::from_utf8(data[dp as usize..(dp + dn) as usize].to_vec()).unwrap();
                    caller.data_mut().push(format!("{name}:{detail}"));
                },
            )
            .unwrap();
        // The exports that run `update` are lifted async; with no request
        // in flight each returns at once, after `task.return`.
        for name in ["connected", "attribute-changed", "handle-event"] {
            linker.func_wrap("[export]demo:element/element", &format!("[task-return]{name}"), || {}).unwrap();
        }
        let mut store = Store::new(&engine, Vec::new());
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let realloc = instance.get_typed_func(&mut store, "cabi_realloc").unwrap();
        Core { store, instance, memory, realloc }
    }

    fn live(&mut self) -> i32 {
        let f = self.instance.get_typed_func::<(), i32>(&mut self.store, "conduct_live").unwrap();
        f.call(&mut self.store, ()).unwrap()
    }

    /// Lower a string into a buffer the module allocates, as a host does.
    fn string(&mut self, s: &str) -> (i32, i32) {
        if s.is_empty() {
            return (1, 0);
        }
        let p = self.realloc.call(&mut self.store, (0, 0, 1, s.len() as i32)).unwrap();
        self.memory.data_mut(&mut self.store)[p as usize..p as usize + s.len()].copy_from_slice(s.as_bytes());
        (p, s.len() as i32)
    }

    fn create(&mut self, attrs: &[(&str, &str)]) -> i32 {
        let list = self.realloc.call(&mut self.store, (0, 0, 4, 16 * attrs.len() as i32)).unwrap();
        for (i, (k, v)) in attrs.iter().enumerate() {
            let (kp, kn) = self.string(k);
            let (vp, vn) = self.string(v);
            let at = list as usize + 16 * i;
            let data = self.memory.data_mut(&mut self.store);
            for (j, word) in [kp, kn, vp, vn].iter().enumerate() {
                data[at + 4 * j..at + 4 * j + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        let f =
            self.instance.get_typed_func::<(i32, i32), i32>(&mut self.store, "demo:element/element#create").unwrap();
        f.call(&mut self.store, (list, attrs.len() as i32)).unwrap()
    }

    /// Render and return the node count, then let the module release the
    /// result, as `post-return` does.
    fn render(&mut self, id: i32) -> i32 {
        let f = self.instance.get_typed_func::<i32, i32>(&mut self.store, "demo:element/element#render").unwrap();
        let ret = f.call(&mut self.store, id).unwrap() as usize;
        let data = self.memory.data(&self.store);
        let n = i32::from_le_bytes(data[ret + 4..ret + 8].try_into().unwrap());
        let post =
            self.instance.get_typed_func::<i32, ()>(&mut self.store, "cabi_post_demo:element/element#render").unwrap();
        post.call(&mut self.store, ret as i32).unwrap();
        n
    }

    fn handle(&mut self, id: i32, handler: &str, kind: &str, value: Option<&str>, key: Option<&str>) {
        let (hp, hn) = self.string(handler);
        let (kp, kn) = self.string(kind);
        let (vd, vp, vn) = match value {
            Some(v) => {
                let (p, n) = self.string(v);
                (1, p, n)
            }
            None => (0, 0, 0),
        };
        let (yd, yp, yn) = match key {
            Some(k) => {
                let (p, n) = self.string(k);
                (1, p, n)
            }
            None => (0, 0, 0),
        };
        let f = self.instance.get_func(&mut self.store, "[async-lift]demo:element/element#handle-event").unwrap();
        let args: Vec<wasmtime::Val> =
            [id, hp, hn, kp, kn, vd, vp, vn, 0, 0, yd, yp, yn].iter().map(|v| wasmtime::Val::I32(*v)).collect();
        let mut code = [wasmtime::Val::I32(-1)];
        f.call(&mut self.store, &args, &mut code).unwrap();
        assert_eq!(code[0].unwrap_i32(), 0, "the task exits");
    }

    fn set_attr(&mut self, id: i32, name: &str, value: Option<&str>) {
        let (np, nn) = self.string(name);
        let (disc, vp, vn) = match value {
            Some(v) => {
                let (p, n) = self.string(v);
                (1, p, n)
            }
            None => (0, 0, 0),
        };
        let f = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(
                &mut self.store,
                "[async-lift]demo:element/element#attribute-changed",
            )
            .unwrap();
        assert_eq!(f.call(&mut self.store, (id, np, nn, disc, vp, vn)).unwrap(), 0, "the task exits");
    }

    fn disconnect(&mut self, id: i32) {
        let f = self.instance.get_typed_func::<i32, ()>(&mut self.store, "demo:element/element#disconnected").unwrap();
        f.call(&mut self.store, id).unwrap();
    }
}

const TODO_ITEM: &str = include_str!("../examples/todo-item.cdt");

#[test]
fn todo_item_memory_is_flat() {
    let mut core = Core::new(TODO_ITEM);
    let id = core.create(&[("todo-id", "7"), ("title", "Buy milk"), ("completed", "false")]);
    // One round to let the element table and the deferred list grow to
    // their working size.
    let round = |core: &mut Core| {
        assert_eq!(core.render(id), 6);
        core.handle(id, "toggle", "change", None, None);
        core.set_attr(id, "completed", Some("true"));
        core.handle(id, "edit", "dblclick", None, None);
        assert_eq!(core.render(id), 2);
        core.handle(id, "key", "keydown", Some("Buy oat milk"), Some("Enter"));
        core.set_attr(id, "completed", Some("false"));
        core.set_attr(id, "title", Some("Buy milk"));
    };
    round(&mut core);
    let baseline = core.live();
    for _ in 0..200 {
        round(&mut core);
    }
    assert_eq!(core.live(), baseline, "the element's memory grew");
    assert_eq!(core.store.data().len(), 201 * 2, "each round emits a toggle and an edit");
    assert_eq!(core.store.data()[1], "todo-edit:7\nBuy oat milk");
}

#[test]
fn disconnecting_frees_an_element() {
    let mut core = Core::new(TODO_ITEM);
    let first = core.create(&[("todo-id", "1"), ("title", "warm up")]);
    core.render(first);
    core.disconnect(first);
    let baseline = core.live();
    for i in 0..100 {
        let id = core.create(&[("todo-id", &i.to_string()), ("title", "temporary")]);
        core.handle(id, "edit", "dblclick", None, None);
        core.render(id);
        core.disconnect(id);
    }
    // The element table grows to hold the ids; nothing else stays.
    let table_growth = core.live() - baseline;
    assert!(table_growth <= 1, "{table_growth} blocks stayed after disconnecting");
}

#[test]
fn todo_input_and_footer_memory_is_flat() {
    let mut input = Core::new(include_str!("../examples/todo-input.cdt"));
    let id = input.create(&[("has-todos", "true"), ("all-completed", "false")]);
    let round = |core: &mut Core| {
        core.handle(id, "key", "keydown", Some("  Buy milk "), Some("Enter"));
        core.render(id);
        core.handle(id, "key", "keydown", Some(""), Some("a"));
        core.render(id);
        core.set_attr(id, "all-completed", Some("true"));
        core.handle(id, "toggleAll", "change", None, None);
    };
    round(&mut input);
    let baseline = input.live();
    for _ in 0..100 {
        round(&mut input);
    }
    assert_eq!(input.live(), baseline, "todo-input's memory grew");

    let mut footer = Core::new(include_str!("../examples/todo-footer.cdt"));
    let id = footer.create(&[("active", "2"), ("completed", "1"), ("filter", "all")]);
    let round = |core: &mut Core| {
        core.render(id);
        core.set_attr(id, "active", Some("1"));
        core.handle(id, "clear", "click", None, None);
        core.set_attr(id, "completed", None);
    };
    round(&mut footer);
    let baseline = footer.live();
    for _ in 0..100 {
        round(&mut footer);
    }
    assert_eq!(footer.live(), baseline, "todo-footer's memory grew");
}
