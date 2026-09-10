# Payment authentication review

## AUTH-001 — Nonzero query payments could bypass authentication

**Fixed in this branch.** Payment validation accepted a missing payer public key,
did not bind the signed channel ID to the channel being debited, and the query
handler skipped validation entirely for positive payments on price-zero content.
The handler now authenticates every nonzero payment before changing balances,
nonces, economics, or settlement. Full validation requires a key that identifies
the payer and a valid signature for the selected channel. Structural-only
validation remains explicitly available as `validate_payment_basic`.

Regression tests cover missing, unset, and mismatched keys; invalid signatures;
cross-channel signatures; price-zero payments; unchanged state on rejection; and
successful authenticated payments with mock settlement. Existing paid-query
fixtures now register payer keys and sign their payments.

## AUTH-002 — Authenticated peer-key discovery is missing

**Open; merge/deployment prerequisite for public paid-query adoption.** Production
code currently does not populate the peer store with Nodalync public keys.
`NetworkNode::register_peer_mapping` uses an all-zero placeholder, and Identify
processing only imports addresses. libp2p transport and Nodalync signing identities
can differ. After AUTH-001, fresh paid peers without registered keys are rejected
instead of silently trusted; genuine zero-payment queries continue to work.

Define and implement an authenticated binding between the two identities and
persist verified Nodalync keys. Validate two fresh nodes completing an
authenticated paid query without manually seeding their databases. Do not infer a
Nodalync signing key from an unrelated libp2p identity.

## AUTH-003 — Payment authorization and settlement need a broader security design

**Open.** The legacy payment signature covers channel ID, amount, recipient, query
hash, and timestamp. Payment ID, provenance, and the separately transmitted query
nonce are outside that signature. The generic inbound message handler also
permits unknown sender keys during bootstrap. Payment-channel close/dispute
contract methods check signature length rather than cryptographically verifying
the counterparty's authorization.

These limitations are not fixed by AUTH-001. A coordinated, versioned signing and
identity-binding design is required before claiming production payment safety.
Settlement failures also leave channel balances and nonces advanced by design,
while clients advance only after success; recovery and idempotency require an
explicit design that handles uncertain on-chain outcomes and crashes.

## SETTLE-001 — Adapter/contract compatibility and funding reconciliation

**Open; verify before relying on settlement receipts.** The Rust Hedera adapter
encodes settlement entries using `encode_settlement_entry_evm`: an EVM address,
a 256-bit amount, and provenance hashes. The checked-in Solidity contract's
`_decodeEntry` instead reads the older shard/realm/account-number layout with a
64-bit amount. A deployed contract may differ; compare its verified source and
bytecode with the adapter and add cross-language encoding fixtures before making
compatibility claims. The contract also emits the supplied Merkle root without
checking that it commits to the entries.

`settleBatch` debits the transaction sender's deposit, while the query handler
submits settlement as the content provider. Channel escrow and recipient deposits
therefore need an explicit reconciliation test across requester, provider, and
upstream creators. In particular, an L0 batch can credit its sender before
deducting the same amount; transaction success alone does not establish that the
requester's funds paid for the query. This review has not verified deployed
contract behavior and does not change settlement accounting.
