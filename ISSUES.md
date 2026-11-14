# Issues

Open issues found during the v1 hardening sprint. Each entry says where the problem
is, how it shows, and what a fix needs.

## Open

### Faucet error responses are reported as success

- **Where:** `crates/protocol/nodalync-settle/src/faucet.rs`, `FaucetResponse` / `parse_faucet_body`
- **What happens:** `FaucetResponse` is `#[serde(untagged)]` and its first variant,
  `Success`, has only optional fields. A faucet error body such as
  `{"error": "rate limited"}` therefore deserializes as `Success`, and `request_hbar`
  reports a 100 HBAR success instead of the faucet's error.
- **Evidence:** the test `test_parse_faucet_body_error_object_current_behavior`
  records the current (wrong) behavior.
- **Fix:** try the `Error` variant first (or tag the enum) and flip that test to
  expect an error. Kept out of the complexity refactor so that change stays
  behavior-neutral.

### `build-l2` and `merge-l2` cannot read the graph they just built

- **Where:** `crates/apps/nodalync-cli/src/context.rs`, `load_entity_graph`
  (used by `commands/content/build_l2.rs` and `commands/content/merge_l2.rs`)
- **What happens:** `nodalync-ops` stores L2 entity graphs as JSON
  (`serde_json::to_vec` in `l2.rs`), but the CLI decodes the stored content with
  `ciborium` (CBOR). After a successful build or merge, the command fails with
  "Failed to parse entity graph" / "Failed to parse merged graph".
- **Evidence:** no CLI test covers a successful build or merge; existing tests only
  cover the error paths. The ops-level golden test
  `test_build_and_merge_l2_content_is_unchanged` confirms the graphs are stored as JSON.
- **Fix:** decode with `serde_json::from_slice` in `load_entity_graph` and add a
  CLI test that builds an L2 from an owned L1. Needs a way to create L1 content
  from the CLI, which may need a design decision (escalate to Gabe).

### Deploy script prints a stale crate path

- **Where:** `contracts/scripts/deploy-hedera-sdk.js`, `printNextSteps`
- **What happens:** step 4 tells the user to `cd ../crates/nodalync-settle`;
  the crate now lives at `crates/protocol/nodalync-settle`.
- **Fix:** update the printed path.
