# CI compatibility with Rust 1.98.0

## CI-001: Stable Clippy rejects redundant comparison closures (fixed)

The CI workflow installs the current stable Rust toolchain and denies Clippy
warnings. Rust 1.98.0 reports `clippy::unnecessary_sort_by` for nine existing
comparison closures in the types, economics, operations, and CLI crates. The
first three diagnostics in `nodalync-types` stop the normal CI check before it
can report the remaining sites.

Replace those closures with equivalent `sort_by_key` calls. Hashes and peer identifiers
retain their ascending byte order; topic frequency and earnings retain their
descending order through `std::cmp::Reverse`. Both sorting APIs are stable, so
equal-key ordering is preserved. No economic rules, serialized formats,
dependencies, lint policies, or CI commands change.

## Reproduction and validation

Install the explicit toolchain without changing the user's default:

```sh
rustup toolchain install 1.98.0 --profile minimal --component clippy --component rustfmt
cargo +1.98.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Against baseline `db60d90`, that command fails on the three types-crate sorts.
Running once with `-W warnings` inventories all nine sites; no other Clippy
warnings were reported. After the changes, the strict command passes.

Additional validation on macOS arm64 with Rust 1.98.0:

```sh
cargo +1.98.0 clippy --locked --workspace -- -D warnings
cargo +1.98.0 test --locked -p nodalync-types -p nodalync-econ -p nodalync-ops -p nodalync-cli
cargo +1.98.0 fmt --all --check
git diff --check
```

The existing affected-package test suites pass: 566 tests passed and two existing
documentation examples were ignored. No new tests are needed for these
mechanical substitutions. Linux CI remains the cross-platform verification;
these checks do not exercise real-value settlement.
