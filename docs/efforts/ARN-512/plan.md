# ARN-512 plan (temperpaw side)

1. Bump both kernel pins to `90548bd1`; move only the kernel crates in
   `Cargo.lock`.
2. Build temperpaw; run the dsf-twin contract and runtime tests.
3. Records in Temper for the head (ReviewRuns, ProofPacket), gates, merge.
4. Docker image for the merge commit; `railway-redeploy` with that tag;
   `/healthz` 200.
5. Reload the DSF schema (`/api/specs/load-inline`); production `$metadata`
   has `Temper.Twin`; open the Twins page and see "Deep Sci-Fi".
