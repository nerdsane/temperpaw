# Decisions

## Keep semantic judgment in the twin verifier

**Decision:** Add an opt-in semantic check to the resource verifier rather than change the agent loop.

**Came up because:** The user wants observability to drive smart twin transitions, and DSF already has correlated deployment verification callbacks.

**Options:** Extend the twin verifier; add a separate agent outcome monitor; introduce a generic kernel inference primitive.

**Chose the twin verifier because:** It directly demonstrates the accepted workflow with fewer changes while preserving resource ownership and existing transition semantics.

**Where:** `os-apps/dsf-twin/wasm/dsf_resource_common/`.

## Use isolated evidence scenarios for repeatable failure demonstrations

**Decision:** Exercise controlled startup, regression and telemetry-gap cases without injecting production failures, and label their provenance in the demo.

**Came up because:** Colleagues need a repeatable recording covering negative cases, while live production data and TypeSafe access are not yet verified.

**Options:** Cause production faults; wait for incidental real failures; use controlled scenarios with actual model and Temper execution.

**Chose controlled scenarios because:** They make the failure demonstrations reproducible and safe, while retaining real inference and real state transitions. Separate live-provider proof remains necessary for production claims.

**Where:** Demo harness and recording; source integration remains the production-shaped verifier.
