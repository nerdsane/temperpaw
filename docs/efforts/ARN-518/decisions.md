# Decisions and tradeoffs

## Use the existing TemperPaw dashboard

**Decision:** Add Foresight to the authenticated TemperPaw dashboard.

**Came up because:** The user asked for a working interface as soon as possible, and the historical Temper-based Deep Sci-Fi frontend is in a closed, unmerged 117-file rewrite.

**Options:** Extend the current dashboard; revive the separate Next.js rewrite; build another standalone frontend.

**Chose the current dashboard because:** It already provides authenticated OData reads/actions and an event stream, keeping backend and UI in the same repository. This gives up the historical frontend's presentation work, which remains reference material.

**Where:** dashboard/src/routes/foresight; PR https://github.com/nerdsane/temperpaw/pull/526.

## Use an isolated worktree on the functioning governed computer

**Decision:** Work in a dedicated git worktree on Computers('arni-big') while its requested copy remains unresolved.

**Came up because:** Computer.Copy created 01a0a7c4-ad8d-7e20-b5d9-54b05ab62f08 in CopyUnknown, and reconciliation returned HTTP 404, while governed Exec on arni-big succeeded.

**Options:** Wait for the unresolved copy; use an isolated git worktree on arni-big; move implementation to the laptop.

**Chose the isolated worktree on arni-big because:** It retains the prescribed governed execution path and keeps other checkouts untouched while allowing progress. The shared machine requires disjoint worktrees and scoped build directories.

**Where:** /home/tl-user/worktrees/arn-518-foresight, branch codex/arn-518-learning-foresight, GitHub remote nerdsane/temperpaw.

## 3. Fit a bounded probability calibrator

**Decision:** Fit logistic calibration as the first trainable predictive component while retaining the scenario engine.
**Came up because:** The user prioritized a complete, observable learning loop quickly.
**Options:** Train a larger dynamics model; add memory only; fit a small predictor with held-out evaluation.
**Chose calibration because:** It changes real parameters and later probabilities, exposes its coefficients, and needs no GPU service. This component does not learn causal world dynamics.
**Where:** os-apps/paw-foresight/adrs/010-observable-learning.md; backend commit77539d5.

## 4. Let the authenticated server select the tenant

**Decision:** Remove the dashboard's hardcoded default-tenant header.
**Came up because:** Dedicated Foresight runs in tenant deep-sci-fi; authentication already injects its configured tenant.
**Options:** Add a second frontend configuration endpoint; duplicate transport; use existing authenticated context.
**Chose existing context because:** It preserves a single tenant authority and avoids sending Foresight requests to the wrong data.
**Where:** dashboard/src/lib/api.ts and dashboard/tests/foresight-transport.test.mjs.

## 5. Preserve Genesis changes during publication

**Decision:** Reconcile existing Genesis fixes before applying the learner delta.
**Came up because:** Genesis7c5b6e750b215d64e3d3178e7c9eb4abefaf1909 contains scenario, prompt, timeout and policy fixes absent from the GitHub mirror.
**Options:** Replace Genesis from GitHub; preserve the authoritative source and retain compatible build configuration.
**Chose preservation because:** A learning feature must not remove existing runtime behavior. One inherited prompt failed its existing contract test; restoring a concrete EventNode payload preserves the new execution guidance and makes the test pass.
**Where:** os-apps/paw-foresight agents, world specification, policy and reconciled existing WASM modules.

## 6. Reuse the published runtime with identical server source

**Decision:** Run the new app and UI against the verified published f1b3f892ce652f1355daeabe68ae49cd3de3fa85 runtime for local proof.
**Came up because:** A fresh native build consumed the computer's remaining disk capacity.
**Options:** Continue until disk exhaustion; alter shared caches; use the published binary after checking source identity.
**Chose the published binary because:** The server, transport and Cargo lockfile diff against base07cbb1d is empty. The binary came from image digest sha256:8ba6cb3b887ee7d7a15aaaf587711b1c9507bcce8af5012e3a917c61d68e24e3; its SHA-256 is c9e507322df4657bef5152004bf67fc17d88038a3bbae06d866cbc50ff57b276. Only this effort's incomplete build cache was removed. A missing packaged file-storage module was rebuilt from its own source.
**Where:** Local proof on governed arni-big, isolated PostgreSQL database arn518, with evidence retained outside the repository.
