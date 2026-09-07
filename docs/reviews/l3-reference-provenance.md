# Preserve attribution when an L3 becomes a foundation

Status: draft implementation proposal. This change needs a compatibility decision
before release. It adds local SQLite schema version 5 and implements the optional
L3 creator root described in specification §7.1.6; older validators reject that
root extension even though no wire fields are added.

## Problem and observed behavior

`reference_l3_as_l0` previously built a new L0 manifest with the importer's
identity and the same hash as the source L3. However, the required source manifest
already exists, and SQLite manifest storage uses `INSERT OR IGNORE`. The original
manifest is therefore preserved in the reference implementation. The operation
does **not** overwrite its provenance in SQLite: it is effectively a no-op.

After Alice publishes an original observation and Bob derives an insight from
it, Carol can reference Bob's L3 and build a new insight. Previously, Carol's
derivative preserved Alice's root but did not give Bob the additional foundational
entry promised by §7.1.6. The existing test checked only that the returned hash
was unchanged, so it missed the absent economic effect.

## Proposed behavior

Import stores a local `(hash, original owner, imported_at)` record, leaving the
source's bytes, manifest, owner, visibility, type and derivation history unchanged.
It does not copy cached response bytes into owned content storage. Repeated
imports keep the first timestamp and do not accumulate revenue weight.

When creating a derivative, an explicitly imported direct L3 contributes:

1. All its existing foundational entries, with their existing weights.
2. One additional entry for the L3's original creator, keyed by that L3's hash.

An ordinary L3 source used without import keeps the existing behavior. A later
import of Carol's new L3 can add Carol while preserving Alice and Bob. Import
choices persist after restart, and importing one hash does not automatically
import its later versions.

The validator accepts an optional creator entry only for a direct L3 source,
bound to that source's hash and owner. A new root records the source's current
visibility; a root already inherited through another path preserves its earlier
visibility snapshot, matching the existing merge behavior. Inherited roots must still
match in full, including payment recipient and weight. The validator also handles
a creator already present through another derivation path without losing that
path's weight. Both repeated direct source occurrences and distinct derivation
paths retain the existing weight policy. Each direct occurrence adds the same
one-unit imported creator contribution; repeating the import operation itself
does not add weight.

Remote source metadata alone is insufficient to import or derive: the caller
must own the source or have it in the local query cache on first import. The
owner-bound reference then records prior access even after normal cache eviction.
This uses the existing
local access model; it does not introduce a new cryptographic proof of paid
access or authenticate remote manifests independently.

## Migration and compatibility decisions

- Opening an existing version 4 database creates the `l3_references` table and
  advances the local schema to version 5. Existing manifests, derivation edges,
  and settlement rows are preserved, including version 4's settlement uniqueness
  constraint. Older databases still run the version 4 settlement deduplication
  migration before adding references. Fresh databases create both the reference
  table and unique settlement index immediately.
- Previous import calls left no durable import marker, so their intent cannot
  be recovered automatically. They must be repeated to opt into the behavior.
  Existing derivatives and their payment allocations are not rewritten.
- Specification §7.1.6 allows the imported L3 creator to become a root, while
  §4.5/§9.3 generally describe roots as L0/L1 only. This proposal interprets the
  imported L3 as a foundational reference without falsifying its source type.
  Maintainers should reconcile that text and choose a version/rollout policy.
- Older binaries ignore local reference choices and older validators reject new
  derivatives containing the optional L3 root. Before release, decide how peers
  advertise support and how operators upgrade. Retaining old fields alone does
  not make this behavior interoperable with the old validation rules.
- Keep a database backup before deploying an experimental schema. Removing the
  local import table would not undo already-created derivative manifests or
  their economic meaning. No production database was opened during this work.

## Validation

Regression tests cover the original manifest and ancestry remaining unchanged,
repeated imports, persistence after reopening the node, missing/unqueried remote
sources, repeated direct source weighting, cache eviction, visibility changes,
two generations of import and synthesis,
and conserved allocations to the original author, intermediate synthesizers and
new author. The payout tests use the existing distribution function and current
source-weight behavior; they do not assert live network settlement.

Validator tests reject altered creator recipients, visibility, inflated weight,
unrelated entries, omitted upstream roots and redirected upstream payments.
Another test combines direct and inherited paths to the same creator. Migration
tests cover the version 3 settlement deduplication followed by reference creation,
and version 4 upgrades that preserve an existing manifest, edge, queued and settled
payments, and settlement uniqueness. Repeated initialization is also verified.

The affected crates are tested with both default and all enabled features:

```bash
cargo test -p nodalync-store -p nodalync-valid -p nodalync-ops
cargo test -p nodalync-store -p nodalync-valid -p nodalync-ops --all-features
```

## Remaining economic questions

This change restores an explicit attribution choice; it cannot prove that every
source was disclosed or that a contribution is useful or original. Provenance
stuffing, repeated low-value derivation paths, downstream pricing, and rounding
dust remain economic design questions. Existing direct-L0 double counting is
unchanged to keep this proposal separate from a redistribution policy change.
Creator authority, payment-recipient binding at every network boundary, and
license terms also require their own reviews. A successful import payout test
does not establish market demand or guaranteed future royalties.
