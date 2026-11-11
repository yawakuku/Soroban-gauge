# Examples

| Example | What it shows |
|---|---|
| `counter/` | Smallest contract (`increment`, `get`) with a committed baseline. |
| `counter-regressed/` | Same interface, but `increment` does extra storage writes and a loop. Its config compares against the `counter` baseline, so `check` fails. |
| `token/` | A minimal token with a constructor, typed `i128`/`address` arguments, contract errors and an expected failure. |

Run from the repository root (build once: `cargo build -p gauge-cli`):

```bash
soroban-gauge --config examples/counter/soroban-gauge.toml check            # exit 0
soroban-gauge --config examples/counter-regressed/soroban-gauge.toml check  # exit 1
soroban-gauge --config examples/token/soroban-gauge.toml check              # exit 0
```

The configs point at the prebuilt Wasm in `tests/fixtures/` so the committed
baselines are reproducible. Rebuild the fixtures with `tests/fixtures/build.sh`;
rebuilt Wasm can differ byte-for-byte (compiler and `stellar-cli` versions), which
changes the Wasm hash and may change the numbers, so re-run `snapshot` afterwards.
