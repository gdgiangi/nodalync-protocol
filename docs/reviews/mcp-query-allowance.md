# MCP query allowances: findings and validation

## MCP-001: Default query allowance ignored (fixed)

With `--budget 1.0 --auto-approve 0.01`, a `query_knowledge` call without
`budget_hbar` could retrieve content priced at 0.5 HBAR and debit that amount.
`resources/read` for `knowledge://{hash}` bypassed the threshold in the same way.
This contradicted the tool input contract and MCP module documentation.

Both paths now check the default allowance and session ceiling before executing
a query. The tool checks before automatic deposit or channel funding as well.
Explicit query allowances retain their documented ability to replace the default,
while remaining subject to the session ceiling. Invalid startup limits and
explicit query allowances are rejected. Budget failures give budget-specific
recovery guidance instead of suggesting a deposit.

Regression evidence: `test_query_without_explicit_budget_enforces_auto_approve`
and `test_resource_read_enforces_auto_approve` both failed against `db60d90`:
the tool returned success and the resource returned the paid content. They pass
with the fix. Additional cases cover the exact threshold, explicit allowances,
shared session accounting, free content, failed-query refunds, and invalid limits.
These tests use local non-owned content and exercise real RMCP resource dispatch;
they do not prove a live Hedera payment or remote peer exchange.

The refund regression uses a priced remote announcement with no local content
and networking disabled: preview succeeds, then retrieval fails after budget
reservation. Temporarily removing the tool refund or the resource refund causes
the test to fail independently with 1,000,000 tinybars left reserved. Restoring
both refunds makes it pass.

Original main-branch validation: `cargo test -p nodalync-mcp` (67 passed),
`cargo clippy -p nodalync-mcp --all-targets -- -D warnings`,
`cargo fmt -p nodalync-mcp --check`, and `git diff --check`.
The query flow was checked against spec sections 7.2.2 (free preview) and 7.2.3
(paid query). The allowance is MCP application policy; no wire, storage, or
economic rule changes are needed.

## Integration with dev's x402 and resource payment support

The dev integration preserves x402 payment requirements, verification, settlement,
and receipt handling, plus automatic channel creation for resource reads.
Both payment modes check allowances before payment effects. x402 processing
reserves the content price before contacting the facilitator. Definite local or
verification rejections release that reservation. Settlement, network, or internal
errors retain it because they do not prove non-payment. Successful payments stay
counted even if delivery fails, because a settled payment cannot be refunded by
changing local budget accounting.
Application fees remain outside this query-price budget, like network fees.

Added regression coverage uses a local HTTP facilitator, with no real payment,
to check reservation before verification/settlement, successful shared-session
accounting, content-delivery failure after settlement, and malformed/disconnected
settlement responses that retain the reservation and block another payment attempt.
Additional tests check rejection before payment processing and channel creation only after the
allowance passes. The resource fixture allows the existing 1 HBAR channel deposit;
it does not change dev's default channel minimum or funding policy.

Dev integration validation: `cargo test --locked -p nodalync-mcp --lib
--no-default-features` and the corresponding `--all-features` run each passed all
80 tests. `cargo clippy --locked -p nodalync-mcp --all-targets
--no-default-features --no-deps -- -D warnings`, `cargo fmt --all --check`, and
`git diff --check` also passed.

## MCP-002: Wallet funding and approval policy (open design decision)

`query_knowledge` can automatically deposit 10 HBAR and fund a 1 HBAR channel
after a paid query passes the query-price checks. These operations and network
fees are outside `--budget`. Direct deposit/channel tools also operate outside
that budget. An agent-supplied `budget_hbar` is not evidence of human approval.

Define a trusted operator policy for per-query authorization, wallet funding,
and fees before promising bounded autonomous wallet spending. Potential controls
include a non-overridable per-query ceiling, explicit automatic-funding limits,
and host-issued authorization. This requires an explicit policy decision;
MCP-001 restores the existing documented query semantics without selecting one.

## MCP-003: Own-content reads consume the advertised query price (open)

Both MCP retrieval paths reserve the manifest price, but `nodalync-ops`
`query_content` returns a zero-amount receipt for owned content. This can exhaust
the session budget or trigger unnecessary funding while reading one's own paid
publication. Align the MCP charged cost and funding decisions with the actual
query cost, with regression coverage for owned content.
