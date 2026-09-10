# Decisions

## Explicit output directory

**Decision:** Pass an explicit target directory to Cargo and use it for packaging.

**Came up because:** Cargo honors CARGO_TARGET_DIR while the builders guess a module-local path.

**Options:** Parse Cargo JSON artifact messages using a new parser dependency; resolve Cargo metadata; or pass one explicit output directory.

**Chose explicit output over parsing because:** It fixes the mismatch without adding tools to Docker and agent build environments. CARGO_TARGET_DIR remains supported, including relative paths. When unset, these builders deliberately select the module-local target directory instead of inheriting a Cargo config target-dir.

**Where:** os-apps/wasm-build-env.sh.

## Isolated worktree on the source computer

**Decision:** Use /home/tl-user/work/arn490-wasm-artifacts on arni-big.

**Came up because:** The governed Copy returned uncertain HTTP400 and exact-name reconciliation returned HTTP404; the primary checkout is dirty.

**Options:** Wait on unrelated Copy recovery, modify the primary checkout, or use a separate worktree.

**Chose a separate worktree because:** Governed Exec works and it isolates source edits without taking over Copy cleanup or overwriting ongoing work. The unresolved copy remains recorded as Computer 01a08c11-1072-7c23-9183-5e8a5c254a03.

**Where:** Branch codex/arn490-wasm-artifacts.
