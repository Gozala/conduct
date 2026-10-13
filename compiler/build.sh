#!/usr/bin/env bash
# Builds the conduct compiler as a component at
# compiler/target/conduct-compiler.wasm.
#
# The crate builds for `wasm32-unknown-unknown` with `wit-bindgen`, and
# `wasm-tools component new` makes the component. A toolchain without
# that target's standard library builds it from the toolchain's sources
# (`rust-src`) instead.
set -euo pipefail
cd "$(dirname "$0")"

target=wasm32-unknown-unknown
if [ -d "$(rustc --print sysroot)/lib/rustlib/$target/lib" ]; then
  cargo build --release --target "$target"
else
  RUSTC_BOOTSTRAP=1 cargo build --release --target "$target" -Zbuild-std=std,panic_abort
fi

# The async component model is not yet a phase-4 proposal, so the
# encode step skips validation and the component is validated with it on.
wasm-tools component new --skip-validation \
  "target/$target/release/conduct_compiler.wasm" -o target/conduct-compiler.wasm
wasm-tools validate -f cm-async target/conduct-compiler.wasm
echo "target/conduct-compiler.wasm: $(wc -c <target/conduct-compiler.wasm) bytes"
