//! Runs scenarios on the real Soroban host VM and records per-call resources.
//!
//! Contracts are registered from their Wasm bytes, so the VM actually runs the
//! compiled contract. Each top-level invocation is metered on its own by the
//! host (`cost_estimate().resources()` describes the *last* top-level call).
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use sha2::{Digest, Sha256};
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::xdr::{ScAddress, ScErrorType, ScSpecEntry, ScVal};
use soroban_sdk::{Address, Env, InvokeError, Symbol, TryFromVal, Val};

use crate::args::{convert_args, ArgContext};
use crate::baseline::{Baseline, ContractInfo, Entry, Metrics};
use crate::config::{ContractCfg, LoadedConfig};
use crate::scenario::{Auth, Expect, Scenario, Step};
use crate::CoreError;

/// A contract from the config, loaded and decoded.
#[derive(Debug, Clone)]
pub struct ContractDef {
    /// Alias.
    pub name: String,
    /// Wasm bytes.
    pub wasm: Vec<u8>,
    /// Decoded spec entries.
    pub spec: Vec<ScSpecEntry>,
    /// Constructor arguments from the config.
    pub constructor: Option<BTreeMap<String, toml::Value>>,
}

impl ContractDef {
    /// Reads the Wasm file and decodes its spec.
    pub fn load(lc: &LoadedConfig, c: &ContractCfg) -> Result<ContractDef, CoreError> {
        let path = lc.resolve(&c.wasm);
        let wasm = std::fs::read(&path).map_err(|e| {
            CoreError::Config(format!(
                "contract `{}`: cannot read wasm {}: {e} (build it first)",
                c.name,
                path.display()
            ))
        })?;
        let report = gauge_wasm::inspect(&wasm)
            .map_err(|e| CoreError::Config(format!("contract `{}`: {e}", c.name)))?;
        let section_missing = report.spec_functions.is_empty();
        let spec = spec_entries(&wasm)
            .map_err(|e| CoreError::Config(format!("contract `{}`: {e}", c.name)))?;
        if section_missing && spec.is_empty() {
            return Err(CoreError::Config(format!(
                "contract `{}` has no contractspecv0 section, so scenario arguments cannot be typed",
                c.name
            )));
        }
        Ok(ContractDef {
            name: c.name.clone(),
            wasm,
            spec,
            constructor: c.constructor.clone(),
        })
    }

    /// Wasm identity for the baseline.
    pub fn info(&self) -> ContractInfo {
        ContractInfo {
            wasm_sha256: hex::encode(Sha256::digest(&self.wasm)),
            wasm_size_bytes: self.wasm.len() as u64,
        }
    }

    fn function(&self, name: &str) -> Option<&soroban_sdk::xdr::ScSpecFunctionV0> {
        self.spec.iter().find_map(|e| match e {
            ScSpecEntry::FunctionV0(f) if f.name.to_utf8_string_lossy() == name => Some(f),
            _ => None,
        })
    }

    fn function_names(&self) -> Vec<String> {
        self.spec
            .iter()
            .filter_map(|e| match e {
                ScSpecEntry::FunctionV0(f) => Some(f.name.to_utf8_string_lossy()),
                _ => None,
            })
            .collect()
    }
}

fn spec_entries(wasm: &[u8]) -> Result<Vec<ScSpecEntry>, String> {
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        if let wasmparser::Payload::CustomSection(c) = payload.map_err(|e| e.to_string())? {
            if c.name() == "contractspecv0" {
                return gauge_wasm::decode_spec(c.data()).map_err(|e| e.to_string());
            }
        }
    }
    Ok(Vec::new())
}

/// How a call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Ok,
    ContractError(u32),
    Failed(String),
}

impl Outcome {
    fn describe(&self) -> String {
        match self {
            Outcome::Ok => "succeeded".into(),
            Outcome::ContractError(c) => format!("failed with contract error {c}"),
            Outcome::Failed(m) => format!("failed: {m}"),
        }
    }
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with a non-text payload".to_string()
    }
}

fn sc_address(env: &Env, a: &Address) -> Result<ScAddress, String> {
    match ScVal::try_from_val(env, &a.to_val()) {
        Ok(ScVal::Address(a)) => Ok(a),
        other => Err(format!("unexpected address conversion: {other:?}")),
    }
}

fn to_vals(env: &Env, args: &[ScVal]) -> Result<soroban_sdk::Vec<Val>, String> {
    let mut v = soroban_sdk::Vec::<Val>::new(env);
    for a in args {
        v.push_back(
            Val::try_from_val(env, a).map_err(|e| format!("argument conversion failed: {e:?}"))?,
        );
    }
    Ok(v)
}

fn u64_of(x: i64) -> u64 {
    u64::try_from(x).unwrap_or(0)
}

/// Runs the scenarios (optionally only those named in `only`) and returns a
/// sorted, deterministic result.
pub fn run(
    contracts: &BTreeMap<String, ContractDef>,
    scenarios: &[Scenario],
    only: Option<&[String]>,
) -> Result<Baseline, CoreError> {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = run_inner(contracts, scenarios, only);
    std::panic::set_hook(prev_hook);
    result
}

fn run_inner(
    contracts: &BTreeMap<String, ContractDef>,
    scenarios: &[Scenario],
    only: Option<&[String]>,
) -> Result<Baseline, CoreError> {
    let mut entries = Vec::new();
    for sc in scenarios {
        if only.is_some_and(|names| !names.contains(&sc.name)) {
            continue;
        }
        entries.extend(run_scenario(contracts, sc)?);
    }
    let infos = contracts
        .iter()
        .map(|(k, v)| (k.clone(), v.info()))
        .collect();
    Baseline::new(infos, entries)
}

fn scenario_err(sc: &Scenario, step: Option<&Step>, msg: impl std::fmt::Display) -> String {
    match step {
        Some(st) => format!("scenario `{}`, step `{}`: {msg}", sc.name, st.id),
        None => format!("scenario `{}`: {msg}", sc.name),
    }
}

fn run_scenario(
    contracts: &BTreeMap<String, ContractDef>,
    sc: &Scenario,
) -> Result<Vec<Entry>, CoreError> {
    let env = Env::default();
    let mut li = env.ledger().get();
    li.timestamp = 1_700_000_000;
    li.sequence_number = 1_000;
    env.ledger().set(li);

    let mut deployed_sc: BTreeMap<String, ScAddress> = BTreeMap::new();
    let mut deployed: BTreeMap<String, Address> = BTreeMap::new();

    for alias in &sc.contracts {
        let def = contracts.get(alias).ok_or_else(|| {
            CoreError::Config(scenario_err(
                sc,
                None,
                format!("contract `{alias}` is not declared in the config"),
            ))
        })?;
        let ctor_args = constructor_args(def, &deployed_sc).map_err(|e| {
            CoreError::Config(scenario_err(
                sc,
                None,
                format!("contract `{alias}` constructor: {e}"),
            ))
        })?;
        let deployed_try = catch_unwind(AssertUnwindSafe(|| -> Result<Address, String> {
            let args = to_vals(&env, &ctor_args)?;
            Ok(env.register(def.wasm.as_slice(), args))
        }));
        let addr = match deployed_try {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                return Err(CoreError::Scenario(scenario_err(
                    sc,
                    None,
                    format!("deploying `{alias}`: {e}"),
                )))
            }
            Err(p) => {
                return Err(CoreError::Scenario(scenario_err(
                    sc,
                    None,
                    format!("deploying `{alias}` failed: {}", panic_text(p)),
                )))
            }
        };
        let sca =
            sc_address(&env, &addr).map_err(|e| CoreError::Scenario(scenario_err(sc, None, e)))?;
        deployed_sc.insert(alias.clone(), sca);
        deployed.insert(alias.clone(), addr);
    }

    let mut out = Vec::new();
    for step in &sc.steps {
        let def = contracts.get(&step.contract).ok_or_else(|| {
            CoreError::Config(scenario_err(
                sc,
                Some(step),
                format!("contract `{}` is not declared in the config", step.contract),
            ))
        })?;
        let func = def.function(&step.function).ok_or_else(|| {
            CoreError::Config(scenario_err(
                sc,
                Some(step),
                format!(
                    "contract `{}` has no function `{}` (functions: {})",
                    step.contract,
                    step.function,
                    def.function_names().join(", ")
                ),
            ))
        })?;
        let ctx = ArgContext {
            spec: &def.spec,
            deployed: &deployed_sc,
        };
        let sc_args = convert_args(func.inputs.as_slice(), &step.args, &ctx)
            .map_err(|e| CoreError::Config(scenario_err(sc, Some(step), e)))?;
        let addr = deployed.get(&step.contract).ok_or_else(|| {
            CoreError::Config(scenario_err(
                sc,
                Some(step),
                format!(
                    "contract `{}` is not deployed in this scenario",
                    step.contract
                ),
            ))
        })?;

        match step.auth {
            Auth::All => env.mock_all_auths(),
            Auth::None => env.set_auths(&[]),
        }

        let times = step.repeat.unwrap_or(1);
        for n in 1..=times {
            let outcome = invoke(&env, addr, &step.function, &sc_args)
                .map_err(|e| CoreError::Scenario(scenario_err(sc, Some(step), e)))?;
            check_expect(sc, step, &outcome)?;
            if step.measure {
                let (metrics, fee_components) = read_resources(&env)
                    .map_err(|e| CoreError::Scenario(scenario_err(sc, Some(step), e)))?;
                let key = if step.repeat.is_some() {
                    format!("{}/{}#{n}", sc.name, step.id)
                } else {
                    format!("{}/{}", sc.name, step.id)
                };
                out.push(Entry {
                    key,
                    scenario: sc.name.clone(),
                    step: step.id.clone(),
                    contract: step.contract.clone(),
                    function: step.function.clone(),
                    metrics,
                    fee_components,
                });
            }
        }
    }
    Ok(out)
}

fn constructor_args(
    def: &ContractDef,
    deployed: &BTreeMap<String, ScAddress>,
) -> Result<Vec<ScVal>, String> {
    let ctor = def.function("__constructor");
    match (ctor, &def.constructor) {
        (None, None) => Ok(Vec::new()),
        (None, Some(t)) if t.is_empty() => Ok(Vec::new()),
        (None, Some(_)) => {
            Err("config gives constructor arguments but the contract has no __constructor".into())
        }
        (Some(f), given) => {
            let empty = BTreeMap::new();
            let given = given.as_ref().unwrap_or(&empty);
            let ctx = ArgContext {
                spec: &def.spec,
                deployed,
            };
            convert_args(f.inputs.as_slice(), given, &ctx)
        }
    }
}

fn invoke(env: &Env, addr: &Address, function: &str, args: &[ScVal]) -> Result<Outcome, String> {
    let res = catch_unwind(AssertUnwindSafe(|| -> Result<Outcome, String> {
        let sym = Symbol::new(env, function);
        let vals = to_vals(env, args)?;
        Ok(
            match env.try_invoke_contract::<Val, soroban_sdk::Error>(addr, &sym, vals) {
                Ok(_) => Outcome::Ok,
                Err(Ok(err)) => {
                    if err.is_type(ScErrorType::Contract) {
                        Outcome::ContractError(err.get_code())
                    } else {
                        Outcome::Failed(format!("{err:?}"))
                    }
                }
                Err(Err(InvokeError::Contract(code))) => Outcome::ContractError(code),
                Err(Err(InvokeError::Abort)) => {
                    Outcome::Failed("the contract trapped (abort)".into())
                }
            },
        )
    }));
    match res {
        Ok(r) => r,
        Err(p) => Err(format!(
            "the host panicked while calling `{function}`: {}",
            panic_text(p)
        )),
    }
}

fn check_expect(sc: &Scenario, step: &Step, outcome: &Outcome) -> Result<(), CoreError> {
    let ok = match (&step.expect, outcome) {
        (Expect::Ok, Outcome::Ok) => true,
        (Expect::Error, Outcome::ContractError(_) | Outcome::Failed(_)) => true,
        (Expect::ErrorCode(c), Outcome::ContractError(got)) => c == got,
        _ => false,
    };
    if ok {
        return Ok(());
    }
    let wanted = match &step.expect {
        Expect::Ok => "success".to_string(),
        Expect::Error => "an error".to_string(),
        Expect::ErrorCode(c) => format!("contract error {c}"),
    };
    Err(CoreError::Scenario(scenario_err(
        sc,
        Some(step),
        format!("expected {wanted} but the call {}", outcome.describe()),
    )))
}

fn read_resources(env: &Env) -> Result<(Metrics, BTreeMap<String, u64>), String> {
    let got = catch_unwind(AssertUnwindSafe(|| {
        let est = env.cost_estimate();
        (est.resources(), est.fee())
    }))
    .map_err(|p| {
        format!(
            "no resource report is available for this call ({}); set `measure = false` for steps that fail",
            panic_text(p)
        )
    })?;
    let (r, fee) = got;
    let metrics = Metrics {
        instructions: u64_of(r.instructions),
        mem_bytes: u64_of(r.mem_bytes),
        disk_read_entries: u64::from(r.disk_read_entries),
        memory_read_entries: u64::from(r.memory_read_entries),
        write_entries: u64::from(r.write_entries),
        disk_read_bytes: u64::from(r.disk_read_bytes),
        write_bytes: u64::from(r.write_bytes),
        events_bytes: u64::from(r.contract_events_size_bytes),
        fee_stroops: u64_of(fee.total),
    };
    let mut comps = BTreeMap::new();
    comps.insert("instructions".to_string(), u64_of(fee.instructions));
    comps.insert(
        "disk_read_entries".to_string(),
        u64_of(fee.disk_read_entries),
    );
    comps.insert("write_entries".to_string(), u64_of(fee.write_entries));
    comps.insert("disk_read_bytes".to_string(), u64_of(fee.disk_read_bytes));
    comps.insert("write_bytes".to_string(), u64_of(fee.write_bytes));
    comps.insert("contract_events".to_string(), u64_of(fee.contract_events));
    comps.insert(
        "persistent_entry_rent".to_string(),
        u64_of(fee.persistent_entry_rent),
    );
    comps.insert(
        "temporary_entry_rent".to_string(),
        u64_of(fee.temporary_entry_rent),
    );
    Ok((metrics, comps))
}
