#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TYPESAFE_API_KEY_FILE:?Set TYPESAFE_API_KEY_FILE to a private key file outside this checkout}"
source os-apps/wasm-build-env.sh
cargo build --manifest-path os-apps/dsf-twin/wasm/dsf_railway_deploy_verify/Cargo.toml --target wasm32-wasip1 --release
unset RUSTFLAGS
exec cargo run -p temperpaw --example jev_twin -- "$@"
