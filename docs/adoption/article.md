# Give AI a reason to come back to the source

*Draft for review. The proposed pilot below has not run; this article reports no adoption or revenue results.*

Imagine an AI agent helping a developer fix an integration. It can read the public documentation. It can inspect the code. It can suggest a plausible patch.

What it lacks is a short note from the engineer who encountered the same failure yesterday: the example configuration is outdated, a particular version changed the retry behavior, and the workaround has a hidden cost.

That note might save hours. It may never become a paper, a public repository, or a training example. Someone still needs a reason to write it, keep it accurate, and make it available where an agent can find it.

This is the problem Nodalync should earn the right to solve.

## Better evidence needs an economic reason to exist

We often talk about what AI can produce. We should also ask what will make people continue producing the knowledge AI needs.

Useful knowledge is work. Someone investigates a failure, tests an explanation, records a decision, or updates an old recommendation. When agents use that work, an attribution link can identify the contributor. It does not pay for the next correction.

Nodalync explores a different arrangement: discover knowledge, pay for access, and preserve declared attribution when new work builds on it. A later paid query can compensate the sources behind the synthesis, not just its immediate publisher.

If this works, the benefit reaches beyond compensation. An application can afford to seek evidence that improves a task. An expert has a reason to keep that evidence useful. The agent gets access to information that might otherwise remain in someone's private notes.

That is the hypothesis. It needs a real workload, not a promise of inevitable royalties.

## What the protocol does

Nodalync has a Rust implementation with local storage, peer-to-peer discovery, an MCP interface for agents, and Hedera settlement integration.

Its knowledge model starts with source material, called L0. L1 contains extracted mentions that help describe it. L2 is a private graph used to organize understanding. L3 contains an insight with links to its declared sources.

For a participating paid query, the current economic rule assigns 5% to the content owner and distributes the remaining pool among declared foundational contributors by weight. An owner who also contributed sources participates in that pool. Integer rounding is handled by the implementation.

The important idea is continuity: when people build on useful knowledge, attribution should travel with it. A new synthesis should not make the upstream contributors disappear.

But we need to be precise about what that means. The current retrieval path returns content bytes and caches them. A content hash can verify those bytes; it cannot prove that a claim is true or prevent a recipient from copying it. Provenance records declared contribution. It cannot detect every omitted source or guarantee payment for every use outside the protocol.

Those limits shape what is worth building. Freshness, accountable authors, corrections, and access to useful evidence must give applications a reason to return.

## Builders need a product people want

The first successful application may look ordinary: an engineering assistant that solves a difficult integration faster because it can obtain a maintainer's current explanation.

The customer buys the completed task. The application buys useful source access and pays its other operating costs. It can charge for the service it provides. Contributors earn when their knowledge is purchased and when participating downstream transactions attribute value to them.

There is no requirement that the customer understand provenance graphs or run a peer-to-peer node. There is a requirement that the total price make sense.

The same discipline applies to creators. Publishing thousands of files is not evidence of value. Adding more owned sources to a synthesis can increase an owner's share without making the answer better. The 95/5 rule needs honest, useful attribution and scrutiny of how weights are assigned; arithmetic alone cannot identify valuable contribution.

Nodalync should compete on the quality of the work it makes possible and the credibility of its accounting.

## Start with one task we can check

Here is a concrete proposed pilot: two application teams, a few experts, and 20 real software integration problems.

Run each problem with public documentation. Run it again with conventional retrieval over the experts' selected notes. Then run it through Nodalync using the same notes, model, and task criteria.

The first comparison tells us whether the evidence matters. The second tells us what Nodalync adds and what its commerce layer costs. Keep the failed attempts. Count inference, hosting, maintenance, and payment costs. Check whether citations support the answer. Trace authorized spending through to recipient balances in the payment exercises.

Then ask the teams to return with another batch of work. Voluntary repeated use is a better signal than a demonstration arranged to succeed.

The experiment may fail. Perhaps the information is already available. Perhaps selecting evidence takes too long. Perhaps a single purchase and a cache satisfy most demand. Each result would tell us something worth knowing before building a larger marketplace.

## Try the mechanism, help define the task

Developers can begin with a controlled test in the public repository:

```bash
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol
cargo test --locked -p nodalync-ops --test e2e_economic_loop test_e2e_multihop_provenance_distribution -- --exact
```

This uses mock settlement. It demonstrates the test's distribution behavior, not live payouts or production security. The implementation still has payment, provenance, and settlement hardening work to complete before real-value use.

If you build an agent product, bring a task where missing expert context caused an expensive failure. If you maintain valuable technical knowledge, bring a small example you are willing to share under explicit terms. We can evaluate whether connecting those two needs produces something worth paying for.

The [pilot protocol](pilot.md) describes the comparisons and decision rules. The [technical review](review.md) explains the implementation and unresolved questions. Open a [repository issue](https://github.com/gdgiangi/nodalync-protocol/issues/new) with the task, today's workaround, and what a successful result would look like. Do not include confidential source material in a public issue.

AI's next useful advance may depend on giving someone a reason to share the one thing the model does not know—and a reason to keep it correct tomorrow.
