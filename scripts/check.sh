#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

. $SCRIPT_DIR/vars.sh

echo "PACKAGE_PATH = $PACKAGE_PATH"
echo "WASM_FILE_PATH = $WASM_FILE_PATH"

# First we make stylus build
# Note: it will actually fail at the end, because it doesn't understand cargo workspaces
cd $PACKAGE_PATH && cargo stylus check || true 

# Then we run actual check
cd $PACKAGE_PATH && cargo stylus check --wasm-file "$WORKSPACE_ROOT/$WASM_FILE_PATH"
