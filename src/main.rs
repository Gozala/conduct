//! The `conduct` command.
//!
//! ```text
//! conduct check FILE...            list each definition's hash and type
//! conduct element FILE... -o OUT   compile an element to a component
//! conduct wat FILE...              print an element's core module text
//! conduct check-route FILE...      `check`, for a route
//! conduct route FILE... -o OUT     compile a route to a component
//! ```
//!
//! Files are added to one codebase in order, so a later file can use the
//! definitions of an earlier one. A route's codebase starts with the
//! todo model's functions and the route prelude.

use anyhow::{Context, Result, bail};
use conduct::{element, route};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        bail!("usage: conduct check|element|wat|check-route|route FILE... [-o OUT]");
    };
    let mut files = Vec::new();
    let mut out = None;
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        if arg == "-o" {
            out = Some(rest.next().context("`-o` needs a path")?.clone());
        } else {
            files.push(arg.clone());
        }
    }
    let routing = command == "route" || command == "check-route";
    let mut cb = if routing { route::codebase().map_err(|e| anyhow::anyhow!("{e}"))? } else { element::codebase() };
    let mut added = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;
        added.extend(cb.add_source(&source).map_err(|e| anyhow::anyhow!("{file}: {e}"))?);
    }
    match command.as_str() {
        "check" | "check-route" => {
            for a in &added {
                println!("{} {} : {}", a.hash.short(), a.name, cb.show(&cb.defs[&a.hash].scheme.body));
            }
        }
        "element" => {
            let bytes = element::component(&cb).map_err(|e| anyhow::anyhow!("{e}"))?;
            let out = out.context("`element` needs `-o OUT`")?;
            std::fs::write(&out, &bytes).with_context(|| format!("writing {out}"))?;
            println!("wrote {out} ({} bytes)", bytes.len());
        }
        "route" => {
            let bytes = route::component(&cb).map_err(|e| anyhow::anyhow!("{e}"))?;
            let out = out.context("`route` needs `-o OUT`")?;
            std::fs::write(&out, &bytes).with_context(|| format!("writing {out}"))?;
            println!("wrote {out} ({} bytes)", bytes.len());
        }
        "wat" => print!("{}", element::core_wat(&cb).map_err(|e| anyhow::anyhow!("{e}"))?),
        other => bail!("unknown command `{other}`"),
    }
    Ok(())
}
