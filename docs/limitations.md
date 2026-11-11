# Limitations

- **Estimates, not fees.** Numbers come from the Soroban host embedded through `soroban-sdk` 29.0.1
  (protocol 29) with the SDK's built-in fee model, which uses hardcoded fee settings copied from a Stellar mainnet snapshot dated 2026-07-10 (per the SDK's own documentation). The fee the network actually charges depends on the
  network's live settings, which can differ. Nothing here is an "exact fee".
- **Costs differ by protocol and host version.** Every baseline records the SDK, host and protocol
  versions; comparing across them exits with code 4 unless `--allow-version-mismatch` is given.
- **Scenarios cover only what you write.** A function with no scenario is not measured. State matters:
  the first call that touches a key can cost more than later calls (compare `increment#1` and `#2`).
- **Authorization.** `auth = "all"` (the default) uses `mock_all_auths`, which records authorization
  and adds its own work to the measured call. `auth = "none"` provides no authorization.
- **Metrics the pinned SDK does not expose are omitted.** The maximum read/write key size
  (`max_rw_key_bytes`) is not reported by `soroban-sdk` 29.0.1, so it is not in baselines. Fee
  components are recorded as reported by the SDK.
- **Failed calls.** A step whose call is expected to fail should use `measure = false`; if the host
  reports no resources for it, measuring it is an error.
- **Argument types.** `u256`/`i256`, raw `val`, `error` and `result` arguments are not supported in
  scenario files yet. Spec-defined structs, enums, unions, options, vectors, maps and tuples are.
- **Sequential only.** Scenarios run one after another; there is no `--jobs` option.
- **Network limits are not built in.** Wasm size limits are optional config values. Read current values
  from the network settings; defaults in the generated config are documented with a date and may be stale.
- **Not yet verified here.** The GitHub Action, the release workflow (including the Windows package step)
  and the `examples` CI job have not been executed on GitHub by the author of this version; their
  shell logic was tested locally on Linux. A Wasm-versus-native cost comparison was not performed.
- **Fixtures.** The committed `.wasm` files in `tests/fixtures` were built without `stellar-cli`'s spec
  shaking (see DEVIATIONS), so their spec section can contain extra entries compared with an official build.
