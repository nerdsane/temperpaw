# Jev shadow evaluation in Foresight

The existing generator, repairer, critic, deterministic costing and immutable forecasts remain the authority. On each StartChallenge, a separate declared integration evaluates a frozen repair and its required events using Jev 1.13.0. Only worlds explicitly configured for shadow evaluation send inputs.

Each result records its exact request, SHA-256, model, complete distributions, usage, round and repair log reference in a separate immutable SemanticEvaluation entity. Path completion cannot invalidate a late evaluation callback. Responses never change cost flags, routing or probabilities. Missing credentials, incomplete inputs, oversized snapshots, invalid distributions and provider failures are recorded as errors, never as successful judgments. No automatic retries amplify provider load.

Comparison uses the same frozen snapshots for Jev and the recorded critic, with critic disagreement distinguished from accuracy. Human-labelled fixtures and resolved observed outcomes are separate evidence. Report coverage and errors alongside latency and cost. Whole-pipeline timing must include research, generation, queues and revisions. A shadow integration cannot claim a pipeline speedup.

A future adoption experiment requires held-out labels and an explicitly justified routing policy. No threshold or forecast accuracy is asserted by this implementation.

## Verification checkpoint — 2026-09-18

The pinned WASM builds successfully. Five module unit tests, eight integration tests and eleven existing forecast-registration contract tests pass. The integration suite builds the guest itself and exercises production Path spawn effects, the real actor/runtime, Cedar, internal OData reads and the WASM callback. It reads back Skipped for an off world and Failed/missing_typesafe_api_key for a shadow world without a credential. Provider-success and malformed-output cases use explicitly simulated HTTP responses; these are not live Jev measurements.

The comparison script's fixture checks cover coverage denominators, usage totals, duplicate-input exclusion from quality metrics, multiclass Brier calculation, empty input and rejection of critic-generated truth labels. All app TOML and CSDL parse and git diff whitespace checks pass.

The current acceptance baseline export has 56 preregistered forecasts and 23 paths (12 Scored, 5 Canonical, 5 Tail, 1 Solving); first pass is complete and deepening remains unfinished. That unfinished route prevents treating the baseline as a completed full-pipeline benchmark. The existing PR notes an incomplete OpenAI SSE stream in the repairer; this checkpoint does not independently establish its root cause.

No live Jev request or newly deployed full-pipeline run has been made at this checkpoint. The Foresight-specific `foresight_typesafe_api_key` location has not been supplied. Provider latency, billed usage, matched critic agreement, held-out quality and whole-pipeline comparative performance remain unmeasured. Acceptance deployment, full local-server verification and final review also remain outstanding. Do not treat this checkpoint as release approval or forecasting-accuracy evidence.
