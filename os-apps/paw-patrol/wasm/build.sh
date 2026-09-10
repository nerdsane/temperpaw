#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../../wasm-build-env.sh"

copy_artifact() {
    local module="$1"
    local target="wasm32-unknown-unknown"
    local source_file="$SCRIPT_DIR/$module/target/$target/release/${module}.wasm"
    if [ ! -f "$source_file" ]; then
        source_file="$SCRIPT_DIR/$module/target/$target/release/$(echo "$module" | tr '_' '-').wasm"
    fi
    if [ -f "$source_file" ]; then
        cp "$source_file" "$SCRIPT_DIR/$module/$module.wasm"
    fi
}

for module in patrol_request_router signal_router worker_run_lifecycle work_cycle_lifecycle finding_lifecycle review_gate_lifecycle repo_sweep_lifecycle daily_brief_lifecycle patrol_schedule_lifecycle patrol_run_lifecycle release_run_lifecycle record_ingest chain_file_ready chain_github_ready chain_review_ready chain_proof_ready chain_merge_ready effort_resource_delivery_validate effort_resource_delivery_merge effort_resource_delivery_verify temper_deploy_lifecycle contract_probe; do
    echo "Building $module..."
    if [ "$module" = "patrol_request_router" ]; then
        (cd "$SCRIPT_DIR/$module" && cargo build --bin patrol_request_router --target wasm32-unknown-unknown --release)
    else
        (cd "$SCRIPT_DIR/$module" && cargo build --target wasm32-unknown-unknown --release)
    fi
    copy_artifact "$module"
    echo "  -> $module built successfully"
done

echo ""
echo "All paw-patrol WASM modules built."
