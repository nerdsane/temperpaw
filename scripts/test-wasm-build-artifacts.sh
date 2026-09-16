#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin" "$TMP/repo/os-apps"
cp "$ROOT/os-apps/wasm-build-env.sh" "$TMP/repo/os-apps/"
while IFS= read -r file; do
    relative="${file#"$ROOT/"}"
    mkdir -p "$TMP/repo/$(dirname "$relative")"
    if [[ "$file" == */build.sh ]]; then cp "$file" "$TMP/repo/$relative"; fi
done < <(find "$ROOT/os-apps" \( -name Cargo.toml -o -name build.sh \) -type f)
cat > "$TMP/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
target_dir="${CARGO_TARGET_DIR:-target}"
target=""
while (($#)); do
    case "$1" in
        --target-dir) target_dir="$2"; shift;;
        --target) target="$2"; shift;;
    esac
    shift
done
module="${PWD##*/}"
case "$target_dir" in
    /*) ;;
    *) target_dir="$PWD/$target_dir";;
esac
artifact="$target_dir/$target/release/${module//-/_}.wasm"
if [[ "${TEST_CARGO_MODE:-}" == fail ]]; then exit 17; fi
if [[ "${TEST_CARGO_MODE:-}" != missing ]]; then
    mkdir -p "$(dirname "$artifact")"
    printf 'fresh %s wasi_snapshot_preview1\n' "$module" > "$artifact"
fi
printf '%s\t%s\n' "$artifact" "$PWD/$module.wasm" >> "$TEST_ARTIFACT_LOG"
if [[ "$PWD" == */paw-fs/wasm/* ]]; then
    printf '%s\t%s\n' "$artifact" "$PWD/../$module.wasm" >> "$TEST_ARTIFACT_LOG"
fi
CARGO
cat > "$TMP/bin/wasm-tools" <<'WASM'
#!/usr/bin/env bash
cat "$2"
WASM
chmod +x "$TMP/bin/cargo" "$TMP/bin/wasm-tools"
export PATH="$TMP/bin:$PATH"
export TEST_ARTIFACT_LOG="$TMP/artifacts"
# Exercise the shell builders migrated to temperpaw_build_wasm. dsf-twin has
# its own generator and explicit Cargo output directory, outside this fixture.
builders=(
    paw-agent/wasm/build.sh
    paw-channels/wasm/build.sh
    paw-compute/wasm/build.sh
    paw-foresight/wasm/build.sh
    paw-fs/wasm/artifact_batch_apply/build.sh
    paw-fs/wasm/blob_adapter/build.sh
    paw-fs/wasm/workspace_fs/build.sh
    paw-ingest/wasm/build.sh
    paw-managed-agents/wasm/build.sh
    paw-media/wasm/build.sh
    paw-patrol/wasm/build.sh
    paw-research/wasm/build.sh
    paw-skills/wasm/build.sh
)
for index in "${!builders[@]}"; do
    builders[$index]="$TMP/repo/os-apps/${builders[$index]}"
    test -f "${builders[$index]}"
done
for mode in default absolute relative missing fail; do
    for builder in "${builders[@]}"; do
        find "$TMP/repo" -name '*.wasm' -delete
        rm -rf "$TMP/shared target"
        while IFS= read -r manifest; do
            relative="${manifest#"$ROOT/"}"
            directory="$TMP/repo/$(dirname "$relative")"
            module="${directory##*/}"
            for target in wasm32-unknown-unknown wasm32-wasip1; do
                mkdir -p "$directory/target/$target/release"
                printf 'stale wasi_snapshot_preview1\n' > "$directory/target/$target/release/${module//-/_}.wasm"
            done
        done < <(find "$ROOT/os-apps" -name Cargo.toml -type f)
        unset CARGO_TARGET_DIR TEST_CARGO_MODE
        case "$mode" in
            absolute|missing|fail) export CARGO_TARGET_DIR="$TMP/shared target";;
            relative) export CARGO_TARGET_DIR="redirected target";;
        esac
        case "$mode" in missing|fail) export TEST_CARGO_MODE="$mode";; esac
        : > "$TEST_ARTIFACT_LOG"
        if bash "$builder" > "$TMP/log" 2>&1; then result=0; else result=$?; fi
        if [[ "$mode" == missing || "$mode" == fail ]]; then
            if [[ "$result" == 0 ]]; then
                echo "FAIL: $builder succeeded with $mode output" >&2
                cat "$TMP/log" >&2
                exit 1
            fi
        else
            if [[ "$result" != 0 ]]; then cat "$TMP/log" >&2; exit 1; fi
            while IFS=$'\t' read -r artifact package; do
                if ! cmp -s "$artifact" "$package"; then
                    echo "FAIL: $builder packaged bytes differ from $artifact: $package ($mode)" >&2
                    exit 1
                fi
            done < "$TEST_ARTIFACT_LOG"
            test -s "$TEST_ARTIFACT_LOG"
            if [[ "$builder" == "$TMP/repo/os-apps/paw-foresight/wasm/build.sh" ]]; then
                python3 - "$ROOT/os-apps/paw-foresight/app.toml" "$TMP/repo/os-apps/paw-foresight/wasm" <<'PY_MANIFEST'
from pathlib import Path
import sys, tomllib

manifest = tomllib.loads(Path(sys.argv[1]).read_text())
modules = [item["name"] for item in manifest["wasm_modules"]]
assert modules, "Foresight must declare its runtime modules"
missing = [name for name in modules if not (Path(sys.argv[2]) / name / (name + ".wasm")).is_file()]
if missing:
    raise SystemExit("FAIL: Foresight builder omitted declared modules: " + ", ".join(missing))
PY_MANIFEST
            fi
        fi
    done
    echo "PASS: $mode (${#builders[@]} builders)"
done

