#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

(for f in `ls contracts`;
    do echo "contract name: $f";
    $SCRIPT_DIR/check.sh $f;
    done) 2>&1 | grep "contract size\|contract name"
