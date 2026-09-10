# ARN-461 — Temper is the review and proof door

Rita needs one book. Panel output lands on ReviewRuns and a ProofPacket. Merge refuses unless those rows are Recorded and pass the same rules as `validate.py` (panel, no open act-on, commit pin, proof reruns). The implementer does not set those bools by declaration. CI asks Temper, or goes away for a walked Effort. No PR-body copy of `decisions.md`. No hidden comment.

WASM inspects sibling rows the same way `chain_github_ready` inspects a git path. `on_success` / `on_failure` set or retract a bool. Do not extend `review_gate_lifecycle` (it already dispatches transitions). One integration, one concern.

L1 still cannot require `panel_count >= 3`. The WASM can require the panel; the guard stays the bool that WASM set.

Linear: https://linear.app/arni-build/issue/ARN-461
Temper Intent: `en-01a069b4-02c5-7753-a9f3-148d59357f7b`

## September 10 repair

Rita authorized fixing the confirmed gate failures: harness review/proof submission, evidence files, approval delivery and authenticated arbitration, plus shared helper/CI agreement. Existing validation, exact commit provenance and the three-round human decision boundary remain required. Fork PRs 411/412 were rehosted unchanged as 459/460; no secret exposure to forks is needed.
