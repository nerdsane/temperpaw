#!/usr/bin/env bash
set -euo pipefail
module_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$module_dir/../../../wasm-build-env.sh"
cargo build --manifest-path "$module_dir/Cargo.toml" --target wasm32-unknown-unknown --release "$@"
cp "$module_dir/target/wasm32-unknown-unknown/release/dsf_operation_validate.wasm" "$module_dir/dsf_operation_validate.wasm"
