# Deviations from the build prompt

1. **Name.** `soroban-meter` is already used by another GitHub project, so this project is `soroban-gauge`:
   crates `gauge-*`, binary `soroban-gauge`, config `soroban-gauge.toml`, baseline dir `.soroban-gauge/`, comment
   marker `<!-- soroban-gauge:v1 -->`. `OWNER` in the README and `Cargo.toml` is a placeholder until the repo exists.
2. **Toolchain.** `rust-toolchain.toml` pins 1.91.0, the MSRV listed for soroban-sdk 29.x. Local checks ran on
   Rust 1.99.1 (conda-forge); CI uses 1.91.0 and is the check of record.
3. **Fixture build.** The SDK refuses to compile unless built by `stellar contract build` or with
   `SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2` set. `stellar-cli` could not be installed in the authoring
   environment, so the fixtures were built with that variable set and `-Zbuild-std` for `wasm32v1-none`
   (`tests/fixtures/build.sh` uses `stellar contract build` when available). Their spec section is therefore not
   shaken. Rebuilding can change hashes and numbers; re-run `snapshot` afterwards.
4. **`soroban-spec-tools` not used.** Its releases track `stellar-cli` (latest 30.x needs Rust 1.93 and a pre-release
   `soroban-spec`), which does not match the pinned SDK 29.0.1. Argument conversion is implemented in
   `gauge-core/src/args.rs` on `stellar-xdr`. `u256`/`i256` are unsupported.
5. **`max_rw_key_bytes` omitted.** `soroban-sdk` 29.0.1 does not expose it.
6. **No `--jobs`.** Scenarios run sequentially.
7. **Test location.** End-to-end tests are in `crates/gauge-cli/tests/cli.rs` (a virtual workspace has no root package).
8. **Additions.** `gauge-core` feature `runner` (so report crates avoid linking the VM); `fee_components` per entry;
   `$alias` arguments; `check/diff --diff-out`; action input `binary-path` (needed so this repo can dogfood before a
   release exists); default step `auth = "all"`; `read_entries_*` thresholds apply to disk and in-memory read entries.
9. **Status semantics.** Non-failing changes are `improved` (some metric fell, none rose) or `unchanged` (identical, or
   only increases within thresholds). `environment-drift` rows that exceed thresholds still count as regressions.
10. **Examples.** `counter-regressed` has no baseline of its own: its config compares against the `counter` baseline
    to demonstrate failure. Example configs point at `tests/fixtures/*.wasm`.
11. **`--color`.** Accepted, no effect (no ANSI is emitted).
12. **`calibrate`.** Not implemented; `gauge-rpc` is a placeholder and the command exits 2.
13. **Release.** `workflow_dispatch` performs the dry run (all five targets, no publish). `cargo publish` is not automated.
14. **`cargo deny`.** Two transitive advisories are ignored with reasons (see `docs/DEPENDENCIES.md`).
15. **Coverage.** The 80% coverage target was not measured (no coverage tool in the authoring environment).
16. **Not executed on GitHub.** The Action, release and examples jobs; see `docs/limitations.md`.
