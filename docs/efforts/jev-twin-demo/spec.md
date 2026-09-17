# Semantic deployment verification

The existing DSF resource operation remains the state machine. Add an opt-in semantic verification configuration to its verifier. Existing configurations behave unchanged. Jev evaluates narrowly scoped questions over bounded, revision-bound flow and telemetry evidence. Its response is validated, recorded and combined with deterministic checks. It cannot authorize a provider write, change a source revision, or override unavailable/stale evidence.

## Contract

- Verification begins only in the operation's Verifying state, with a known provider result.
- Correct target, operation sequence, requested revision and evidence provenance are checked by the existing runtime boundary.
- Semantic outcomes are pass, wait and fail. The result maps to the existing VerificationSucceeded, VerificationPending and VerificationFailed callbacks.
- Missing evidence, model unavailability and uncertain results cannot produce success.
- A conclusive observed contradiction can fail verification even if the provider is healthy.
- A result from an older operation must not advance a newer one.
- Every live demo evaluation uses the actual Jev endpoint; fixtures describe a controlled environment and are visibly labeled. No canned model answer may be shown as live.
- The recording must show all three scenarios, the real judgments, evidence and resulting state transitions.

## State model

The model is the existing generated DsfRailwayServiceInstance IOA contract: DeployObserved -> DeployVerifying -> {Active with deploy_verified, DeployObserved without verification, DeployFailed without verification}. The new judgment only selects the correlated callback. Tests exercise each result and malformed/missing evidence, and reject stale operation results through the existing contract.

## Delivery

One opt-in verifier, a narrow demonstration surface, reproducible launch instructions and a Screen Studio recording. Validate the API result, actual WASM execution, real state transitions and browser output before recording. Production provider behavior must not be claimed from controlled fixtures.
