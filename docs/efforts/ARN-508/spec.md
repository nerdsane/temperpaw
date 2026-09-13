# Spec

1. Record the live deployment before touching anything.
2. Set the service instance's `source.image` to the requested tag
   (`serviceInstanceUpdate`). Railway starts a deployment on a source change;
   if none appears within six polls, start one explicitly, once.
3. Gate on Railway's own record: a deployment newer than the recorded one,
   of the requested image, in status `SUCCESS`. A `FAILED`/`CRASHED`
   deployment fails the run immediately.
4. Then prove the process answers: `/healthz` 200 gates. `/readyz` is reported
   and warns if not 200 but does not gate, because it folds optional
   integrations into the answer. `/paw/version` is checked only when an
   expected sha is given, and a missing sha warns rather than fails, since
   the image assertion is the proof.

Out of scope: rotating the Discord credential; deciding whether a degraded
optional integration should make `/readyz` 503; making `/paw/version`
return a sha; the `temperpaw.katagami.ai` DNS.
