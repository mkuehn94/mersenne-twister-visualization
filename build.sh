#!/usr/bin/env bash
# Build the Rust core to WebAssembly and embed it for file:// use.
#   needs: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown --manifest-path crate/Cargo.toml
cp crate/target/wasm32-unknown-unknown/release/mtviz.wasm www/mtviz.wasm
printf 'window.MTV_WASM_B64="%s";\n' "$(base64 < www/mtviz.wasm | tr -d '\n')" > www/js/wasm_bytes.js
echo "built www/mtviz.wasm ($(wc -c < www/mtviz.wasm) bytes) and www/js/wasm_bytes.js"
echo "open www/index.html directly, or serve it:  python3 -m http.server -d www 8000"
