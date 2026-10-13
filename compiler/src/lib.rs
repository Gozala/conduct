//! The conduct compiler as a component.
//!
//! It builds for `wasm32-unknown-unknown` and imports nothing, as the
//! polyfill's own Rust fixtures do: its exports are WASI 0.3 `async`
//! functions, and nothing in it calls libc, so a `wasip2` or `wasip3`
//! target would only add imports a host must supply.

wit_bindgen::generate!({
    path: "wit",
    world: "conduct-compiler",
});

use conduct::service::{self, Problem, World};
use exports::conduct::compiler::compiler::{Guest, Problem as Failure, Shown};

struct Compiler;

impl Guest for Compiler {
    async fn compile(source: String, world: String) -> Result<Vec<u8>, Failure> {
        service::compile(&source, world_of(&world)?).map_err(failure)
    }

    async fn check(source: String, world: String) -> Result<Vec<Shown>, Failure> {
        let shown = service::check(&source, world_of(&world)?).map_err(failure)?;
        Ok(shown
            .into_iter()
            .map(|s| Shown { name: s.name, start: s.span.start as u32, end: s.span.end as u32, type_: s.ty })
            .collect())
    }
}

fn world_of(name: &str) -> Result<World, Failure> {
    World::named(name).ok_or_else(|| Failure { start: 0, end: 0, message: format!("no world `{name}`") })
}

fn failure(p: Problem) -> Failure {
    Failure { start: p.span.start as u32, end: p.span.end as u32, message: p.message }
}

export!(Compiler);
