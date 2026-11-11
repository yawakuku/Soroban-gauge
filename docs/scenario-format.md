# Scenario format

Scenario files are TOML. Each file holds `[[scenario]]` tables; each scenario deploys its contracts
fresh (a new host environment), then runs its steps in order. Unknown keys are errors.

```toml
[[scenario]]
name = "token_basic"           # unique across all files; no "/"
description = "mint then transfer"
contracts = ["token"]          # aliases from [[contract]] in soroban-gauge.toml

  [[scenario.step]]
  id = "mint"                  # unique within the scenario; no "/" or "#"
  contract = "token"
  function = "mint"
  args = { to = "@alice", amount = "1000000000" }
  auth = "all"                 # all (default) | none
  measure = false              # default true; false = executed but not recorded
  expect = "ok"                # ok (default) | error | { error = 3 }
  repeat = 1                   # optional; N runs recorded as id#1..id#N
```

## Arguments

Arguments are keyed by the parameter names in the contract's own spec (`contractspecv0`); order and
types come from there. Omitted `option<T>` arguments become void. Errors name the scenario, step,
argument, expected type and what was supplied, and exit with code 2.

| Spec type | TOML value |
|---|---|
| `bool` | `true` / `false` |
| `u32`, `i32` | integer |
| `u64`, `i64`, `timepoint`, `duration` | integer or decimal string |
| `u128`, `i128` | decimal string (or small integer) |
| `bytes`, `bytesN` | hex string (`"0a1b"`, optional `0x`) |
| `string`, `symbol` | string |
| `address` | `"@name"` (stable test account), `"@name:contract"` (stable contract address), `"$alias"` (a contract deployed in this scenario), or a `G...`/`C...` address |
| `vec<T>`, tuple | array |
| `map<K,V>` | table (string/symbol keys) or array of `[key, value]` pairs |
| struct | table with the declared field names |
| enum (integer) | case name or integer |
| union | `"Case"` for no-data cases, `{ Case = value-or-array }` otherwise |

`@name` addresses are derived from a fixed hash, so they are identical on every run and machine.

## Outcomes

`expect = "ok"` requires success; `"error"` any failure; `{ error = N }` requires contract error code `N`.
A call that does not match its expectation is a scenario failure (exit code 3), not a metric.

## Constructors

Contracts with `__constructor` need `constructor = { ... }` in their `[[contract]]` table, using the
parameter names from the spec.
