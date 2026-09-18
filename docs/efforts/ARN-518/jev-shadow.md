# Jev shadow evaluation in Foresight

The existing generator, repairer, critic, deterministic costing and immutable forecasts remain the authority. On each StartChallenge, a separate declared integration evaluates a frozen repair and its required events using Jev 1.13.0. Only worlds explicitly configured for shadow evaluation send inputs.

Each result records its exact request, SHA-256, model, complete distributions, usage, round and repair log reference in a separate immutable SemanticEvaluation entity. Path completion cannot invalidate a late evaluation callback. Responses never change cost flags, routing or probabilities. Missing credentials, incomplete inputs, oversized snapshots, invalid distributions and provider failures are recorded as errors, never as successful judgments. No automatic retries amplify provider load.

Comparison uses the same frozen snapshots for Jev and the recorded critic, with critic disagreement distinguished from accuracy. Human-labelled fixtures and resolved observed outcomes are separate evidence. Report coverage and errors alongside latency and cost. Whole-pipeline timing must include research, generation, queues and revisions. A shadow integration cannot claim a pipeline speedup.

A future adoption experiment requires held-out labels and an explicitly justified routing policy. No threshold or forecast accuracy is asserted by this implementation.

## Verification checkpoint — 2026-09-18

The pinned WASM builds successfully. Five module unit tests, eight integration tests and eleven existing forecast-registration contract tests pass. The integration suite builds the guest itself and exercises production Path spawn effects, the real actor/runtime, Cedar, internal OData reads and the WASM callback. It reads back Skipped for an off world and Failed/missing_typesafe_api_key for a shadow world without a credential. Provider-success and malformed-output cases use explicitly simulated HTTP responses; these are not live Jev measurements.

The comparison script's fixture checks cover coverage denominators, usage totals, duplicate-input exclusion from quality metrics, multiclass Brier calculation, empty input and rejection of critic-generated truth labels. All app TOML and CSDL parse and git diff whitespace checks pass.

The current acceptance baseline export has 56 preregistered forecasts and 23 paths (12 Scored, 5 Canonical, 5 Tail, 1 Solving); first pass is complete and deepening remains unfinished. That unfinished route prevents treating the baseline as a completed full-pipeline benchmark. The existing PR notes an incomplete OpenAI SSE stream in the repairer; this checkpoint does not independently establish its root cause.

Recovered session reports document 101 earlier live Jev API requests using the credential already supplied by the user: 12 exploratory, 23 prediction/update and 66 broader tests. Reported median client latency was 0.354, 0.315 and 0.299 seconds respectively. Those exploratory experiments did not exercise this new WASM integration or the full pipeline. The credential location on the revived host remains to be recovered. Live integration latency, billed usage, matched critic agreement, held-out quality and whole-pipeline comparative performance remain unmeasured. Acceptance deployment, full local-server verification and final review also remain outstanding. Do not treat this checkpoint as release approval or forecasting-accuracy evidence.

## Fresh live API comparison — 2026-09-18

After the credential was supplied on this host, 70 fresh pinned-model requests succeeded: 48 repeated exploratory cases and 22 structured Foresight paths. The real-path packets withheld existing critic flags and contained claims, causal nodes and 14 determined observations. Jev reported evidence/timing clear on all 22; prerequisites were defect on 4 and unknown on 18. Median client latency was 0.399 seconds, p95 0.471 seconds, and input usage was 134,091 tokens across the 22 calls.

This is not a complete pipeline or a live-WASM acceptance test. The existing critic had additional narratives and broader incentive/lag checks, so its findings on all 22 paths cannot be treated as equivalent labels or proof that Jev missed 18 defects. FileVersions access was denied by Cedar; the denied version was not fetched by another route. Full narrative replay, independent truth labels and end-to-end timing remain outstanding. The results do not justify replacing the critic. Exact request/response evidence and the report are published in the session's Foundry catalogue as `arn518-live-path-comparison-results.json`, `arn518-live-jev-replay.json` and `arn518-live-jev-comparison-report.md`.
