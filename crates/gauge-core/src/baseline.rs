//! Baseline / run-report file model. Deterministic by construction: sorted
//! keys, sorted entries, no timestamps, no absolute paths.
use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Current schema version of baseline and report files.
pub const SCHEMA_VERSION: u32 = 1;

/// Per-invocation resource numbers. Only metrics the pinned SDK reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    /// Modelled CPU instructions.
    pub instructions: u64,
    /// Modelled memory in bytes.
    pub mem_bytes: u64,
    /// Entries read from disk (restored or classic entries).
    pub disk_read_entries: u64,
    /// In-memory ledger entries accessed.
    pub memory_read_entries: u64,
    /// Entries written.
    pub write_entries: u64,
    /// Bytes read from disk.
    pub disk_read_bytes: u64,
    /// Bytes written.
    pub write_bytes: u64,
    /// Size of emitted contract events.
    pub events_bytes: u64,
    /// Estimated fee in stroops (see docs/limitations.md).
    pub fee_stroops: u64,
}

impl Metrics {
    /// Metric names in report order.
    pub const NAMES: [&'static str; 9] = [
        "instructions",
        "mem_bytes",
        "disk_read_entries",
        "memory_read_entries",
        "write_entries",
        "disk_read_bytes",
        "write_bytes",
        "events_bytes",
        "fee_stroops",
    ];

    /// Value of a metric by name.
    pub fn get(&self, name: &str) -> Option<u64> {
        Some(match name {
            "instructions" => self.instructions,
            "mem_bytes" => self.mem_bytes,
            "disk_read_entries" => self.disk_read_entries,
            "memory_read_entries" => self.memory_read_entries,
            "write_entries" => self.write_entries,
            "disk_read_bytes" => self.disk_read_bytes,
            "write_bytes" => self.write_bytes,
            "events_bytes" => self.events_bytes,
            "fee_stroops" => self.fee_stroops,
            _ => return None,
        })
    }
}

/// One measured invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// `scenario/step` or `scenario/step#N`.
    pub key: String,
    /// Scenario name.
    pub scenario: String,
    /// Step id.
    pub step: String,
    /// Contract alias.
    pub contract: String,
    /// Function name.
    pub function: String,
    /// Resource numbers.
    pub metrics: Metrics,
    /// Components of `fee_stroops` as reported by the SDK.
    #[serde(default)]
    pub fee_components: BTreeMap<String, u64>,
}

/// Wasm identity recorded per contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractInfo {
    /// SHA-256 of the Wasm file.
    pub wasm_sha256: String,
    /// Size of the Wasm file.
    pub wasm_size_bytes: u64,
}

/// Versions that decide whether two result files are comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// soroban-gauge version (informational; not compared).
    pub tool_version: String,
    /// `soroban-sdk` version.
    pub soroban_sdk_version: String,
    /// `soroban-env-host` version.
    pub soroban_env_host_version: String,
    /// Ledger protocol version of the host.
    pub protocol_version: u32,
}

impl Environment {
    /// The environment of this binary.
    #[cfg(feature = "runner")]
    pub fn current() -> Environment {
        Environment {
            tool_version: crate::environment::TOOL_VERSION.to_string(),
            soroban_sdk_version: crate::environment::SOROBAN_SDK_VERSION.to_string(),
            soroban_env_host_version: crate::environment::SOROBAN_ENV_HOST_VERSION.to_string(),
            protocol_version: crate::environment::protocol_version(),
        }
    }

    /// True when costs from the two environments are comparable.
    pub fn comparable_with(&self, other: &Environment) -> bool {
        self.soroban_sdk_version == other.soroban_sdk_version
            && self.soroban_env_host_version == other.soroban_env_host_version
            && self.protocol_version == other.protocol_version
    }
}

/// Baseline file. Run reports use the same shape plus an optional `wasm` map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// Always [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Host versions the numbers were produced with.
    pub environment: Environment,
    /// Wasm identity per contract alias.
    pub contracts: BTreeMap<String, ContractInfo>,
    /// Measurements sorted by key.
    pub entries: Vec<Entry>,
    /// Static Wasm reports (run reports only; never written to baselines).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wasm: BTreeMap<String, serde_json::Value>,
}

impl Baseline {
    /// Builds a baseline for this binary's host, sorting entries and rejecting
    /// duplicate keys.
    #[cfg(feature = "runner")]
    pub fn new(
        contracts: BTreeMap<String, ContractInfo>,
        entries: Vec<Entry>,
    ) -> Result<Baseline, CoreError> {
        Baseline::with_environment(Environment::current(), contracts, entries)
    }

    /// Builds a baseline for an explicit environment.
    pub fn with_environment(
        environment: Environment,
        contracts: BTreeMap<String, ContractInfo>,
        mut entries: Vec<Entry>,
    ) -> Result<Baseline, CoreError> {
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        if let Some(w) = entries.windows(2).find(|w| w[0].key == w[1].key) {
            return Err(CoreError::Config(format!(
                "duplicate entry key `{}`",
                w[0].key
            )));
        }
        Ok(Baseline {
            schema_version: SCHEMA_VERSION,
            environment,
            contracts,
            entries,
            wasm: BTreeMap::new(),
        })
    }

    /// Canonical text: sorted keys, 2-space indent, trailing newline.
    pub fn to_canonical_json(&self) -> Result<String, CoreError> {
        let value = serde_json::to_value(self).map_err(|e| CoreError::Io(e.to_string()))?;
        let mut s =
            serde_json::to_string_pretty(&value).map_err(|e| CoreError::Io(e.to_string()))?;
        s.push('\n');
        Ok(s)
    }

    /// Writes atomically (temp file in the same directory, then rename).
    pub fn write(&self, path: &Path) -> Result<(), CoreError> {
        let text = self.to_canonical_json()?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = std::path::PathBuf::from(tmp);
        std::fs::write(&tmp, text).map_err(|e| CoreError::io(&tmp, e))?;
        std::fs::rename(&tmp, path).map_err(|e| CoreError::io(path, e))
    }

    /// Reads and validates a baseline or run report.
    pub fn read(path: &Path) -> Result<Baseline, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        let b: Baseline = serde_json::from_str(&text).map_err(|e| {
            CoreError::Io(format!("{}: not a valid result file: {e}", path.display()))
        })?;
        if b.schema_version != SCHEMA_VERSION {
            return Err(CoreError::Io(format!(
                "{}: unsupported schema_version {} (this tool reads {SCHEMA_VERSION})",
                path.display(),
                b.schema_version
            )));
        }
        Ok(b)
    }

    /// Merges `fresh` into `self`, replacing entries whose scenario is in
    /// `scenarios` and keeping the rest (used by `snapshot --only`).
    pub fn merge_scenarios(
        &self,
        fresh: &Baseline,
        scenarios: &[String],
    ) -> Result<Baseline, CoreError> {
        let mut entries: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| !scenarios.contains(&e.scenario))
            .cloned()
            .collect();
        entries.extend(fresh.entries.iter().cloned());
        let mut contracts = self.contracts.clone();
        contracts.extend(fresh.contracts.clone());
        Baseline::with_environment(fresh.environment.clone(), contracts, entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> Entry {
        Entry {
            key: key.into(),
            scenario: "s".into(),
            step: "t".into(),
            contract: "c".into(),
            function: "f".into(),
            metrics: Metrics::default(),
            fee_components: BTreeMap::new(),
        }
    }

    #[test]
    fn sorted_and_deterministic() {
        let b = Baseline::new(BTreeMap::new(), vec![entry("b/x"), entry("a/x")]).unwrap();
        assert_eq!(b.entries[0].key, "a/x");
        let t = b.to_canonical_json().unwrap();
        assert!(t.ends_with("}\n"));
        assert_eq!(t, b.to_canonical_json().unwrap());
        assert!(!t.contains("timestamp"));
    }

    #[test]
    fn duplicate_keys_rejected() {
        assert!(Baseline::new(BTreeMap::new(), vec![entry("a/x"), entry("a/x")]).is_err());
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/baseline.json");
        let b = Baseline::new(BTreeMap::new(), vec![entry("a/x")]).unwrap();
        b.write(&p).unwrap();
        assert_eq!(Baseline::read(&p).unwrap(), b);
        assert!(!dir.path().join("sub/baseline.json.tmp").exists());
    }

    #[test]
    fn merge_replaces_only_named_scenarios() {
        let mut old_a = entry("a/x");
        old_a.scenario = "a".into();
        let mut old_b = entry("b/x");
        old_b.scenario = "b".into();
        let base = Baseline::new(BTreeMap::new(), vec![old_a, old_b.clone()]).unwrap();
        let mut new_a = entry("a/x");
        new_a.scenario = "a".into();
        new_a.metrics.instructions = 7;
        let fresh = Baseline::new(BTreeMap::new(), vec![new_a]).unwrap();
        let m = base.merge_scenarios(&fresh, &["a".to_string()]).unwrap();
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.entries[0].metrics.instructions, 7);
        assert_eq!(m.entries[1], old_b);
    }
}
