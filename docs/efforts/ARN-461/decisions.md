# Decision log — ARN-461

**Decision:** (2026-09-03) Three chain_* modules, not an extension of `review_gate_lifecycle`.
**Came up because:** Rita said WASM inspects rows and retracts a bool. `review_gate_lifecycle` already dispatches PassReview / AttachProofPacket.
**Options:** (1) add checks inside `review_gate_lifecycle`; (2) one mega `chain_sdlc_ready`; (3) one module per verb.
**Chose (3) over (1) and (2) because:** (1) hides more dispatch. (2) mixes review, proof, and commit pin. What we gave up: three Cargo.toml files instead of one.
**Where:** `os-apps/paw-patrol/wasm/chain_review_ready`, `chain_proof_ready`, `chain_merge_ready`.

---

**Decision:** (2026-09-03) Merge WASM on_failure returns Proving. The agent still fires Merge.
**Came up because:** Merge is Proving → Merged. A self-loop bool cannot pin `head_sha` until Merge names it.
**Options:** (1) new CheckRecords verb before Merge; (2) Merge stays Merged on a failed check; (3) RetractMerge back to Proving and clear the bools.
**Chose (3) over (1) and (2) because:** (1) is another agent step. (2) leaves a false Merged. Merge does not call GitHub, so retracting the state is safe. What we gave up: a brief Merged flicker if the rows fail.
**Where:** `effort.ioa.toml` Merge / RetractMerge.

---

**Decision:** (2026-09-03) CI lists Temper rows by commit. It does not read PR comments.
**Came up because:** The vendored gate scraped `sdlc-review-record-b64` comments and folded a hidden `<!-- sdlc-review -->` comment.
**Options:** (1) keep comments as fallback; (2) Temper only, fail closed.
**Chose (2) because:** Rita said one book and no hidden comment. What we gave up: a PR with only a comment record and no Temper rows fails until the implementer writes the rows.
**Where:** `.github/workflows/sdlc-review.yml`, `sdlc-verification.yml`.

## Continue the existing recording effort

**Decision:** Repair the confirmed operational gaps under ARN-461 with one PR per affected repository.

**Came up because:** TemperPaw PR 500 installed merge gates before ordinary harnesses had a complete validated submission path; three current tasks are blocked by related contract mismatches.

**Options:** Patch each blocked record with a one-off grant; repair shared contracts; redesign the full lifecycle.

**Chose shared contract repair over one-off grants and a redesign because:** It addresses the recurring failure while retaining validation and keeping the task bounded.

**Where:** docs/efforts/ARN-461; Temper Intent arn461-gate-repair-20260910; user authorization in Codex task 01a08157-a252-7b43-b1d9-facd84cd2695.

## Preserve retired review rounds without blocking their replacements

**Decision:** Exclude explicitly Superseded ReviewRuns from the active review and merge panel, while requiring their original record to remain present.

**Came up because:** Effort appends review IDs across rounds, but both row validators rejected the existing terminal Superseded state, so a historical failed round blocked a later passing panel indefinitely.

**Options:** Delete historical attachments; infer retirement from commit differences; honor the existing Supersede transition.

**Chose explicit supersession over deletion or inference because:** It preserves honest evidence and makes retirement intentional. Requested, failed, or stale active runs still block; retired runs never contribute model votes. Requiring record_present also retains the ReviewRun RecordedHasRecord invariant. The tradeoff is one explicit retirement action after replacement confirmation exists.

**Where:** os-apps/paw-patrol/wasm/chain_review_ready/src/lib.rs; os-apps/paw-patrol/wasm/chain_merge_ready/src/lib.rs; os-apps/paw-patrol/specs/effort.ioa.toml; os-apps/paw-patrol/policies/patrol.cedar.

## Keep contributor pull requests in their original repository

**Decision:** Run privileged SDLC validation from the trusted base repository, inspecting the contribution as Git objects and PR data, and report each gate against the actual contributor commit.

**Came up because:** GitHub withholds STACK_TOKEN from fork pull_request workflows, but our required gates clone private arni-labs/stack. Rehosting Nick's work avoided that restriction without fixing the shared failure.

**Options:** Rehost contributor branches; remove the token while retaining the private clone; publish private Stack code; separate trusted gate execution from unprivileged contribution builds.

**Chose trusted gate execution because:** It preserves Nick's original PRs and keeps Stack private. The workflow must never check out or execute the contributor's code with repository secrets. Explicit checks remain tied to the PR head because pull_request_target itself runs against the base. Merge remains owned by the authorized Temper effort; this privileged validator does not auto-merge.

**Where:** Stack gates/sdlc.yml and check-effort-artifacts.py; Temper .github/workflows/sdlc-*.yml; original Temper PRs 411 and 412.

## Submit evidence through the validator boundary

**Decision:** Permit registered harnesses to record actual review panels and submit raw proof JSON through SubmitProof, while keeping IngestProof and IngestRecord internal.

**Came up because:** The installed policies excluded the harness class from review submission and ordinary proof callers had no public path into the existing validator.

**Options:** Grant the internal callbacks to agents; keep issuing per-session exceptions; expose the already validated proof input and repair the existing panel permit.

**Chose the validated public input because:** It removes the recurring recording dead end without allowing callers to declare invalid evidence Recorded. The packaged module reuses the current proof validation rules and preserves the submitted commit and evidence fields. Invalid input remains unrecorded.

**Where:** patrol.cedar; proof_packet.ioa.toml; record_ingest/src/lib.rs; paw_patrol_foundation.rs.
