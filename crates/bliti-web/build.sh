#!/bin/sh
# Build the wasm module and its bindings into the application's source tree, where Vite picks them up
# and bundles them like any other module. Everything protocol-shaped is in here; the application
# around it only drives Web Bluetooth, the camera, and the interface.
set -eu
cd "$(dirname "$0")"
cargo build -p bliti-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir app/src/wasm \
	../../target/wasm32-unknown-unknown/release/bliti_web.wasm
