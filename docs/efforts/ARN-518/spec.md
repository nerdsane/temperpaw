# Foresight learning and observation

## Outcome

A user can inspect a world and its predictions, follow the engine's activity, run accelerated learning from dated experiences, and see evidence of what changed. New resolved outcomes drive subsequent learning. Model parameters actually change; stored prose alone is insufficient.

## Contract

1. Preserve the current World, EventNode, Endpoint, Path, Claim and Forecast capabilities.
2. Every prediction is immutable and records its raw input probability, applied model version, evidence provenance and registration time. A changed probability or model produces a revision linked to the previous prediction. Repeating the same registration is idempotent.
3. An outcome identifies the question, resolution time and sources. Observed, historical and simulated experiences remain distinguishable. Voided, duplicated, undated, future or unsupported outcomes cannot silently become training truth.
4. Learning has visible preparation, training, evaluation and final states. A run retains its input identities, preceding model, candidate parameters, sample counts, evaluation results and reason for adoption or rejection.
5. Train a bounded predictive component from outcomes. Evaluate on examples excluded from that candidate's fitting. Temporal evaluation must not use an outcome before it was available. Compare candidate and preceding model using identical examples and features.
6. Adopt only supported improvements. Insufficient evidence, malformed data, non-finite values and failed evaluations preserve the incumbent and explain why. Concurrent or repeated completion must not replace a newer model with an older candidate.
7. Historical replay advances a supplied logical time and exercises the same learner. It does not wait for wall time. Simulation data may demonstrate mechanics but must be labeled and must not promote a live model as empirical evidence.
8. New verified outcomes can trigger another learning cycle through declared Temper transitions. The engine's control flow lives in entity specs and WASM callbacks, without an independent background orchestrator.
9. Applied models affect later registered predictions. The interface shows the input probability, revised probability, model identity and supporting run.
10. Authenticated UI: worlds and dependencies; prediction detail and revisions; activity and failures; learned changes and evaluation. Reuse the existing dashboard client and event stream. No server credential enters browser code.
11. All queries and computation have explicit limits. Truncation or skipped examples are reported. Failures remain visible.

## First end-to-end acceptance case

Create or load a world. Display its events, paths and registered predictions. Supply a dated replay dataset with explicit provenance. Run a real parameter update through Temper. Observe training and evaluation, an adopted candidate or a justified rejection, and a subsequent prediction that uses the adopted model. Refresh the page and verify the records persist. Resolve a new forecast with evidence and prove that feedback reaches the learning path. Repeat delivery and completion events to verify no duplicate scoring or stale model adoption.

A deterministic simulation fixture demonstrates mechanics and is labeled as such. Historical or live quality claims require independently sourced outcomes and comparable scores. No generated display content stands in for a missing engine result.

## Invariants and model

- Registered prediction contents never change.
- Each event contributes at most once to a fitting set.
- Candidate evaluation examples do not enter that candidate's fitting set.
- A prediction uses only the adopted model available at registration.
- A rejected or failed candidate leaves that adopted model unchanged.
- A stale completion cannot overwrite a newer adopted model.
- Simulated feedback never counts as observed evidence.
- Replaying identical inputs in identical order gives identical parameters and scores.
- Terminal learning runs cannot be rewritten.

The implementation's entity state model and deterministic tests must express and check these same invariants. The formal expression will use the project's existing model and verification machinery, without a parallel handwritten orchestration implementation.

## Limits

The first trainable component improves bounded event prediction and calibration. Its coefficients establish learned predictive associations, not discovered causal laws. The design must support future improvements to consequence models and research choice without claiming those research ambitions have already been validated.
