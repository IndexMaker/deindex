#!/bin/bash
set -e

die() {
    echo "ERROR: $1" >&2
    exit 1
}

PACKAGE_NAME=${1:-$(basename "$PWD")}

CARGO_METADATA_COMMAND="cargo metadata --no-deps --format-version 1"
if ! command -v jq &> /dev/null
then
    die "The 'jq' utility (JSON processor) is required to parse cargo metadata. Please install it."
fi

WORKSPACE_ROOT=$($CARGO_METADATA_COMMAND | jq -r '.workspace_root')
if [ -z "$WORKSPACE_ROOT" ]; then
    die "Could not determine the workspace root. Are you inside a Cargo project?"
else
    echo "WORKSPACE_ROOT = $WORKSPACE_ROOT"
fi

PACKAGE_PATH="$WORKSPACE_ROOT/contracts/$PACKAGE_NAME"
WASM_FILE_PATH="target/wasm32-unknown-unknown/release/$PACKAGE_NAME.wasm"
