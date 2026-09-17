# Decisions

## D1 Keep semantic judgment in the twin verifier

**Decision:** Add an opt-in semantic check to the resource verifier rather than change the agent loop.

**Came up because:** The user wants observability to drive smart twin transitions, and DSF already has correlated deployment verification callbacks.

**Options:** Extend the twin verifier; add a separate agent outcome monitor; introduce a generic kernel inference primitive.

**Chose the twin verifier because:** It directly demonstrates the accepted workflow with fewer changes while preserving resource ownership and existing transition semantics.

**Where:** `os-apps/dsf-twin/wasm/dsf_resource_common/`.

## D2 Use isolated evidence scenarios for repeatable failure demonstrations

**Decision:** Exercise controlled startup, regression and telemetry-gap cases without injecting production failures, and label their provenance in the demo.

**Came up because:** Colleagues need a repeatable recording covering negative cases, while live production data and TypeSafe access are not yet verified.

**Options:** Cause production faults; wait for incidental real failures; use controlled scenarios with actual model and Temper execution.

**Chose controlled scenarios because:** They make the failure demonstrations reproducible and safe, while retaining real inference and real state transitions. Separate live-provider proof remains necessary for production claims.

**Where:** Demo harness and recording; source integration remains the production-shaped verifier.

## D3 Controlled demo runs the packaged verifier

**Decision:** Use the existing Temper WASM engine and production actor evaluator with a controlled HTTP host for the local walkthrough.

**Came up because:** The connected tenant denied DSF reads without creating an elicitable approval, while the user wanted independent work to continue.

**Options:** Wait for production access, build a browser-only simulation, or run the real verifier and actor with explicitly labeled provider fixtures.

**Chose the real verifier with controlled provider fixtures because:** It demonstrates actual Jev calls and actual IOA transitions without inventing successful deployment or observability access. It does not prove live provider integration or production Cedar permissions.

**Where:** `crates/temperpaw/examples/jev_twin.rs`, PR #532.

## D4 Conservative semantic evidence window

**Decision:** Keep the existing exact provider and probe gates, then evaluate bounded recent telemetry with an opt-in Jev configuration.

**Came up because:** Semantic interpretation must not allow model output to substitute for resource identity, requested revision or fresh observations.

**Options:** Let Jev replace the verifier, or add a semantic gate after existing checks.

**Chose an additional gate because:** Existing invariants remain authoritative. Unknown, malformed, low-confidence, empty and potentially truncated results remain pending. This favors false waits until DSF-specific calibration and broader live telemetry coverage exist.

**Where:** `os-apps/dsf-twin/wasm/dsf_resource_common/src/semantic.rs` and `verification.rs`.
