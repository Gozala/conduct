//! Macros that are functions: `(macro name f)`, where `f` takes the form
//! of a call and answers the form it expands to, run while compiling.
//!
//! The function is compiled as any definition is, to WebAssembly, and run
//! by an interpreter, `wasmi`, so it runs wherever the compiler does, in a
//! browser too, with the same runtime and the same meaning as compiled
//! code. What it is given and answers are values of the type `form`, the
//! reader's forms as conduct values; see `core.cdt`.

use crate::Codebase;
use crate::codegen::{self, Builder, KIND_LIST, KIND_RECORD, KIND_STR, Value};
use crate::reader::{Sexp, Span, bare};
use crate::term::Hash;
use wasmi::{Config, Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

/// What a macro function may take, at most: past it, it is taken to run
/// forever.
const FUEL: u64 = 1 << 30;

/// A macro function compiled, ready to run.
pub struct Compiled {
    engine: Engine,
    module: Module,
}

impl std::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Compiled")
    }
}

/// Compile the macro function `hash`: a function of a form and, labelled
/// `declared:`, the declarations the call's names have.
pub fn compile(cb: &Codebase, hash: Hash) -> Result<Compiled, String> {
    let mut b = Builder::new(cb);
    let call = b.call(hash, &["(local.get $a0)".into(), "(local.get $a1)".into()]).map_err(|e| e.to_string())?;
    b.add_func(format!("  (func (export \"transform\") (param $a0 i64) (param $a1 i64) (result i64) {call})"));
    b.add_func(
        "  (func (export \"conduct_obj\") (param i32 i32 i32) (result i32) \
         (call $obj (local.get 0) (local.get 1) (local.get 2)))",
    );
    let wat = b.finish().map_err(|e| e.to_string())?;
    let wasm = wat::parse_str(&wat).map_err(|e| format!("internal: {e}"))?;
    let mut config = Config::default();
    config.wasm_tail_call(true).consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, &wasm).map_err(|e| format!("internal: {e}"))?;
    if module.imports().len() > 0 {
        return Err("a macro cannot use the host: it runs while compiling, where there is none".into());
    }
    Ok(Compiled { engine, module })
}

/// Run a compiled macro function on the form of a call, with the
/// declarations of the names it holds: the form it expands to, or why
/// it did not.
pub fn run(compiled: &Compiled, call: &Sexp, declared: &[(String, Sexp)]) -> Result<Sexp, String> {
    let mut store = Store::new(&compiled.engine, ());
    store.set_fuel(FUEL).map_err(|e| e.to_string())?;
    let instance = Linker::<()>::new(&compiled.engine)
        .instantiate_and_start(&mut store, &compiled.module)
        .map_err(|e| format!("internal: {e}"))?;
    let transform: TypedFunc<(i64, i64), i64> =
        instance.get_typed_func(&store, "transform").map_err(|e| format!("internal: {e}"))?;
    let (form, declared, memory) = {
        let mut heap = Heap::new(&mut store, &instance)?;
        let form = heap.form(call)?;
        let mut entries = Vec::new();
        for (name, declaration) in declared {
            let name = heap.text(KIND_STR, name)?;
            let declaration = heap.form(declaration)?;
            entries.push(
                heap.record(vec![("conduct.form/declared-name", name), ("conduct.form/declared-type", declaration)])?,
            );
        }
        (form, heap.list(entries)?, heap.memory)
    };
    let out = match transform.call(&mut store, (form, declared)) {
        Ok(out) => out,
        Err(e) if e.as_trap_code() == Some(wasmi::TrapCode::OutOfFuel) => {
            return Err("it ran too long: does it always call itself again?".into());
        }
        Err(e) => return Err(format!("it failed: {e}")),
    };
    let value = codegen::decode(memory.data(&store), out);
    let span = call.span();
    match value {
        Value::Record(fields) if fields.len() == 1 => {
            let (id, inner) = &fields[0];
            if *id == codegen::field_id("conduct.result/ok") {
                to_sexp(inner, span)
            } else {
                match inner {
                    Value::Str(why) => Err(why.clone()),
                    other => Err(format!("{other:?}")),
                }
            }
        }
        other => Err(format!("internal: a macro function answered {other:?}")),
    }
}

/// The form a value of the type `form` is.
fn to_sexp(v: &Value, span: Span) -> Result<Sexp, String> {
    let Value::Record(fields) = v else { return Err(format!("internal: not a form: {v:?}")) };
    let [(id, inner)] = &fields[..] else { return Err(format!("internal: not a form: {v:?}")) };
    let is = |name: &str| *id == codegen::field_id(&format!("conduct.form/{name}"));
    let text = |v: &Value| match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(format!("internal: not text: {other:?}")),
    };
    let items = |v: &Value| match v {
        Value::List(items) => items.iter().map(|i| to_sexp(i, span)).collect::<Result<Vec<_>, _>>(),
        other => Err(format!("internal: not a list: {other:?}")),
    };
    Ok(if is("symbol") {
        Sexp::Symbol(text(inner)?, span)
    } else if is("keyword") {
        Sexp::Keyword(text(inner)?, span)
    } else if is("label") {
        Sexp::Label(text(inner)?, span)
    } else if is("text") {
        Sexp::Str(text(inner)?, span)
    } else if is("integer") {
        match inner {
            Value::Int(n) => Sexp::Int(*n, span),
            other => return Err(format!("internal: not an integer: {other:?}")),
        }
    } else if is("list") {
        Sexp::List(items(inner)?, span)
    } else if is("vector") {
        Sexp::Vector(items(inner)?, span)
    } else if is("map") {
        Sexp::Map(items(inner)?, span)
    } else {
        return Err(format!("internal: not a form: {v:?}"));
    })
}

/// Values written into a module's memory, as compiled code makes them.
struct Heap<'s> {
    store: &'s mut Store<()>,
    memory: Memory,
    obj: TypedFunc<(i32, i32, i32), i32>,
}

impl<'s> Heap<'s> {
    fn new(store: &'s mut Store<()>, instance: &Instance) -> Result<Heap<'s>, String> {
        let memory = instance.get_memory(&*store, "memory").ok_or("internal: no memory")?;
        let obj = instance.get_typed_func(&*store, "conduct_obj").map_err(|e| format!("internal: {e}"))?;
        Ok(Heap { store, memory, obj })
    }

    /// A block of `bytes` bytes for an object of `kind` and length `len`.
    fn alloc(&mut self, kind: u32, len: usize, bytes: usize) -> Result<usize, String> {
        let p = self.obj.call(&mut *self.store, (kind as i32, len as i32, bytes as i32)).map_err(|e| e.to_string())?;
        Ok(p as u32 as usize)
    }

    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), String> {
        self.memory.write(&mut *self.store, at, bytes).map_err(|e| e.to_string())
    }

    fn text(&mut self, kind: u32, s: &str) -> Result<i64, String> {
        let p = self.alloc(kind, s.len(), s.len())?;
        self.write(p + 8, s.as_bytes())?;
        Ok(p as i64)
    }

    fn list(&mut self, items: Vec<i64>) -> Result<i64, String> {
        let p = self.alloc(KIND_LIST, items.len(), 8 * items.len())?;
        let bytes: Vec<u8> = items.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.write(p + 8, &bytes)?;
        Ok(p as i64)
    }

    /// A record of the attributes named, by their whole names.
    fn record(&mut self, fields: Vec<(&str, i64)>) -> Result<i64, String> {
        let mut fields: Vec<(i64, i64)> = fields.into_iter().map(|(name, v)| (codegen::field_id(name), v)).collect();
        fields.sort();
        let p = self.alloc(KIND_RECORD, fields.len(), 16 * fields.len())?;
        let bytes: Vec<u8> = fields.iter().flat_map(|(id, v)| [id.to_le_bytes(), v.to_le_bytes()]).flatten().collect();
        self.write(p + 8, &bytes)?;
        Ok(p as i64)
    }

    fn variant(&mut self, name: &str, v: i64) -> Result<i64, String> {
        self.record(vec![(&format!("conduct.form/{name}"), v)])
    }

    /// A form as a value of the type `form`. A symbol is given as
    /// written, without the marks an expansion leaves on it; `true` and
    /// `false` are symbols, as the reader reads them.
    fn form(&mut self, s: &Sexp) -> Result<i64, String> {
        match s {
            Sexp::Symbol(n, _) => {
                let t = self.text(KIND_STR, bare(n))?;
                self.variant("symbol", t)
            }
            Sexp::Keyword(k, _) => {
                let t = self.text(KIND_STR, k)?;
                self.variant("keyword", t)
            }
            Sexp::Label(l, _) => {
                let t = self.text(KIND_STR, l)?;
                self.variant("label", t)
            }
            Sexp::Str(t, _) => {
                let t = self.text(KIND_STR, t)?;
                self.variant("text", t)
            }
            Sexp::Int(n, _) => self.variant("integer", codegen::int(*n)),
            Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
                let values = items.iter().map(|i| self.form(i)).collect::<Result<Vec<_>, _>>()?;
                let list = self.list(values)?;
                let kind = match s {
                    Sexp::List(..) => "list",
                    Sexp::Vector(..) => "vector",
                    _ => "map",
                };
                self.variant(kind, list)
            }
        }
    }
}
