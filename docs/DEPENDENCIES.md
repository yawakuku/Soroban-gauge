# Dependencies

Pinned where the choice defines the numbers; the rest use semver-compatible ranges (the lockfile is committed).

| Crate | Why |
|---|---|
| `soroban-sdk` `=29.0.1` (feature `testutils`) | The host VM (`Env`), contract registration from Wasm bytes, and `cost_estimate()`. Costs are only meaningful for this exact host, so it is pinned. Only in `gauge-core` behind the `runner` feature. |
| `stellar-xdr` `=28.0.0` | The XDR version `soroban-sdk` 29.0.1 resolves to; used to decode custom sections and build argument values. |
| `wasmparser` `=0.263.0` | Parse Wasm without executing it. |
| `serde`, `serde_json`, `toml` | Config, scenarios, baselines. |
| `sha2`, `hex` | Wasm hashes and deterministic addresses. |
| `glob` | Scenario file patterns. |
| `thiserror` | Library error types. |
| `clap` (derive), `anyhow`, `tracing`, `tracing-subscriber` | CLI only. |
| Dev: `assert_cmd`, `predicates`, `insta`, `tempfile`, `proptest`, `jsonschema` | End-to-end, snapshot and property tests; schema validation. |

`soroban-spec-tools` is not used: see DEVIATIONS. `reqwest`/`tokio` are not used yet (`calibrate` is not implemented).

`cargo deny` ignores two advisories that arrive transitively through the Soroban host and cannot be fixed
here (`RUSTSEC-2026-0387`, `RUSTSEC-2024-0436`); the reasons are in `deny.toml`. Re-check on every SDK bump.
