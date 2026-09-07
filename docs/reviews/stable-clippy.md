# Dev compatibility with Rust 1.98.0

## Baseline

This change targets `dev` at `47ee86be4ae6e4ed10add2834f2779ef1ecd2d41`.
That baseline passes `cargo +1.98.0 check --workspace --locked` and formatting,
but its strict Clippy checks fail. A full all-feature workspace test run on
macOS arm64 reports 1,271 passed, one failed, and three ignored. The sole failure
is `entity_extraction::tests::test_entity_from_node_path`, whose hardcoded
Windows paths are not parsed into path components on macOS or Linux.

The original version of this repair targeted old `main` (`db60d90`). Its
validation results do not describe the newer crates and features on `dev`.

## Compatibility repairs

The existing redundant comparison closures use equivalent `sort_by_key` calls,
including the additional graph neighbor sort on `dev`. Hashes and peer
identifiers retain ascending byte order; topic frequency, earnings, and graph
source counts retain descending order through `std::cmp::Reverse`. Both sorting
APIs are stable, preserving equal-key order.

The topic revenue average uses `checked_div(...).unwrap_or(0)` instead of a
separate nonzero check, preserving the zero-message result. Store error tests
use `std::io::Error::other`. The graph path test constructs its fixture with
`Path::join`, preserving the same node category and filename on every platform.

Checking all targets also revealed two stale benchmark `SearchPayload`
initializers that lacked the forwarding fields introduced on `dev`. They now
explicitly use zero hops and an empty visited-peer list. Benchmark-only unused
imports, variables, casts, borrows, closures, and intentionally discarded
results are cleaned up to satisfy warning-denying builds.

## Reproduction and validation

Use the explicit toolchain without changing the user's default:

```sh
rustup toolchain install 1.98.0 --profile minimal --component clippy --component rustfmt
cargo +1.98.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo +1.98.0 clippy --locked --workspace -- -D warnings
cargo +1.98.0 test --locked --workspace --all-features
cargo +1.98.0 fmt --all --check
git diff --check
```

The strict Clippy checks pass on macOS arm64. The full all-feature workspace
suite passes: 1,272 tests passed, none failed, and three existing documentation
examples were ignored. Formatting and whitespace checks pass. Benchmarks are
compiled and linted; they are not timed.
The checks do not exercise real-value settlement, and GitHub Actions remains
paused at repository level during branch recovery.
