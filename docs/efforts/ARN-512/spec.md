# ARN-512 spec (temperpaw side) — pin the kernel that keeps schema annotations

The kernel fix is nerdsane/temper#470 (`90548bd1`). temperpaw pins the kernel
by git rev in `crates/temperpaw/Cargo.toml` and `crates/paw-codex-worker/Cargo.toml`;
the openpaw image is built from this repo, so the fix reaches production only
through this pin.

Contract: the pinned kernel emits schema-level annotations in `$metadata`; after
the image ships and the DSF schema is reloaded, production `$metadata` carries
`<Annotation Term="Temper.Twin" String="Deep Sci-Fi"/>` under `Dsf.Twin` and
Foundry's Twins page lists the twin. No temperpaw code changes.
