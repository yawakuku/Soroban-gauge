//! Command implementations. Machine output goes to stdout, logs to stderr.
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use gauge_core::baseline::{Baseline, ContractInfo};
use gauge_core::config::{Config, LoadedConfig, WasmLimits};
use gauge_core::runner::{self, ContractDef};
use gauge_core::{scenario, CoreError};
use gauge_report::{
    diff, render_json, render_markdown, render_table, DiffOptions, FailOn, RenderOptions,
    SizeFinding,
};
use gauge_wasm::LimitStatus;

use crate::{Cli, Command, CompareArgs, DiffFormat, FailOnArg, InspectFormat};

const CONFIG_TEMPLATE: &str = r#"# soroban-gauge configuration. Paths are relative to this file.

[project]
baseline = ".soroban-gauge/baseline.json"
scenarios = ["scenarios/*.toml"]

[[contract]]
name = "counter"
wasm = "target/wasm32v1-none/release/counter.wasm"
# Only for contracts that define __constructor; arguments are named as in the contract spec:
# constructor = { admin = "@admin" }

# A metric fails only when it exceeds BOTH its _pct and its _abs limit.
[thresholds]
instructions_pct = 5.0
instructions_abs = 10000
mem_bytes_pct = 5.0
mem_bytes_abs = 1024
fee_stroops_pct = 5.0
fee_stroops_abs = 100
read_entries_abs = 0
write_entries_abs = 0
write_bytes_pct = 5.0
write_bytes_abs = 64

# Optional Wasm size limits (unset = report only, never fail). Network limits change
# with protocol upgrades: read current values from the network settings.
# 131072 matches the max contract code entry size in the soroban-sdk 29.0.1 mainnet
# snapshot dated 2026-07-10; 98304 (75%) is an arbitrary warning level. Verify both.
# [wasm_limits]
# max_size_bytes = 131072
# warn_size_bytes = 98304

[report]
max_rows = 50
sort = "regression"   # regression | name | cost
"#;

const SCENARIO_TEMPLATE: &str = r#"# Each scenario deploys its contracts fresh, then runs the steps in order.
[[scenario]]
name = "counter_basic"
description = "increment twice, then read"
contracts = ["counter"]

  [[scenario.step]]
  id = "increment"
  contract = "counter"
  function = "increment"
  repeat = 2            # recorded as increment#1 and increment#2

  [[scenario.step]]
  id = "get"
  contract = "counter"
  function = "get"
"#;

pub fn dispatch(cli: &Cli) -> Result<i32> {
    match &cli.command {
        Command::Init { force } => init(cli, *force),
        Command::Inspect { wasm, format } => inspect(cli, wasm, *format),
        Command::Run { out } => run_cmd(cli, out),
        Command::Snapshot { only } => snapshot(cli, only),
        Command::Diff {
            baseline,
            current,
            compare,
        } => diff_cmd(cli, baseline, current, compare),
        Command::Check { out, compare } => check(cli, out, compare),
        Command::Calibrate => Err(CoreError::Config(
            "`calibrate` is not implemented in this version (planned for milestone 2; see the README roadmap)"
                .to_string(),
        )
        .into()),
    }
}

fn note(cli: &Cli, msg: impl std::fmt::Display) {
    if !cli.quiet {
        eprintln!("{msg}");
    }
}

fn write_new(path: &Path, text: &str, force: bool) -> Result<(), CoreError> {
    if path.exists() && !force {
        return Err(CoreError::Config(format!(
            "{} already exists (use --force to overwrite)",
            path.display()
        )));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    }
    std::fs::write(path, text).map_err(|e| CoreError::io(path, e))
}

fn init(cli: &Cli, force: bool) -> Result<i32> {
    let dir = cli.config.parent().filter(|d| !d.as_os_str().is_empty());
    let scenario_path = dir.map_or_else(
        || Path::new("scenarios").join("example.toml"),
        |d| d.join("scenarios").join("example.toml"),
    );
    // Check both before writing either, so a refusal leaves nothing half-written.
    for p in [&cli.config, &scenario_path] {
        if p.exists() && !force {
            return Err(CoreError::Config(format!(
                "{} already exists (use --force to overwrite)",
                p.display()
            ))
            .into());
        }
    }
    write_new(&cli.config, CONFIG_TEMPLATE, force)?;
    write_new(&scenario_path, SCENARIO_TEMPLATE, force)?;
    note(
        cli,
        format!(
            "wrote {} and {}",
            cli.config.display(),
            scenario_path.display()
        ),
    );
    Ok(0)
}

fn optional_config(cli: &Cli) -> Result<LoadedConfig> {
    if cli.config.exists() {
        Ok(Config::load(&cli.config)?)
    } else {
        Ok(LoadedConfig {
            config: Config::parse("")?,
            base_dir: ".".into(),
        })
    }
}

fn limit_status(size: u64, l: &WasmLimits) -> LimitStatus {
    gauge_wasm::check_limits(size, l.max_size_bytes, l.warn_size_bytes)
}

fn inspect(cli: &Cli, wasm: &Path, format: InspectFormat) -> Result<i32> {
    let bytes = std::fs::read(wasm).map_err(|e| CoreError::io(wasm, e))?;
    let report = gauge_wasm::inspect(&bytes)
        .map_err(|e| CoreError::Io(format!("{}: {e}", wasm.display())))?;
    let lc = optional_config(cli)?;
    let status = limit_status(report.size_bytes, &lc.config.wasm_limits);
    match format {
        InspectFormat::Table => print!("{}", gauge_wasm::render_table(&report, status)),
        InspectFormat::Json => {
            let mut v = serde_json::to_value(&report).map_err(|e| CoreError::Io(e.to_string()))?;
            if let (Some(obj), Ok(s)) = (v.as_object_mut(), serde_json::to_value(status)) {
                obj.insert("size_limit".to_string(), s);
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&v).map_err(|e| CoreError::Io(e.to_string()))?
            );
        }
    }
    Ok(i32::from(status == LimitStatus::Fail))
}

fn size_findings(l: &WasmLimits, contracts: &BTreeMap<String, ContractInfo>) -> Vec<SizeFinding> {
    contracts
        .iter()
        .filter_map(|(name, info)| {
            let size = info.wasm_size_bytes;
            match limit_status(size, l) {
                LimitStatus::Ok => None,
                LimitStatus::Warn => Some(("warn", l.warn_size_bytes?)),
                LimitStatus::Fail => Some(("fail", l.max_size_bytes?)),
            }
            .map(|(status, limit)| SizeFinding {
                contract: name.clone(),
                status: status.to_string(),
                size_bytes: size,
                limit_bytes: limit,
            })
        })
        .collect()
}

/// Loads contracts and scenarios, runs them, and attaches static Wasm reports.
fn execute(lc: &LoadedConfig, only: Option<&[String]>) -> Result<Baseline> {
    if lc.config.contracts.is_empty() {
        return Err(CoreError::Config("the config has no [[contract]] entries".into()).into());
    }
    let mut contracts = BTreeMap::new();
    for c in &lc.config.contracts {
        tracing::info!("loading contract `{}` from {}", c.name, c.wasm);
        contracts.insert(c.name.clone(), ContractDef::load(lc, c)?);
    }
    let files = lc.scenario_files()?;
    if files.is_empty() {
        return Err(CoreError::Config(format!(
            "no scenario files matched {:?} (relative to {})",
            lc.config.project.scenarios,
            lc.base_dir.display()
        ))
        .into());
    }
    let scenarios = scenario::load_scenarios(&files)?;
    if let Some(names) = only {
        for n in names {
            if !scenarios.iter().any(|s| &s.name == n) {
                return Err(CoreError::Config(format!("--only: no scenario named `{n}`")).into());
            }
        }
    }
    tracing::info!("running {} scenario(s)", scenarios.len());
    let mut report = runner::run(&contracts, &scenarios, only)?;
    for (name, def) in &contracts {
        let wasm = gauge_wasm::inspect(&def.wasm)
            .map_err(|e| CoreError::Config(format!("contract `{name}`: {e}")))?;
        let value = serde_json::to_value(wasm).map_err(|e| CoreError::Io(e.to_string()))?;
        report.wasm.insert(name.clone(), value);
    }
    Ok(report)
}

fn print_findings(cli: &Cli, findings: &[SizeFinding]) -> bool {
    let mut failed = false;
    for f in findings {
        note(
            cli,
            format!(
                "{}: wasm `{}` is {} B, over the {} limit of {} B",
                if f.status == "fail" {
                    "error"
                } else {
                    "warning"
                },
                f.contract,
                f.size_bytes,
                if f.status == "fail" {
                    "maximum"
                } else {
                    "warning"
                },
                f.limit_bytes
            ),
        );
        failed |= f.status == "fail";
    }
    failed
}

fn run_cmd(cli: &Cli, out: &Path) -> Result<i32> {
    let lc = Config::load(&cli.config)?;
    let report = execute(&lc, None)?;
    report.write(out)?;
    note(
        cli,
        format!(
            "measured {} call(s); report written to {}",
            report.entries.len(),
            out.display()
        ),
    );
    let findings = size_findings(&lc.config.wasm_limits, &report.contracts);
    Ok(i32::from(print_findings(cli, &findings)))
}

fn snapshot(cli: &Cli, only: &[String]) -> Result<i32> {
    let lc = Config::load(&cli.config)?;
    let only_opt = if only.is_empty() { None } else { Some(only) };
    let mut fresh = execute(&lc, only_opt)?;
    fresh.wasm.clear();
    let path = lc.baseline_path();
    let to_write = match only_opt {
        Some(names) if path.exists() => Baseline::read(&path)?.merge_scenarios(&fresh, names)?,
        _ => fresh,
    };
    to_write.write(&path)?;
    note(
        cli,
        format!(
            "baseline with {} entries written to {}",
            to_write.entries.len(),
            path.display()
        ),
    );
    Ok(0)
}

fn fail_on(f: FailOnArg) -> FailOn {
    match f {
        FailOnArg::Regression => FailOn::Regression,
        FailOnArg::AnyChange => FailOn::AnyChange,
        FailOnArg::Never => FailOn::Never,
    }
}

fn compare(
    cli: &Cli,
    lc: &LoadedConfig,
    base: &Baseline,
    cur: &Baseline,
    args: &CompareArgs,
) -> Result<i32> {
    let mismatch = !base.environment.comparable_with(&cur.environment);
    if mismatch && !args.allow_version_mismatch {
        return Err(CoreError::VersionMismatch(format!(
            "costs are not comparable across host versions (baseline: soroban-sdk {} / env-host {} / protocol {}; current: soroban-sdk {} / env-host {} / protocol {}); re-run `soroban-gauge snapshot` after upgrading, or pass --allow-version-mismatch",
            base.environment.soroban_sdk_version,
            base.environment.soroban_env_host_version,
            base.environment.protocol_version,
            cur.environment.soroban_sdk_version,
            cur.environment.soroban_env_host_version,
            cur.environment.protocol_version,
        ))
        .into());
    }
    let opts = DiffOptions {
        thresholds: lc.config.thresholds.clone(),
        fail_on: fail_on(args.fail_on),
        sort: lc.config.report.sort,
    };
    let mut d = diff(base, cur, &opts);
    d.size_findings = size_findings(&lc.config.wasm_limits, &cur.contracts);
    if d.size_findings.iter().any(|f| f.status == "fail")
        && !matches!(args.fail_on, FailOnArg::Never)
    {
        d.failed = true;
    }
    if let Some(path) = &args.diff_out {
        let json = render_json(&d).map_err(|e| CoreError::Io(e.to_string()))?;
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        std::fs::write(path, json).map_err(|e| CoreError::io(path, e))?;
    }
    let ropts = RenderOptions {
        max_rows: lc.config.report.max_rows,
    };
    match args.format {
        DiffFormat::Table => print!("{}", render_table(&d, ropts)),
        DiffFormat::Markdown => print!("{}", render_markdown(&d, ropts)),
        DiffFormat::Json => {
            print!(
                "{}",
                render_json(&d).map_err(|e| CoreError::Io(e.to_string()))?
            );
        }
    }
    if mismatch {
        note(
            cli,
            "warning: baseline and current host versions differ; numbers are not comparable",
        );
    }
    Ok(i32::from(d.failed))
}

fn diff_cmd(cli: &Cli, baseline: &Path, current: &Path, args: &CompareArgs) -> Result<i32> {
    let lc = optional_config(cli)?;
    let base = Baseline::read(baseline)?;
    let cur = Baseline::read(current)?;
    compare(cli, &lc, &base, &cur, args)
}

fn check(cli: &Cli, out: &Path, args: &CompareArgs) -> Result<i32> {
    let lc = Config::load(&cli.config)?;
    let path = lc.baseline_path();
    if !path.exists() {
        return Err(CoreError::Config(format!(
            "baseline not found at {}; create it with `soroban-gauge snapshot`",
            path.display()
        ))
        .into());
    }
    let base = Baseline::read(&path)?;
    let current = execute(&lc, None)?;
    current.write(out)?;
    compare(cli, &lc, &base, &current, args)
}
