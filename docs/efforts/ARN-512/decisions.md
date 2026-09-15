# ARN-512 decisions (temperpaw side)

## D1 — pin, do not re-vendor

**Decision:** the only change is the kernel rev (both crates that pin it) and
the matching lockfile entries.

**Came up because:** the kernel fix has to reach the openpaw image.

**Options:** wait for the daily pin-bump bot (its PR carries no design chain
and fails the gates); bump by hand under this effort (chosen).

**Where:** `crates/temperpaw/Cargo.toml`, `crates/paw-codex-worker/Cargo.toml`, `Cargo.lock`.
