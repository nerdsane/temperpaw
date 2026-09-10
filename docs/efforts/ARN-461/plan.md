# ARN-461 plan

What we are addressing: Effort.Merge believed implementer bools. CI believed a hidden PR comment. Those were two books, and neither was the rows.

Expected end state: PassReview / AttachProofPacket / Merge retract unless Temper rows pass the same rules as validate.py. CI asks those rows by commit. No hidden comment.

1. Add `chain_review_ready`, `chain_proof_ready`, `chain_merge_ready` (host-tested pure checks + GET). Register in `app.toml`, `build.sh`, Cedar `http_call`.
2. Wire the three Effort actions and retract callbacks.
3. Let `RecordPanel` write `commit` / `reviewers_ran` / `risk` so one write can land a real record.
4. Point CI at Temper by commit. Drop the hidden `<!-- sdlc-review -->` comment.
5. Foundation tests for the spec/WASM/CI contract. Compile the new wasm blobs.

## September 10 repair plan

What we are addressing: completed work cannot cross the recording and merge gates because the public submission contract, authorization policies, approval transport and CI disagree.

Expected end state: a normal harness records genuine evidence, an authentic human decision is enforced at its approved scope, and the affected authorized PRs can merge and install with verified results.

1. Reproduce harness denial and validated callback boundaries against the deployed kernel contract; add regression coverage before changing them.
2. Repair the public recording and file submission path with existing app/kernel primitives. Preserve validator-owned callbacks.
3. Repair shared gate/helper arbitration and record-store agreement, and diagnose/fix the actual MCP approval delivery condition.
4. Run relevant local end-to-end cases, the fixed review panel and required confirmation; publish/install through the governed release path and re-drive the originally blocked records.

Keep one PR per affected repository. Broader policy garbage collection and unrelated platform changes are outside this repair.
