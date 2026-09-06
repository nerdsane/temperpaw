# DSF factory

This Temper app records Deep Sci-fi's resources, observed behavior, participants,
resource operations and isolated experiments. DSF's product implementation stays
in its existing repository. Existing Intent, Effort and Ask records describe the
work and decisions that change these resources.

The executable contract is in `specs/*.ioa.toml`. `model.csdl.xml` exposes the same
fields and bound actions. The Rust contract suite checks their agreement and runs
the IOA through Temper's production transition evaluator and actor simulator.

## Resource and evidence updates

Create each record in Draft, then invoke its registration action. Resource IDs
must remain stable across sync runs. Provider identity, source repository and
intended configuration are registered separately from observations.

Deployment configuration and source revision come from the deployment provider.
Datadog service telemetry uses a separate linked resource, so an instrumentation
version or missing telemetry field cannot replace a deployed commit or binding.

`DsfResource.Observe` accepts a measured observation with its evidence reference,
timestamp and expected current sequence. It rejects a stale timestamp or sequence
before changing any field. Successful updates increment `observed_sequence`.
`ObserveUnavailable` records absent, inaccessible or stale coverage and preserves
the last measured configuration and revision. Consumers must check
`observation_available` before presenting those retained facts as current.

`SetIntent` requires a decision reference and separate authorization. Observers
cannot call it as part of collection. Drift must produce an existing Effort Ask;
observing drift does not approve the new configuration.

`DsfObservation` records are immutable after `RecordMeasured`, `RecordAbsent`,
`RecordInaccessible` or `RecordStale`. Their query window, source event and evidence
reference identify what was inspected. Sampled spans, request metrics, jobs and
participants retain distinct sample kinds and units in their summaries.

`DsfFlow` revisions identify the source revision, entry points, resource IDs and
outcomes. `DsfParticipant.RegisterAgent` requires a real product participant ID;
`RegisterCohort` records an aggregate anonymous population. Neither accepts API
keys or user content. Activity updates have the same sequence/timestamp checks as
resource observations.

## Deployment contract

`DsfResource.RequestOperation` reserves the resource and creates a child operation
at the accepted operation key. While Operating, the resource rejects another
request and continues accepting observations. `DsfOperation.Request` requires
`operation_key` to equal its canonical ID and fixes the resource, Effort, operation
kind and intended revision. Later actions cannot replace these fields.

Declared reactions advance Request to validation, validation success to execution,
and a known provider execution to verification. Terminal Verified, Failed and
Cancelled outcomes release the matching resource reservation. An uncertain
provider result retains it. Each integration performs one concern and returns a
callback; it never dispatches transitions.

| Action and integration | Result |
|---|---|
| `Validate` / `dsf_operation_validate` | `ValidationSucceeded`, `ValidationBlocked` or `ValidationFailed`. Validate the resource operation permission, linked Effort, unresolved Asks, revision and required proof. |
| `Execute` / `dsf_operation_execute` | `ExecutionSucceeded` or `ExecutionUncertain`. Reuse the accepted operation key at the provider. |
| `Reconcile` / `dsf_operation_observe` | `ProviderFound`, `ProviderAbsent` or `ObservationFailed`. Read provider execution before deciding whether another write is permitted. |
| `Verify` / `dsf_operation_verify` | `VerificationSucceeded`, `VerificationPending` or `VerificationFailed`. Check the exact resource/revision, affected flow and required Datadog telemetry. |

Successful callbacks carry the accepted `operation_key`. Verification also
matches `verified_resource_id` and `verified_revision` against the accepted target,
and requires provider, flow and telemetry evidence references. The integration
must establish that each check passed; nonempty references alone are insufficient.
Callback authorization must prevent clients from substituting self-reported proof.

An uncertain provider result permits observation only. A definitive absence can
permit another attempt, bounded to three attempts with the same key. Provider
lookups are bounded to twenty attempts; exhausting them leaves the resource
reserved and the uncertainty visible. Once a provider execution is known, the
operation cannot execute again. Verification allows 40 attempts; unavailable or delayed evidence returns to Observed with the resource lock held. Verified is terminal. Rollback is another
operation with its own target and verification.

## Recurring observation

`DsfModelSync.Refresh` increments the sync sequence and invokes
`dsf_model_collect`. Collection performs bounded provider reads using the existing
service and has no agent/model calls. Each success callback must match the current
sync sequence; delayed results from an earlier cycle fail.

Each sync records one source/resource observation. `CollectionSucceeded` records
source cursor, evidence and last-success time. A declared reaction creates the
immutable observation, whose `RecordResourceMeasured` reaction updates the
resource. If that update loses the sequence comparison, the evidence remains
recorded and newer resource facts remain unchanged.

`CollectionAbsent`, `CollectionStale` and `CollectionInaccessible` record their
respective evidence and use `ObserveUnavailable` to preserve prior measured
facts. Inaccessible collection also retains the prior last-success time. Successful
collection uses the declared `schedule_at` effect to request the next refresh.
Three consecutive failures stop automatic retry. Pause stops scheduled collection;
Resume allows another bounded read. The deployment budget accounts for existing
service hosting. New paid compute and experiments require bounded cost authority in their own path. These collectors do not create spending reservations.

## Experiments

`Configure` fixes the variant's Effort, branch/revision, computer, database, media
bucket and namespace. `dsf_experiment_validate` resolves production identities
from authoritative resources and returns isolation evidence. Its callback rejects
the production database or media bucket, including a different prefix in that
bucket. Execution requires successful isolation validation.

`dsf_experiment_run` returns results and application-test evidence.
`Select` records the existing decision Ask and a delivery Effort; it does not
deploy the variant. Authorization must verify that this Ask selected this variant.
`dsf_experiment_cleanup` removes the experiment's own resources and records cleanup
evidence. Cleaned is terminal.

## Verification

Run `cargo test -p temperpaw --test dsf_factory_contract` from the repository root.
The suite executes the production `TransitionTable` and `EntityActorHandler`
inside `SimActorSystem`. `SpecRegistry` loads declared cross-entity reactions into
`SimReactionSystem`. Tests cover replay, out-of-order observations, immutable
targets, resource reservations, uncertain provider responses, bounded retries,
missing evidence, isolation, inaccessible sources and deterministic scheduler
faults. Cascade tests preregister target actors, so live proof must also establish
durable create-if-missing behavior.

The actor simulator substitutes the external environment. The same suite also builds and invokes all four operation WASM modules through the real engine with controlled HTTP responses; see [operation adapter contract](wasm/dsf_operation_common/README.md). Live provider reads/writes, persistent-store restart recovery,
callback authorization, Datadog checks and browser flows require the effort's
separate end-to-end proof. These tests cannot establish those results.
