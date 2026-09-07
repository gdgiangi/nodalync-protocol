# Pilot: does maintained evidence improve an agent's work?

**Proposed experiment, not reported results.** Use local/mock execution first and testnet only for network-payment exercises until the settlement and authorization gates in the [review](review.md) are resolved. A testnet transfer is evidence of integration, not revenue or willingness to pay.

## Recruit a problem, then a corpus

Choose one software integration workflow with reproducible success criteria. Ask two application teams to supply 20 recent, representative tasks before choosing documents. Exclude secrets, personal data, and material contributors cannot authorize for the pilot. Record exclusions and failed tasks rather than selecting only favorable examples.

Ask three to five experts for a small set of relevant, maintained notes. Record author identity, version/date, scope, permitted use, price, and the exact preview each expert approves. Freeze a corpus snapshot for evaluation. Keep some tasks held out from anyone tuning retrieval or prompts. A later version can evaluate freshness by introducing a documented correction or software change.

People can help operate the test, but log the assistance. A founder manually choosing every source is not an autonomous discovery result. Do not create accounts, contact recruits, or publish their documents as part of this proposal; those are separate activities requiring their participation.

## Compare the right alternatives

Run the same tasks with the same model/version, task prompt, tool budget, and scoring rubric. Randomize condition order and use fresh contexts to prevent information leaking between runs.

| Condition | Access | What it measures |
|---|---|---|
| Public baseline | Public documentation/search available to the intended product | What buyers can already obtain. |
| Matched corpus | Conventional retrieval over the exact expert corpus, with the same usage permission | The value of the expert evidence without Nodalync's commerce layer. |
| Nodalync | The same corpus accessed through discovery and priced content reads | Integration cost, attribution, accounting, payment friction, and task performance under budget. |

An improvement over public search can establish corpus value. An improvement over nothing cannot establish protocol value. The matched-corpus condition makes that distinction visible: Nodalync should preserve quality while adding acceptable overhead and reliable contributor accounting. Model-only runs can be an additional diagnostic, but are not the main commercial comparator.

Repeat stochastic runs under declared settings where affordable. Have evaluators score artifacts without knowing the condition. Use executable checks when available; otherwise define a concrete task rubric in advance. Publish per-task paired differences, failure counts, and uncertainty instead of only aggregate averages. Twenty tasks are a screening exercise, not proof of general superiority.

## Keep an evidence ledger

For each attempt, retain these fields in an exportable ledger. This is an experiment schema proposal, not an existing protocol API.

| Group | Required fields |
|---|---|
| Experiment | Task ID; condition; run ID; corpus snapshot; model/version; prompt/config digest; timestamp; assistance supplied. |
| Outcome | Success/failure and rubric score; evaluator; artifact/check output; unsupported claims; citations that actually support those claims; abstention reason. |
| Retrieval | Selected source hashes/versions; discovery candidates; request/response identifier; cache hit/miss; bytes returned; attribution weights and recipients. |
| Authorization | Operator session/per-call limits; any authorized override; quote/terms reference; exact amount in integer currency units; rejected requests. |
| Payment | Payment/receipt identifier; estimated, reserved, charged, pending, settled, failed, or uncertain state; chain/network; transaction ID; independently verified recipient amounts. |
| Costs | Source access; inference; hosting; transaction fees; support; refunds; deposits/locked channel funds recorded separately from expenses. |
| Timing | Discovery, retrieval, inference, and settlement latency; end-to-end latency; retries and timeout outcome. |

Use `unknown` or a missing-evidence flag where current tooling cannot provide proof. Do not populate a settled amount from a query's advertised price. Do not label a synthetic/local receipt or a mock settlement as an on-chain payout. Preserve failed and uncertain attempts in denominators. Publish redacted ledgers only with participants' permission.

## Agree on the decision rules before running

The following are proposed screening thresholds, chosen to force a decision; participants may revise them before collecting results.

- **Evidence value:** Nodalync improves success by at least three of the 20 paired tasks over public search, with expert-supported explanations of the improvement. Report regressions and uncertainty alongside wins.
- **Protocol overhead:** it loses no more than one successful task relative to matched-corpus retrieval. Median and p95 latency and total cost must fit each application's written service target; record those targets before the pilot.
- **Accounting:** every accepted paid test transaction reconciles to the expected integer-unit distribution, including rounding. No missing payer authorization or double charges after retry. Uncertain transactions remain unresolved, not counted as successes.
- **Human demand:** both application teams return for a second task batch without the founder prompting each query. At least one documents a price and concrete conditions under which it would run a paid follow-up after the safety gates pass. This is an interest signal, not recognized revenue.
- **Supplier value:** contributors can explain their statements, correct or withdraw a listing for future access, and choose whether the observed workload justifies maintenance effort. Record maintenance and support time even if unpaid.

Stop or narrow the pilot if the expert corpus supplies no distinctive value. Investigate discovery/pricing/latency if the matched corpus succeeds but Nodalync fails. If quality succeeds and economics fail, revise the service economics before recruiting a larger network.

## Exercise failure before promotion

Use a disposable local harness for wrong payer keys, changed channel IDs, nonzero payments on free content, imported-L3 derivations, substituted recipient metadata, duplicate requests, stale prices, failed settlement, process restart, and a provider timeout after authorization. Include a document that asks the agent to ignore its budget and spend more. Confirm that client/operator policy governs actions even when source content requests them.

Test a synthesis with many irrelevant owned roots. A correct 95/5 calculation should not be reported as evidence that attribution is fair. Review the citations and measure the independent expert's actual payout. Compare caching and fresh-version reads: continued royalty demand must have a reason to recur after bytes have been obtained.

## A runnable first inspection

From a checkout of this repository, Rust users can inspect the current controlled economic path:

```bash
cargo test --locked -p nodalync-ops --test e2e_economic_loop test_e2e_multihop_provenance_distribution -- --exact
```

This exercises an in-process scenario with **mock settlement**. It does not contact contributors, spend HBAR, establish external demand, test independent network operators, or demonstrate production-safe settlement. The [test source](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-ops/tests/e2e_economic_loop.rs) makes the setup inspectable.

For agent integration, start with the existing [MCP reference](../modules/11-mcp.md). Query inputs are content hashes, so the application must discover/select evidence before retrieval and perform synthesis itself. An independent two-node testnet run, recipient balance reconciliation, and the matched-task experiment are later gates; the command above must not be presented as completing them.

## Publish the result, including a negative result

Report the registered targets, corpus selection process, assisted steps, all per-task outcomes, cost and latency distributions, recipient reconciliation, unresolved failures, and repeat-use observations. Release runnable configurations and consented artifacts. Keep participant-authored findings separate from the protocol team's interpretation.

An unsuccessful pilot still advances the project if it identifies a specific constraint. The useful output is a decision about which knowledge product deserves another iteration.
