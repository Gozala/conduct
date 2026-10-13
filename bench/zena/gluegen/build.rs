//! Points the demo's own glue module and sources table at the polyfill
//! checkout in `POLYFILL_DIR`. A `#[path]` attribute cannot read the
//! environment, so this writes the two module declarations with the
//! checkout's path in them, and main.rs includes them.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=POLYFILL_DIR");
    let Some(dir) = std::env::var_os("POLYFILL_DIR") else {
        panic!("set POLYFILL_DIR to a checkout of dialog-db/wasm-component-model-polyfill");
    };
    let demo = PathBuf::from(dir).join("rust/wcmp-demo/src");
    let demo = demo.canonicalize().unwrap_or_else(|e| panic!("{}: {e}", demo.display()));
    let glue = demo.join("glue/mod.rs");
    let sources = demo.join("sources.rs");
    for file in [&glue, &sources] {
        assert!(file.is_file(), "{} is not in the polyfill checkout", file.display());
    }
    println!("cargo:rerun-if-changed={}", demo.display());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("demo.rs");
    let text = format!(
        "#[path = {:?}]\nmod glue;\n#[path = {:?}]\nmod sources;\n",
        glue.display().to_string(),
        sources.display().to_string(),
    );
    std::fs::write(&out, text).unwrap();
}
