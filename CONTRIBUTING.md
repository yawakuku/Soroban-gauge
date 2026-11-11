# Contributing

## Setup
```bash
git clone <repo> && cd soroban-gauge
cargo build --workspace
cargo test --workspace
```
The toolchain is pinned in `rust-toolchain.toml`.

## Checks before a PR
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
```

## Commit style
Conventional-style prefixes (`feat:`, `fix:`, `docs:`, `test:`, `ci:`, `chore:`).

## Sign-off (DCO)
Sign every commit with `git commit -s`, certifying the
[Developer Certificate of Origin](https://developercertificate.org/).

## Adding a metric
1. Add the field to `Metrics` in `crates/gauge-core/src/baseline.rs` and to `Metrics::NAMES` and `get`.
2. Read it in `read_resources` (`crates/gauge-core/src/runner.rs`) from what the pinned SDK reports. Do not invent values.
3. Add threshold keys in `config.rs` and map the metric in `limits_for` (`crates/gauge-report/src/engine.rs`).
4. Update `schemas/*.json`, the docs, regenerate example baselines (`soroban-gauge snapshot`) and snapshot tests
   (`INSTA_UPDATE=always cargo test`, then review the diff).

## Adding a renderer
Add a function in `crates/gauge-report/src/render.rs` taking `&DiffReport`, export it from `lib.rs`, add a
`DiffFormat` variant in `crates/gauge-cli/src/main.rs`, handle it in `compare` in `cmds.rs`, and add an `insta` test.
