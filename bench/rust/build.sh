#!/usr/bin/env bash
# Builds each element as a component of `demo:element/element-component`:
#
#   out/{todo-item,todo-input,todo-footer,todo-app}.wasm        opt-level "s" (profile `release`)
#   out/opt-z/{todo-item,todo-input,todo-footer,todo-app}.wasm  opt-level "z" (profile `release-z`)
#
# and prints each one's size, raw and after `gzip -9`.
#
# Each element builds in a cargo invocation of its own, so that the
# features one element turns on (todo-app's `async-spawn`) do not reach
# the others. The toolchain has no prebuilt `wasm32-unknown-unknown`
# standard library, so it builds std from `rust-src` when it must.
set -euo pipefail
cd "$(dirname "$0")"

target=wasm32-unknown-unknown
elements=(todo-item todo-input todo-footer todo-app)
profiles=("${@:-release release-z}")

build() {
  if [ -d "$(rustc --print sysroot)/lib/rustlib/$target/lib" ]; then
    cargo build --target "$target" "$@"
  else
    RUSTC_BOOTSTRAP=1 cargo build --target "$target" -Zbuild-std=std,panic_abort "$@"
  fi
}

for profile in ${profiles[@]}; do
  case "$profile" in
    release) dir=out ;;
    release-z) dir=out/opt-z ;;
    *) echo "unknown profile $profile" >&2; exit 1 ;;
  esac
  mkdir -p "$dir"
  for element in "${elements[@]}"; do
    build --profile "$profile" -p "$element"
    core="target/$target/$profile/${element//-/_}.wasm"
    # The async component model is not yet a phase-4 proposal, so the
    # encode step skips validation and the component is validated with it on.
    wasm-tools component new --skip-validation "$core" -o "$dir/$element.wasm"
    wasm-tools validate -f cm-async "$dir/$element.wasm"
  done
done

for profile in ${profiles[@]}; do
  case "$profile" in
    release) dir=out; level=s ;;
    release-z) dir=out/opt-z; level=z ;;
  esac
  for element in "${elements[@]}"; do
    file="$dir/$element.wasm"
    printf '%-12s opt-level=%s %8d bytes %8d gzip -9\n' "$element" "$level" \
      "$(wc -c <"$file")" "$(gzip -9 -c "$file" | wc -c)"
  done
done
