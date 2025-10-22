#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

. $SCRIPT_DIR/vars.sh

echo "PACKAGE_PATH = $PACKAGE_PATH"

cd $PACKAGE_PATH && RUST_BACKTRACE=1 cargo stylus export-abi
