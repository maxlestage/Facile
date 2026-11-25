#!/bin/sh
# Builds the browser side of Facile (the Rythme interpreter and the
# components' runtime) into crates/facile/assets/facile.wasm, which the
# `facile` binary serves at /_facile/facile.wasm.
# Needs: rustup target add wasm32-unknown-unknown
set -e
cd "$(dirname "$0")/.."
cargo build -p facile-client --target wasm32-unknown-unknown --profile wasm
target="${CARGO_TARGET_DIR:-target}"
cp "$target/wasm32-unknown-unknown/wasm/facile_client.wasm" crates/facile/assets/facile.wasm
# wasm-opt (binaryen), when installed, makes it smaller still.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    crates/facile/assets/facile.wasm -o crates/facile/assets/facile.wasm
fi
ls -l crates/facile/assets/facile.wasm
