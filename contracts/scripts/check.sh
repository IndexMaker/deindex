#!/bin/bash
set -e

PACKAGE_NAME=$(basename "$PWD")
WASM_FILE_PATH="../../target/wasm32-unknown-unknown/release/$PACKAGE_NAME.wasm"

# First we make stylus build
# Note: it will actually fail at the end, because it doesn't understand cargo workspaces
cargo stylus check || true 

# Then we run actual check
cargo stylus check --wasm-file "$WASM_FILE_PATH"
