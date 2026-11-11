# Baseline and report files

`schemas/baseline.schema.json` (JSON Schema draft-07) describes the files; the test suite validates real
output against it.

- JSON with sorted keys, 2-space indent, trailing newline, **no timestamps and no absolute paths**.
- `entries` are sorted by `key` (`scenario/step`, or `scenario/step#N` for repeats). Duplicate keys are an error.
- `environment` records `tool_version`, `soroban_sdk_version`, `soroban_env_host_version` and
  `protocol_version`. SDK, host and protocol must match to compare two files (the tool version is informational).
- `contracts` records each contract's `wasm_sha256` and `wasm_size_bytes`.
- Each entry has nine `metrics` (`instructions`, `mem_bytes`, `disk_read_entries`, `memory_read_entries`,
  `write_entries`, `disk_read_bytes`, `write_bytes`, `events_bytes`, `fee_stroops`) and `fee_components`
  (the pieces of `fee_stroops` as reported by the SDK).
- Run reports (`run`, `check`) use the same shape plus a `wasm` object with the static report per contract.
  Baselines never contain it.
- Files are written atomically (temp file, then rename).

## Diff output

`--format json` prints a document with its own `schema_version` (currently 1): one row per key with a status
(`regressed`, `improved`, `unchanged`, `new`, `removed`, `environment-drift`), per-metric deltas and percent
changes, the verdict and any Wasm size findings.

A metric fails only when it exceeds both its `_pct` and `_abs` thresholds (when only one is configured, that
one decides). A baseline of 0 counts as an infinite percent increase. `new` and `removed` entries never fail
unless `--fail-on any-change`. `environment-drift` marks rows whose Wasm hash is unchanged but whose numbers
moved; such rows still count as regressions when they exceed thresholds.
