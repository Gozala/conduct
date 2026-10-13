#!/usr/bin/env bash
# Builds the demo's site into demo/dist: the page and the service worker,
# each a wasm-bindgen module, the compiler component they fetch, and the
# page's static files. Serve demo/dist over HTTP from any path.
#
# Needs `wasm-bindgen` 0.2.126, the version the crate pins, and what
# compiler/build.sh needs.
set -euo pipefail
cd "$(dirname "$0")"

target=wasm32-unknown-unknown
../compiler/build.sh
if [ -d "$(rustc --print sysroot)/lib/rustlib/$target/lib" ]; then
  cargo build --release --target "$target" --bins
else
  RUSTC_BOOTSTRAP=1 cargo build --release --target "$target" --bins -Zbuild-std=std,panic_abort
fi

rm -rf dist
mkdir -p dist
out="target/$target/release"
# The page is an ES module; the service worker loads its module with
# `importScripts`, so it is wasm-bindgen's classic-script form.
wasm-bindgen --target web --no-typescript --out-dir dist "$out/conduct-demo-page.wasm"
wasm-bindgen --target no-modules --no-typescript --out-dir dist "$out/conduct-demo-worker.wasm"
cp web/index.html web/style.css web/elements.js web/sw.js dist/
cp ../compiler/target/conduct-compiler.wasm dist/
ls -l dist
