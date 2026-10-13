//! Writes, verbatim, the glue and files the polyfill demo's `Compile`
//! builds for each of its elements and routes, by compiling the demo's
//! own glue module and sources table (from `POLYFILL_DIR`, see build.rs).
//!
//! Usage: `gluegen <out dir>`. Each compile gets a directory under it,
//! named as its component is: the glue, the author's source, `world.wit`,
//! and `job`, whose two lines are the glue's path and the world.

#![allow(dead_code)]

/// The demo's compile request, which the glue module names.
mod compiler {
    pub struct CompileRequest<'a> {
        pub entry_path: &'a str,
        pub source: &'a str,
        pub files: &'a [(String, String)],
        pub wit: &'a str,
        pub world: &'a str,
    }
}

include!(concat!(env!("OUT_DIR"), "/demo.rs"));

fn write(dir: &std::path::Path, c: &glue::Compile) {
    std::fs::create_dir_all(dir).unwrap();
    let r = c.request();
    std::fs::write(dir.join(r.entry_path), r.source).unwrap();
    for (p, s) in r.files {
        std::fs::write(dir.join(p), s).unwrap();
    }
    std::fs::write(dir.join("world.wit"), r.wit).unwrap();
    std::fs::write(dir.join("job"), format!("{}\n{}\n", r.entry_path, r.world)).unwrap();
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: gluegen <out dir>"));
    for (tag, src) in sources::ELEMENTS {
        write(&out.join(tag), &glue::Compile::element(tag, src).unwrap());
    }
    for (pat, src) in sources::ROUTES {
        let name = if pat == "/api/todos" { "todos-route" } else { "todo-route" };
        write(&out.join(name), &glue::Compile::route(pat, src));
    }
}
