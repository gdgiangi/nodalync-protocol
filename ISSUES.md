# Adoption review issues

Review baseline: `db60d90805f705149d24fff2559a26a97b37ee2d` (7 September 2026).

This file records review findings, not GitHub issue numbers. Details, source links, and acceptance gates are in [the review](docs/adoption/review.md).

| ID | Priority | Status | Finding / acceptance condition |
|---|---|---|---|
| ADOPT-01 | P0 | Open | Contract channel close/dispute paths check signature length rather than authenticate state; require a versioned identity/signature design and hostile-party tests before real-value use. |
| ADOPT-02 | P0 | Draft fix; integration blocked | Nonzero query payments must require a payer key, valid signature, and matching channel before state mutation, including payments attached to free content. Authenticated production key discovery is a merge prerequisite. |
| ADOPT-03 | P1 | Companion fix proposed | Apply the documented default MCP allowance to omitted per-query limits and resource reads. Operator funding authorization remains a separate open design. |
| ADOPT-04 | P1 | Open | L3 reference operation is ineffective under INSERT OR IGNORE; downstream derivations need durable import semantics that include the original L3 creator and upstream roots. |
| ADOPT-05 | P1 | Open | Validate authenticated manifest/provenance recipient metadata; unchanged content bytes must not permit substituted attribution. |
| ADOPT-06 | P1 | Open | Bind full payment intent, nonce, quote, recipients, and retry identity; reconcile uncertain delivery/settlement outcomes. |
| ADOPT-07 | P1 | Open | Adversarially evaluate owned-root padding, import wrappers, cached substitutes, and finite demand; 95/5 alone does not establish fair contribution. |
| ADOPT-08 | P2 | Open | Link runnable simulator, configs, and raw data; current report's reproducibility claim cannot be verified from this tree. |
| ADOPT-09 | P1 | Proposal ready | Run the matched-task pilot and publish quality, total cost, repeat use, and independently reconciled payout evidence before claiming adoption. |
| ADOPT-10 | P0 | Open | Resolve adapter/contract settlement-entry encoding mismatch and demonstrate buyer-funding-to-recipient conservation against the actual deployed contract, not only mocks. |
| ADOPT-11 | P1 | Open; CI blocked | CI reports 14 vulnerability findings against the baseline lockfile. Assess reachable features, upgrade affected dependency families, and rerun audit/integration tests; do not blanket-suppress advisories. |

## Reviewable changes

- [Adoption review, pilot, and article — PR #45](https://github.com/gdgiangi/nodalync-protocol/pull/45)
- [Nonzero payment authentication — draft PR #46](https://github.com/gdgiangi/nodalync-protocol/pull/46); authenticated peer-key discovery remains a prerequisite.
