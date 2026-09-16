# 010: Observable learned calibration

## Decision

Keep the existing scenario and backcasting engine. Add a bounded logistic calibrator over its raw probabilities, with immutable prediction revisions and a recorded LearningRun lifecycle.

## Why

The first working system must actually change predictive parameters, retain evidence, and make adoption understandable. A two-parameter calibrator supports exact deterministic replay and exposes the effect of each change without a GPU dependency. This does not claim to learn causal laws or the full dynamics of a world.

## Contract

LearningRun has Preparing, Training, Evaluating and Adopting states. Each computational concern has one WASM integration; callbacks and declared entity triggers sequence the work. The calibrator fits log-loss with 500 deterministic gradient steps and bounded parameters. Evaluation compares candidate and incumbent Brier loss on the same later examples.

The first two-thirds of eligible events by resolution time are available for fitting. Evaluation additionally requires registration after the last training resolution and the incumbent's previous evaluation boundary. At least 12 training and 8 fresh validation events are required. Adoption requires Brier improvement of 0.005 and more than two paired standard errors. This is a conservative operational threshold, not a population accuracy guarantee.

A World adopts through a param_equals_field constraint on the exact preceding model JSON. A stale candidate cannot overwrite a newer version. Successful adoption confirms the LearningRun through an entity trigger. Stalled or rejected adoption preserves the incumbent.

Observed, historical and simulated worlds keep separate model provenance. OpenReplay opens only historical or simulated worlds, without a surveyor. Synthetic fixtures contain raw inputs, never precomputed outputs. Uploaded datasets cannot masquerade as observed outcome history.

Forecast registration retains base probability, applied model, run identity, revision link, logical registration time and provenance. A stable identity from the world/event/input/model tuple makes retries converge at storage. A conflicting identity is checked rather than silently accepted. Once an event has a recorded outcome it cannot receive a new preregistration.

RecordOutcome validates sources and times, computes each revision's score, propagates the outcome down earlier revisions through entity triggers, and creates a new learning run. Forecast and LearningRun generic updates are forbidden.

## Bounds and limits

Runs accept at most 512 examples and one MB of input JSON. History queries refuse truncation. Each computation/adoption stage has a 120-second failure timeout. Insufficient or temporally overlapping examples lead to rejection.

Source references and timestamps are supplied evidence, not independently verified facts. Historical base probabilities may contain hindsight from the model that generated them. A simulated demonstration establishes operation, not empirical predictive quality. Real prospective outcomes remain necessary to establish value.

## Verification

Pure Rust production logic has red/green tests for actual parameter changes, later predictions, temporal separation, duplicate exclusion, provenance rejection, future outcomes, replay determinism, consumed validation, insufficient data and invalid dates/models. Forecast registration tests exercise deterministic identities and finite inputs. Runtime verification must additionally exercise the declared state machine, atomic adoption and duplicate delivery.

## Existing feedback paths

Evidence ingest and historical grading retain their existing orchestration. Their Resolve payloads now carry resolution time and outcome provenance, and Forecast.Score declares the next learning trigger. Market-price threshold resolutions remain proxy evidence, excluded from empirical fitting. Historical grading carries recorded actual timestamps and references, labels approximate matches unverified, and scores every revision of the matched underlying event.

Observed manual resolutions compare their resolution date to the host-recorded action timestamp, so future outcomes cannot enter observed fitting. Conflicting outcomes for one event are excluded. Generic PATCH/PUT authorization uses the kernel UPDATE_ACTION (update); the precheck label patch is not a Cedar action. Strict action parameters also prevent probability/model mutation through extra action fields.
