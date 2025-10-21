#!/bin/bash
set -e

RUST_BACKTRACE=1 cargo test --features test-debug -- --show-output
