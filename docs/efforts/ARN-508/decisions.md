# Decisions

## D1: Set the source image; do not template it

**Decision:** The workflow sets `source.image` to a literal tag via `serviceInstanceUpdate`.

**Came up because:** The alternative is to make the service source `ghcr.io/nerdsane/temperpaw:${{IMAGE_TAG}}` so the existing redeploy re-resolves the variable.

**Options:** Set the literal source from the workflow; template the source on the variable; keep upserting the variable.

**Chose the literal source because:** It matches how the service is already configured and how it was fixed by hand, and the running image is then readable from Railway's own record, which is what the assertion needs. A templated source hides the resolved tag behind a variable and makes "what is running" a two-step question again.

**Where:** .github/workflows/railway-redeploy.yml, "Set the service image".

## D2: Gate on Railway's deployment record, then /healthz; report /readyz

**Decision:** Success means a new deployment of the requested image reached `SUCCESS` and `/healthz` answers 200. `/readyz` is printed and warns on non-200; it does not fail the run.

**Came up because:** `/readyz` returns 503 whenever an optional integration is degraded. An expired Discord token made it 503 for days, and every workflow run failed at that step while the deploys themselves were fine.

**Options:** Keep gating on `/readyz`; gate on `/healthz` only; gate on Railway's record plus `/healthz`, report `/readyz`.

**Chose the last because:** Railway's record answers the question the workflow exists to answer, "is the requested image running", and `/healthz` proves the process behind it serves. Whether a degraded Discord connection should make the service not-ready is an application decision, not a deploy gate's; the warning keeps it visible. Keeping `/readyz` as a gate was rejected because it makes the workflow report failure for reasons unrelated to the deploy.

**Where:** same file, "Wait for a new deployment" and "Verify the process serves".

## D3: A missing version sha warns only for an immutable tag; a wrong one fails

**Decision:** When `expected_sha` is given: a sha that names a different build fails, whatever the tag. No usable answer - a non-200 (the endpoint is 503 today, ARN-508) or a 200 without a sha - warns for a `sha-*` tag, because the tag's hex is checked against `expected_sha` up front and Railway has confirmed that exact image deployed, so the proof already exists; it fails for `edge`/`latest`, because a mutable tag proves nothing about the build it resolved to. Revised twice: round three made a rejected call fail unconditionally, and the first live run with `expected_sha` then failed a successful deploy on a 503 from the version endpoint.

**Came up because:** `/paw/version` currently returns an empty body, so a required version match could never pass.

**Options:** Require the match; drop the check; warn on empty, fail on mismatch.

**Chose warn-on-empty because:** The image assertion already proves which build is running, so the version check is corroboration. An empty answer is a defect in the endpoint (filed on ARN-508), not evidence of a wrong deploy; a wrong answer is.

**Where:** same file, "Verify the process serves".

## D4: Deploy explicitly after setting the source; no wait-then-fallback

**Decision:** Call `serviceInstanceDeployV2` immediately after `serviceInstanceUpdate`. The earlier "wait six polls, then trigger once" fallback is removed.

**Came up because:** The first live run of the rewritten workflow showed the instance sitting for six polls after the source update with no new deployment and a null `latestDeployment.status`, until the fallback fired. Railway stages a source change; it does not deploy it.

**Options:** Keep the fallback (works, wastes a minute, and its comment claimed the opposite of what happens); always deploy explicitly.

**Chose the explicit call because:** It is the only path that deploys, so calling it a fallback was a false description of the mechanism, and the workflow should not carry a comment its own first run contradicted. The poll loop still asserts a deployment newer than the recorded one, of the requested image, in `SUCCESS`, so a double deploy would be harmless and a missing one still fails closed.

**Where:** .github/workflows/railway-redeploy.yml, "Start the deployment"; run 34759013602 is the evidence.
