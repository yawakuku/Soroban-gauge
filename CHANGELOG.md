# Changelog

## Unreleased
- Workspace skeleton, toolchain pin, licenses, CI (fmt, clippy, test, deny, examples).
- `gauge-wasm`: static Wasm inspection and `inspect`.
- `gauge-core`: config, scenarios, typed argument conversion, runner on the real host VM, deterministic baselines.
- `gauge-report`: diff engine, table/markdown/json renderers, property and snapshot tests.
- `gauge-cli`: `init`, `inspect`, `run`, `snapshot`, `diff`, `check`; `calibrate` reports not implemented.
- Examples (`counter`, `counter-regressed`, `token`) with committed baselines and fixtures.
- Composite GitHub Action, dogfood and release workflows.
