# Decisions and tradeoffs

## D1 — Use the existing TemperPaw dashboard

**Decision**: Add Foresight to the authenticated TemperPaw dashboard.

**Came up because:** The user asked for a working interface as soon as possible, and the historical Temper-based Deep Sci-Fi frontend is in a closed, unmerged 117-file rewrite.

**Options:** Extend the current dashboard; revive the separate Next.js rewrite; build another standalone frontend.

**Chose the current dashboard because:** It already provides authenticated OData reads/actions and an event stream, keeping backend and UI in the same repository. This gives up the historical frontend's presentation work, which remains reference material.

**Where:** dashboard/src/routes/foresight; PR https://github.com/nerdsane/temperpaw/pull/526.

## D2 — Use an isolated worktree on the functioning governed computer

**Decision**: Work in a dedicated git worktree on Computers('arni-big') while its requested copy remains unresolved.

**Came up because:** Computer.Copy created 01a0a7c4-ad8d-7e20-b5d9-54b05ab62f08 in CopyUnknown, and reconciliation returned HTTP 404, while governed Exec on arni-big succeeded.

**Options:** Wait for the unresolved copy; use an isolated git worktree on arni-big; move implementation to the laptop.

**Chose the isolated worktree on arni-big because:** It retains the prescribed governed execution path and keeps other checkouts untouched while allowing progress. The shared machine requires disjoint worktrees and scoped build directories.

**Where:** /home/tl-user/worktrees/arn-518-foresight, branch codex/arn-518-learning-foresight, GitHub remote nerdsane/temperpaw.

## D3 — Fit a bounded probability calibrator

**Decision**: Fit logistic calibration as the first trainable predictive component while retaining the scenario engine.

**Came up because:** The user prioritized a complete, observable learning loop quickly.

**Options:** Train a larger dynamics model; add memory only; fit a small predictor with held-out evaluation.

**Chose calibration because:** It changes real parameters and later probabilities, exposes its coefficients, and needs no GPU service. This component does not learn causal world dynamics.

**Where:** os-apps/paw-foresight/adrs/010-observable-learning.md; backend commit 77539d5.

## D4 — Let the authenticated server select the tenant

**Decision**: Remove the dashboard's hardcoded default-tenant header.

**Came up because:** Dedicated Foresight runs in tenant deep-sci-fi; authentication already injects its configured tenant.

**Options:** Add a second frontend configuration endpoint; duplicate transport; use existing authenticated context.

**Chose existing context because:** It preserves a single tenant authority and avoids sending Foresight requests to the wrong data.

**Where:** dashboard/src/lib/api.ts and dashboard/tests/foresight-transport.test.mjs.

## D5 — Preserve Genesis changes during publication

**Decision**: Reconcile existing Genesis fixes before applying the learner delta.

**Came up because:** Genesis commit 7c5b6e750b215d64e3d3178e7c9eb4abefaf1909 contains scenario, prompt, timeout and policy fixes absent from the GitHub mirror.

**Options:** Replace Genesis from GitHub; preserve the authoritative source and retain compatible build configuration.

**Chose preservation because:** A learning feature must not remove existing runtime behavior. One inherited prompt failed its existing contract test; restoring a concrete EventNode payload preserves the new execution guidance and makes the test pass.

**Where:** os-apps/paw-foresight agents, world specification, policy and reconciled existing WASM modules.

## D6 — Reuse the published runtime with identical server source

**Decision**: Diagnose the app and UI integration against the verified published f1b3f892ce652f1355daeabe68ae49cd3de3fa85 runtime before rebuilding the required session adapter.

**Came up because:** A fresh native build consumed the computer's remaining disk capacity.

**Options:** Continue until disk exhaustion; alter shared caches; use the published binary after checking source identity.

**Chose the published binary because:** The server, transport and Cargo lockfile diff against base 07cbb1d is empty. The binary came from image digest sha256:8ba6cb3b887ee7d7a15aaaf587711b1c9507bcce8af5012e3a917c61d68e24e3; its SHA-256 is c9e507322df4657bef5152004bf67fc17d88038a3bbae06d866cbc50ff57b276. Only this effort's incomplete build cache was removed. A missing packaged file-storage module was rebuilt from its own source.

**Where:** Local proof on governed arni-big, isolated PostgreSQL database arn518, with evidence retained outside the repository.


## D7 — Authenticate browser requests through the supported inner API contract

**Decision**: Translate a verified dashboard session into the server's existing single-use credential scoped to its identity, tenant, method and URI.

**Came up because:** A real signed-in browser passed /auth/me but received an empty401 from the inner platform bearer middleware. That middleware does not accept the outer dashboard's typed context by itself.

**Options:** Change the kernel; bypass its credential check; use its existing internal invocation credential mechanism from the authenticated application adapter.

**Chose the existing credential mechanism because:** It retains both authentication layers and the user's permissions while fixing the necessary TemperPaw integration. Untrusted headers cannot mint a credential.

**Where:** crates/temperpaw/src/auth.rs and crates/temperpaw/src/startup.rs; composed-router regression test and live browser acceptance flow.


## D8 — Continue on the approved replacement computer

**Decision**: Restore the integration branch on an isolated copy of arni-big-2.

**Came up because:** arni-big stopped accepting Exec calls during the native build, and its documented Wake action returned HTTP404 Sandbox not found. The user explicitly approved the replacement computer.

**Options:** Wait for the missing sandbox; move to the laptop; use an isolated copy of another governed computer.

**Chose the governed copy because:** It preserves the prescribed execution layer and has enough disk capacity for a fresh build. Interrupted tests and scratch artifacts are recreated and rerun rather than treated as completed evidence.

**Where:** Computer 01a0a805-fcd7-7252-94fa-8100d049e845; integration worktree /home/tl-user/worktrees/arn-518-foresight.

## D9 — Initialize forecasts through declared transitions

**Decision**: Register forecasts through a declared one-at-a-time World → Forecast → World transition loop.

**Came up because:** The first authenticated browser registration returned HTTP400 StrictActionContract; strict entities accept only identity during creation.

**Options:** Disable strictness; dispatch Register inside WASM; add kernel initializer support; use existing entity triggers.

**Chose declared entity triggers because:** They preserve strict initialization, immutable revisions, deterministic retry identities and the one-concern WASM rule without a kernel change. This adds one transient world state and persisted pending-registration fields.

**Where:** os-apps/paw-foresight/specs/world.ioa.toml, specs/forecast.ioa.toml, and wasm/register_forecasts/src/lib.rs.

## D10 — Keep learning callbacks outside operator authority

**Decision**: Restrict learning, model-adoption, forecast-registration and grading callbacks to their declared system and WASM principals, including against existing broad Admin permits.

**Came up because:** The first review and a failing Cedar test proved that a signed-in dashboard admin could submit forged LearningRun.Prepared data. The same broad permits cover World and Forecast, whose callbacks supply the new learner's model and scored examples.

**Options:** Narrow only LearningRun's Admin permit; enforce the boundary across every callback that produces this learner's model or prediction record.

**Chose the complete callback boundary because:** Cedar permits are additive. Explicit denies prevent broader dashboard permissions from bypassing the new integrity contract while retaining create/start, replay, prediction-update and reviewed outcome-entry controls.

**Where:** os-apps/paw-foresight/policies/foresight.cedar; crates/temperpaw/tests/corridor_cedar_matrix.rs.

## D11 — Recover registration and preserve its model snapshot

**Decision**: Return failed forecast registration to an operable world with a visible error, and retain one model snapshot throughout each registration pass.

**Came up because:** Review found that a bad replay date or transient lookup failure permanently failed the world, while concurrent adoption could interrupt or change a registration pass.

**Options:** Keep terminal failure and require a new world; hide invalid dates by adjusting them; preserve the supplied clock, expose the error and allow a retry.

**Chose recovery with a frozen pass model because:** Invalid or unavailable inputs should not destroy the world, and forecasts in one pass need an auditable model identity. Observed registration uses the host event time; replay and hindcast retain explicit logical time. A matching authoritative forecast read advances the loop even if its list projection lags.

**Where:** os-apps/paw-foresight/specs/world.ioa.toml and wasm/register_forecasts/src/lib.rs.

## D12 — Start learning once after a grading batch

**Decision**: Use a separate system-only batch scoring transition and one declared learning trigger after hindcast grading completes.

**Came up because:** The new per-Score trigger would start a competing learning run for every revision in a hindcast batch.

**Options:** Keep every per-revision run; add a background scheduler; preserve existing batch grading and trigger learning once from its completion.

**Chose the batch completion trigger because:** It retains all scores without a new orchestration service or repeated fitting over the same batch. Standalone scoring and reviewed outcome entry retain their automatic feedback paths.

**Where:** os-apps/paw-foresight/specs/forecast.ioa.toml, specs/hindcast.ioa.toml, and wasm/grade_hindcast/src/lib.rs.

## D13 — Use configured research models and the existing settings page

**Decision**: Research creation uses the server's configured provider/model pair and links missing setup to the authenticated settings page.

**Came up because:** The first review found that the new form omitted both fields required by seed_world. The existing welcome flow cannot configure every supported provider or the model.

**Options:** Guess model defaults; build another provider form; reuse non-secret setup metadata and the existing settings controls.

**Chose the existing setup contract because:** It gives the UI a real configured pair without exposing credentials or duplicating model configuration. Settings must remain reachable before initial setup is complete; authentication still applies. Replay remains available independently of provider setup.

**Where:** dashboard/src/routes/foresight/+page.svelte and dashboard setup routing.

## D14 — Package the required file-storage module

**Decision**: Use one aggregate paw-fs WASM build entry in Docker and CI so all existing child module builders run.

**Came up because:** The immutable release image for d758711 failed its isolated fresh startup: paw-fs declares artifact_batch_apply as app-required, but Docker builds only blob_adapter and workspace_fs. The missing artifact predates this effort, but prevents the complete release image from booting for this app's required verification.

**Options:** Add another per-module entry to both callers; give paw-fs one aggregate build entry like the other apps; trim the core app or copy a module into the extracted image.

**Chose the aggregate entry because:** It removes the duplicated per-module selection that caused the omission, reuses the existing builders, and keeps the tested image intact. The change is limited to packaging and its existing regression coverage; file-storage behavior and the platform are unchanged.

**Where:** Dockerfile; .github/workflows/ci.yml; os-apps/paw-fs/wasm/build.sh; existing WASM packaging tests. Failed immutable image sha256:d221367b10339a427e2be4c7fe9aac73a285a8728313a243adcc5792ed85b2cb; raw evidence /tmp/arn518-image-verification/image-verification-d758711.json.

## D15 — Permit dashboard research to create its workspace

**Decision:** Grant authenticated admins create-only access to Workspace in the Foresight app policy.

**Came up because:** On 16 September, the real-provider browser flow at 33e74b7 created and configured a world, then failed at seed_world's Workspace POST with HTTP 403. The kernel preserves the initiating admin identity for transition-triggered internal HTTP; the filesystem policy permits only agents to create workspaces.

**Options:** Add the required create-only app permission; impersonate an agent or change kernel identity propagation; pre-create workspaces outside the user flow.

**Chose the create-only permission because:** It preserves verified caller identity and makes the normal dashboard flow work without kernel changes or special test setup. It grants no workspace read, update, delete, freeze, or archive access. Rita explicitly approved this permission change after the denial was reported.

**Where:** os-apps/paw-foresight/policies/foresight.cedar; crates/temperpaw/tests/corridor_cedar_matrix.rs; PR #526. The regression failed against 33e74b7 before the permission was added.

## D16 — Authorize the session runtime's transcript operations

**Decision:** Permit only Agent::"service:wasm-runtime" to create, read and list SessionEntry records in the owning paw-agent policy.

**Came up because:** After the approved workspace fix, browser research created its workspace and session, but the callback runtime failed to materialize the first transcript entry with AuthorizationDenied (PD-01a0aaa4-7ad0-75f0-8821-339e0e501e58). Existing permits cover admins and named agent types but omit the authenticated callback runtime.

**Options:** Grant the exact runtime identity its append/read operations; broaden access by agent type; bypass transcript materialization or change kernel identities.

**Chose the exact identity because:** It repairs the existing session pipeline while preserving authentication and denying unrelated identities, transcript updates and deletes. Rita explicitly approved this narrow permission after the reproduced denial. The change belongs to paw-agent and must be published with that dependency before the Foresight app is installed.

**Where:** os-apps/paw-agent/policies/session_entry.cedar; crates/temperpaw/tests/session_entry_runtime_policy.rs; PR #526. The new regression failed on transcript creation before the permission was added.
