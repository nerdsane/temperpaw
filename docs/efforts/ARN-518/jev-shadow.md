# Jev and the Foresight critic: shared evidence

Foresight now prepares one complete evidence packet before either evaluator starts. `Path.StartChallenge` runs `prepare_challenge`; its `ChallengePrepared` callback persists the packet and SHA-256, rejects a callback for an obsolete repair log, and enters the system-only `LaunchChallenge` action. That action launches the reasoning critic and an independent SemanticEvaluation with copies of the same packet.

The packet contains the complete repair narrative, complete endpoint bundle, observed skeleton, required causal nodes, repairer's existing flags and the exact four questions: contradiction, incentive, lag and unexplained discontinuity. No document is silently truncated. A 128 KiB application cap bounds the complete serialized packet; this byte cap is not a guarantee of fitting the provider's separate token limits. Missing/empty documents and oversized packets fail explicitly.

Both consumers verify the stored hash. In shadow mode the critic has only reporting tools, so neither evaluator can independently fetch different evidence. They use the same rubric and clear/defect/unknown choices. The critic additionally provides explanations and reports only costs not already acknowledged by the repairer; Jev's judgments never change scores, routing or forecasts. Existing off-mode research behavior is retained.

Each SemanticEvaluation becomes Recorded, Skipped or Failed, with a two-minute timeout and immutable terminal record. A successful record retains the packet hash, exact provider request, pinned and returned model, distributions, usage and elapsed time. Accepted rounding is preserved in raw records; semantic Brier calculations normalize the rounded distribution. Forecast probabilities are never inferred from Jev confidence.

## Matched live comparison, 2026-09-18

The prior structured-only comparison omitted narratives and is superseded. All 26 source documents were retrieved with the Foresight MCP's `navigate("Files('…')/$value")`; the earlier FileVersions denial did not prevent reading these current files.

All 22 completed paths from run C were replayed against GPT-5.5 through Foresight Sessions and Jev 1.13.0 through the compiled evaluation WASM, using identical frozen packets and question sets. One incomplete source path was excluded explicitly. Both models completed all 22 cases. Packet identities and Jev request contents are checked programmatically.

Agreement was 63/88 judgments: contradiction 22/22, incentive 18/22, lag 17/22, unexplained discontinuity 6/22. Jev marked lag and discontinuity defects on every path; GPT-5.5 did so on 17 and 6 respectively. Agreement is not accuracy: there are no independent gold labels, and the paths share one world and three endpoints. This does not justify replacing the critic.

The production-host WASM Jev replay had a 0.249 s median invocation, 0.561 s p95 and 309,371 input tokens. At the documented $0.042 per million input tokens, estimated cost is $0.012994, not a billing readback. An earlier direct-HTTP preflight agreed on 86/88 categorical judgments with a 0.526 s median; connection reuse and transport differ. For 18 timed reasoning Sessions, median observed completion was bounded between 25.2 and 31.4 seconds by polling; four initial cases lack precise timing. Those clocks are not equivalent. Session token counters returned 1/1 and cannot support a reasoning-cost estimate.

This is a complete paired replay of the critic stage with common upstream paths held fixed, not a newly generated research-to-forecast pipeline run. It does not measure whole-pipeline speedup or future forecast accuracy. The app changes are tested locally; the installed acceptance app has not been replaced.

The report and exact evidence are in Foundry: `arn518-fair-comparison-report.md`, `arn518-fair-packets.json`, `arn518-fair-jev-wasm-results.json`, `arn518-fair-reasoner-sessions.json`, and `arn518-fair-comparison-final.json`. Recompute with `scripts/compare_foresight_evaluators.py`. API model/pricing source: https://docs.typesafe.ai/models.

## Verification

The shared-packet guest test exercises preparation and both consumers, checks that no consumer refetches documents, compares their complete input and rubric, and verifies tampering rejection. Runtime tests prove the callback/system-entry/spawn sequence, terminal outcomes and callback permissions. A stale preparation cannot overwrite a newer repair. The explicit ignored live test replays frozen packets through the actual compiled Jev guest using ProductionWasmHost; credentials are supplied only through a restricted file path and never appear in artifacts.
