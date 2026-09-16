# Implementation

1. Reproduce the missing permit with the real AuthzEngine and canonical Cedar.
2. Verify installed Genesis source and pin; add only the verified operator Ask.Answer permit there and in the GitHub mirror.
3. Verify positive and negative cases, including original policy red and new policy green.
4. Review the full correction, merge the mirror, publish the exact Genesis delta, install the pinned app, and verify through Foundry in Arc.

Catalog read currently returns403. No operator fallback or installation proceeds without the pending scoped approval.
