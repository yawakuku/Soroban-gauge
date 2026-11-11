//! Pure diff logic.
use std::collections::{BTreeMap, BTreeSet};

use gauge_core::config::SortOrder;
use gauge_core::{Baseline, Entry, Environment, Metrics, Thresholds};
use serde::Serialize;

/// Schema version of the JSON diff output.
pub const DIFF_SCHEMA_VERSION: u32 = 1;

/// Classification of one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// A metric exceeded its threshold.
    Regressed,
    /// Nothing got worse and something got better.
    Improved,
    /// Identical, or only increases that stay within thresholds.
    Unchanged,
    /// Not in the baseline.
    New,
    /// In the baseline but not in the current run.
    Removed,
    /// The Wasm hash is unchanged but the numbers moved.
    EnvironmentDrift,
}

impl Status {
    /// Lower-case, hyphenated label.
    pub fn label(self) -> &'static str {
        match self {
            Status::Regressed => "regressed",
            Status::Improved => "improved",
            Status::Unchanged => "unchanged",
            Status::New => "new",
            Status::Removed => "removed",
            Status::EnvironmentDrift => "environment-drift",
        }
    }
}

/// When the overall verdict is a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOn {
    /// A metric exceeded its threshold.
    Regression,
    /// Any difference at all, including new and removed entries.
    AnyChange,
    /// Never.
    Never,
}

/// Options for [`diff`].
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Thresholds from the config.
    pub thresholds: Thresholds,
    /// Failure policy.
    pub fail_on: FailOn,
    /// Row ordering.
    pub sort: SortOrder,
}

/// One metric compared.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricDiff {
    /// Metric name.
    pub name: String,
    /// Baseline value (0 for new entries).
    pub baseline: u64,
    /// Current value (0 for removed entries).
    pub current: u64,
    /// `current - baseline`.
    pub delta: i128,
    /// Percent change; `None` when the baseline is 0.
    pub pct: Option<f64>,
    /// Exceeded its threshold.
    pub failed: bool,
}

/// One entry compared.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowDiff {
    /// Entry key.
    pub key: String,
    /// Contract alias.
    pub contract: String,
    /// Function name.
    pub function: String,
    /// Classification.
    pub status: Status,
    /// True when any metric exceeded its threshold (regardless of status label).
    pub fails: bool,
    /// True when any metric differs, or the entry is new or removed.
    pub changed: bool,
    /// All metrics.
    pub metrics: Vec<MetricDiff>,
}

impl RowDiff {
    /// A metric by name.
    pub fn metric(&self, name: &str) -> Option<&MetricDiff> {
        self.metrics.iter().find(|m| m.name == name)
    }
}

/// Wasm identity change for one contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WasmDelta {
    /// Contract alias.
    pub contract: String,
    /// Baseline size, if the contract was in the baseline.
    pub baseline_size_bytes: Option<u64>,
    /// Current size.
    pub current_size_bytes: Option<u64>,
    /// Hash changed (or one side missing).
    pub changed: bool,
}

/// A Wasm size that crossed a configured limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SizeFinding {
    /// Contract alias.
    pub contract: String,
    /// `"warn"` or `"fail"`.
    pub status: String,
    /// Current Wasm size.
    pub size_bytes: u64,
    /// The limit that was crossed.
    pub limit_bytes: u64,
}

/// Full comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffReport {
    /// Schema version of this JSON shape.
    pub schema_version: u32,
    /// Environment of the baseline.
    pub baseline_environment: Environment,
    /// Environment of the current run.
    pub current_environment: Environment,
    /// Costs are not comparable across these environments.
    pub environment_mismatch: bool,
    /// Rows in display order.
    pub rows: Vec<RowDiff>,
    /// Rows with `fails == true`.
    pub regressions: usize,
    /// Rows with `changed == true`.
    pub changed: usize,
    /// Rows compared in total.
    pub total: usize,
    /// Overall verdict under the chosen failure policy.
    pub failed: bool,
    /// Wasm size and hash changes.
    pub wasm: Vec<WasmDelta>,
    /// Size-limit findings (filled in by the caller from `[wasm_limits]`).
    pub size_findings: Vec<SizeFinding>,
}

/// Percent change; `None` when the baseline is zero.
pub fn pct_change(baseline: u64, current: u64) -> Option<f64> {
    if baseline == 0 {
        return None;
    }
    let delta = i128::from(current) - i128::from(baseline);
    Some(delta as f64 * 100.0 / baseline as f64)
}

fn limits_for(name: &str, t: &Thresholds) -> (Option<f64>, Option<u64>) {
    match name {
        "instructions" => (t.instructions_pct, t.instructions_abs),
        "mem_bytes" => (t.mem_bytes_pct, t.mem_bytes_abs),
        "fee_stroops" => (t.fee_stroops_pct, t.fee_stroops_abs),
        "disk_read_entries" | "memory_read_entries" => (t.read_entries_pct, t.read_entries_abs),
        "write_entries" => (t.write_entries_pct, t.write_entries_abs),
        "write_bytes" => (t.write_bytes_pct, t.write_bytes_abs),
        "disk_read_bytes" => (t.disk_read_bytes_pct, t.disk_read_bytes_abs),
        "events_bytes" => (t.events_bytes_pct, t.events_bytes_abs),
        _ => (None, None),
    }
}

/// Whether `current` regresses `baseline` for the named metric. Only increases
/// can fail. When both a percent and an absolute limit are configured, both must
/// be exceeded; with one, that one decides; with none, nothing fails. A zero
/// baseline counts as an infinite percent increase.
pub fn metric_fails(name: &str, baseline: u64, current: u64, t: &Thresholds) -> bool {
    if current <= baseline {
        return false;
    }
    let delta = u128::from(current - baseline);
    let (pct_limit, abs_limit) = limits_for(name, t);
    let pct_exceeded = pct_limit.map(|limit| {
        if baseline == 0 {
            true
        } else {
            delta as f64 * 100.0 / baseline as f64 > limit
        }
    });
    let abs_exceeded = abs_limit.map(|limit| delta > u128::from(limit));
    match (pct_exceeded, abs_exceeded) {
        (Some(p), Some(a)) => p && a,
        (Some(p), None) => p,
        (None, Some(a)) => a,
        (None, None) => false,
    }
}

fn metric_diffs(
    base: Option<&Metrics>,
    cur: Option<&Metrics>,
    t: &Thresholds,
    gate: bool,
) -> Vec<MetricDiff> {
    Metrics::NAMES
        .iter()
        .map(|name| {
            let b = base.and_then(|m| m.get(name)).unwrap_or(0);
            let c = cur.and_then(|m| m.get(name)).unwrap_or(0);
            MetricDiff {
                name: (*name).to_string(),
                baseline: b,
                current: c,
                delta: i128::from(c) - i128::from(b),
                pct: pct_change(b, c),
                failed: gate && metric_fails(name, b, c, t),
            }
        })
        .collect()
}

fn row(
    entry: &Entry,
    status: Status,
    fails: bool,
    changed: bool,
    metrics: Vec<MetricDiff>,
) -> RowDiff {
    RowDiff {
        key: entry.key.clone(),
        contract: entry.contract.clone(),
        function: entry.function.clone(),
        status,
        fails,
        changed,
        metrics,
    }
}

fn sort_rank(r: &RowDiff) -> u8 {
    match (r.fails, r.status, r.changed) {
        (true, _, _) => 0,
        (_, Status::EnvironmentDrift, _) => 1,
        (_, Status::New, _) => 2,
        (_, Status::Removed, _) => 3,
        (_, Status::Improved, _) => 4,
        (_, _, true) => 5,
        _ => 6,
    }
}

/// Compares `current` against `baseline`.
pub fn diff(baseline: &Baseline, current: &Baseline, opts: &DiffOptions) -> DiffReport {
    let base: BTreeMap<&str, &Entry> = baseline
        .entries
        .iter()
        .map(|e| (e.key.as_str(), e))
        .collect();
    let cur: BTreeMap<&str, &Entry> = current
        .entries
        .iter()
        .map(|e| (e.key.as_str(), e))
        .collect();
    let keys: BTreeSet<&str> = base.keys().chain(cur.keys()).copied().collect();

    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        match (base.get(key), cur.get(key)) {
            (Some(b), Some(c)) => {
                let metrics =
                    metric_diffs(Some(&b.metrics), Some(&c.metrics), &opts.thresholds, true);
                let fails = metrics.iter().any(|m| m.failed);
                let changed = metrics.iter().any(|m| m.delta != 0);
                let increased = metrics.iter().any(|m| m.delta > 0);
                let decreased = metrics.iter().any(|m| m.delta < 0);
                let same_wasm = match (
                    baseline.contracts.get(&c.contract),
                    current.contracts.get(&c.contract),
                ) {
                    (Some(x), Some(y)) => x.wasm_sha256 == y.wasm_sha256,
                    _ => false,
                };
                let status = if !changed {
                    Status::Unchanged
                } else if same_wasm {
                    Status::EnvironmentDrift
                } else if fails {
                    Status::Regressed
                } else if decreased && !increased {
                    Status::Improved
                } else {
                    Status::Unchanged
                };
                rows.push(row(c, status, fails, changed, metrics));
            }
            (None, Some(c)) => {
                let metrics = metric_diffs(None, Some(&c.metrics), &opts.thresholds, false);
                rows.push(row(c, Status::New, false, true, metrics));
            }
            (Some(b), None) => {
                let metrics = metric_diffs(Some(&b.metrics), None, &opts.thresholds, false);
                rows.push(row(b, Status::Removed, false, true, metrics));
            }
            (None, None) => {}
        }
    }

    let instr = |r: &RowDiff| r.metric("instructions").map_or(0, |m| m.delta);
    let cur_instr = |r: &RowDiff| r.metric("instructions").map_or(0, |m| m.current);
    match opts.sort {
        SortOrder::Name => rows.sort_by(|a, b| a.key.cmp(&b.key)),
        SortOrder::Cost => rows.sort_by(|a, b| {
            cur_instr(b)
                .cmp(&cur_instr(a))
                .then_with(|| a.key.cmp(&b.key))
        }),
        SortOrder::Regression => rows.sort_by(|a, b| {
            sort_rank(a)
                .cmp(&sort_rank(b))
                .then_with(|| instr(b).cmp(&instr(a)))
                .then_with(|| a.key.cmp(&b.key))
        }),
    }

    let regressions = rows.iter().filter(|r| r.fails).count();
    let changed = rows.iter().filter(|r| r.changed).count();
    let failed = match opts.fail_on {
        FailOn::Regression => regressions > 0,
        FailOn::AnyChange => changed > 0,
        FailOn::Never => false,
    };

    let names: BTreeSet<&String> = baseline
        .contracts
        .keys()
        .chain(current.contracts.keys())
        .collect();
    let wasm = names
        .into_iter()
        .map(|n| {
            let b = baseline.contracts.get(n);
            let c = current.contracts.get(n);
            WasmDelta {
                contract: n.clone(),
                baseline_size_bytes: b.map(|x| x.wasm_size_bytes),
                current_size_bytes: c.map(|x| x.wasm_size_bytes),
                changed: b.map(|x| &x.wasm_sha256) != c.map(|x| &x.wasm_sha256),
            }
        })
        .collect();

    DiffReport {
        schema_version: DIFF_SCHEMA_VERSION,
        baseline_environment: baseline.environment.clone(),
        current_environment: current.environment.clone(),
        environment_mismatch: !baseline.environment.comparable_with(&current.environment),
        total: rows.len(),
        rows,
        regressions,
        changed,
        failed,
        wasm,
        size_findings: Vec::new(),
    }
}
