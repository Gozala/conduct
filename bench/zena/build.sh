#!/usr/bin/env bash
# Builds the Zena side of the benchmark: the polyfill demo's six Zena
# components and the Zena compiler component.
#
#   ./build.sh <zena checkout> <polyfill checkout> <out dir>
#
# The checkouts are github.com/zena-lang/zena at
# 1bbe472f4c34d3faf12c74f9876afcfe541c4ac3 and
# github.com/dialog-db/wasm-component-model-polyfill at
# eedcd22766b35cc34b3b81b4c753ab4f08a7c09a. It writes:
#
#   <out>/components/{todo-item,todo-input,todo-footer,todo-app,todos-route,todo-route}.wasm
#   <out>/components/zena-compiler.wasm     (ZENA_DIR for the harness)
#   <out>/sources/jobs/<name>/              (ZENA_SOURCES for the harness)
#   <out>/sources/stdlib/
#
# Each job directory holds what the demo hands the compiler component for
# one compile: the glue the demo writes around the author's source, the
# source, the world's WIT, the authoring library, and the package
# manifest. gluegen writes the glue by compiling the demo's own glue
# module, so it is the demo's glue, byte for byte. Each component is
# compiled from its job by Zena's command line compiler, as
# `zena build --target component` would.
#
# The steps that bootstrap Zena (its `zena-run` host and its self-hosted
# compiler, `packages/zena-compiler/zena/out/cli.wasm`) run only when
# their outputs are missing. Building the compiler component takes a few
# minutes and about 6 GB of memory.
set -euo pipefail

if [ $# -ne 3 ]; then
  echo "usage: $0 <zena checkout> <polyfill checkout> <out dir>" >&2
  exit 1
fi
here=$(cd "$(dirname "$0")" && pwd)
zena=$(cd "$1" && pwd)
polyfill=$(cd "$2" && pwd)
mkdir -p "$3"
out=$(cd "$3" && pwd)

zena_rev=1bbe472f4c34d3faf12c74f9876afcfe541c4ac3
polyfill_rev=eedcd22766b35cc34b3b81b4c753ab4f08a7c09a
for pair in "$zena:$zena_rev" "$polyfill:$polyfill_rev"; do
  dir=${pair%:*}
  rev=${pair##*:}
  head=$(git -C "$dir" rev-parse HEAD 2>/dev/null || echo unknown)
  if [ "$head" != "$rev" ]; then
    echo "warning: $dir is at $head, not $rev" >&2
  fi
done

run="$zena/target/release/zena-run"
cli="$zena/packages/zena-compiler/zena/out/cli.wasm"

# 1. Zena's host and compiler, as its DEVELOPMENT.md bootstraps them:
#    cargo builds zena-run, and the checked-in bootstrap compiler builds
#    the working compiler (the command of its `build:cli` script).
if [ ! -x "$run" ]; then
  (cd "$zena" && cargo build --release -p zena-run)
fi
if [ ! -f "$cli" ]; then
  mkdir -p "$(dirname "$cli")"
  (cd "$zena" && ZENA_GC_RESERVE_MB=1536 "$run" --dir .::. --dir packages/stdlib/zena::/stdlib \
    packages/zena-compiler/bootstrap/cli.wasm packages/zena-compiler/zena/cli/main.zena \
    -o packages/zena-compiler/zena/out/cli.wasm)
fi

# 2. The jobs: the demo's glue and sources, its authoring library, and
#    its package manifest.
jobs="$out/sources/jobs"
rm -rf "$jobs"
POLYFILL_DIR="$polyfill" cargo run --quiet --release \
  --manifest-path "$here/gluegen/Cargo.toml" --target-dir "$out/target" -- "$jobs"
for job in "$jobs"/*; do
  cp -r "$polyfill/rust/wcmp-demo/zena/authoring" "$job/"
  cp "$polyfill/rust/wcmp-demo/zena/zena-packages.json" "$job/"
done
rm -rf "$out/sources/stdlib"
cp -r "$zena/packages/stdlib/zena" "$out/sources/stdlib"

# 3. The components. The compiler writes inside its preopened directory,
#    so each output is moved out of its job afterwards.
mkdir -p "$out/components"
for job in "$jobs"/*; do
  name=$(basename "$job")
  entry=$(sed -n 1p "$job/job")
  world=$(sed -n 2p "$job/job")
  echo "compiling $name"
  (cd "$job" && "$run" --dir "$job::." --dir "$zena/packages/stdlib/zena::/stdlib" "$cli" "$entry" \
    --target component --wit world.wit --world "$world" -o "$name.wasm")
  mv "$job/$name.wasm" "$out/components/$name.wasm"
done

# 4. The compiler component: the polyfill's compiler entry module
#    (zena/compiler) over Zena's compiler, language service, formatter
#    and WIT parser, laid out as its package manifest names them.
work="$out/compiler-src"
rm -rf "$work"
mkdir -p "$work/packages" "$work/wcmp"
for package in language-service wit-parser zena-compiler zena-formatter stdlib; do
  mkdir -p "$work/packages/$package"
  cp -r "$zena/packages/$package/zena" "$work/packages/$package/zena"
done
rm -rf "$work/packages/zena-compiler/zena/out"
cp -r "$polyfill/zena/compiler/compiler.zena" "$polyfill/zena/compiler/wit" "$work/wcmp/"
printf '%s' '{"packages":{"language-service":"./packages/language-service/zena/lib","wit-parser":"./packages/wit-parser/zena","zena-compiler":"./packages/zena-compiler/zena/lib","zena-formatter":"./packages/zena-formatter/zena/lib"}}' \
  >"$work/zena-packages.json"
echo "compiling zena-compiler"
(cd "$work" && "$run" --dir "$work::." --dir "$work/packages/stdlib/zena::/stdlib" "$cli" wcmp/compiler.zena \
  --target component --wit wcmp/wit --world compiler -o compiler.wasm)
mv "$work/compiler.wasm" "$out/components/zena-compiler.wasm"

for file in "$out"/components/*.wasm; do
  echo "$file: $(wc -c <"$file") bytes"
done
echo "ZENA_DIR=$out/components ZENA_SOURCES=$out/sources"
