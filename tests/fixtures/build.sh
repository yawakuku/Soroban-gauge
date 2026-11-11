#!/usr/bin/env bash
# Rebuilds the committed fixture .wasm files from examples/*.
# Prefers `stellar contract build`; falls back to cargo with -Zbuild-std for
# wasm32v1-none when that rust-std component is not installed.
set -euo pipefail
cd "$(dirname "$0")/../.."
for ex in counter counter-regressed token; do
  name="${ex//-/_}"
  if command -v stellar >/dev/null 2>&1; then
    (cd "examples/$ex" && stellar contract build)
  else
    (cd "examples/$ex" && RUSTC_BOOTSTRAP=1 cargo build --release --target wasm32v1-none \
      -Zbuild-std=core,alloc -Zbuild-std-features=panic_immediate_abort 2>&1 | tail -3)
  fi
  cp "examples/$ex/target/wasm32v1-none/release/$name.wasm" "tests/fixtures/$name.wasm"
done
sha256sum tests/fixtures/*.wasm
