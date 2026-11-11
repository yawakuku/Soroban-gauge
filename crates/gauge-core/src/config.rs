//! `soroban-gauge.toml`: project configuration.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Default config file name.
pub const DEFAULT_CONFIG: &str = "soroban-gauge.toml";

/// Whole config file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `[project]`
    #[serde(default)]
    pub project: Project,
    /// `[[contract]]`
    #[serde(default, rename = "contract")]
    pub contracts: Vec<ContractCfg>,
    /// `[thresholds]`
    #[serde(default)]
    pub thresholds: Thresholds,
    /// `[wasm_limits]`
    #[serde(default)]
    pub wasm_limits: WasmLimits,
    /// `[report]`
    #[serde(default)]
    pub report: Report,
}

/// `[project]`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Baseline file path.
    #[serde(default = "default_baseline")]
    pub baseline: String,
    /// Scenario file globs.
    #[serde(default = "default_scenarios")]
    pub scenarios: Vec<String>,
}

fn default_baseline() -> String {
    ".soroban-gauge/baseline.json".into()
}
fn default_scenarios() -> Vec<String> {
    vec!["scenarios/*.toml".into()]
}

impl Default for Project {
    fn default() -> Self {
        Project {
            baseline: default_baseline(),
            scenarios: default_scenarios(),
        }
    }
}

/// `[[contract]]`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCfg {
    /// Alias used by scenarios.
    pub name: String,
    /// Path to the compiled `.wasm`.
    pub wasm: String,
    /// Named constructor arguments, if the contract has a constructor.
    #[serde(default)]
    pub constructor: Option<BTreeMap<String, toml::Value>>,
}

/// `[thresholds]`. A metric fails only when it exceeds both its `_pct` and
/// `_abs` limits; when only one of the pair is set, that one decides.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Thresholds {
    /// Percent limit for CPU instructions.
    pub instructions_pct: Option<f64>,
    /// Absolute limit for CPU instructions.
    pub instructions_abs: Option<u64>,
    /// Percent limit for memory bytes.
    pub mem_bytes_pct: Option<f64>,
    /// Absolute limit for memory bytes.
    pub mem_bytes_abs: Option<u64>,
    /// Percent limit for the estimated fee.
    pub fee_stroops_pct: Option<f64>,
    /// Absolute limit for the estimated fee.
    pub fee_stroops_abs: Option<u64>,
    /// Percent limit for read entries (disk and memory).
    pub read_entries_pct: Option<f64>,
    /// Absolute limit for read entries (disk and memory).
    pub read_entries_abs: Option<u64>,
    /// Percent limit for written entries.
    pub write_entries_pct: Option<f64>,
    /// Absolute limit for written entries.
    pub write_entries_abs: Option<u64>,
    /// Percent limit for written bytes.
    pub write_bytes_pct: Option<f64>,
    /// Absolute limit for written bytes.
    pub write_bytes_abs: Option<u64>,
    /// Percent limit for disk-read bytes (unset = report only).
    pub disk_read_bytes_pct: Option<f64>,
    /// Absolute limit for disk-read bytes (unset = report only).
    pub disk_read_bytes_abs: Option<u64>,
    /// Percent limit for event bytes (unset = report only).
    pub events_bytes_pct: Option<f64>,
    /// Absolute limit for event bytes (unset = report only).
    pub events_bytes_abs: Option<u64>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            instructions_pct: Some(5.0),
            instructions_abs: Some(10_000),
            mem_bytes_pct: Some(5.0),
            mem_bytes_abs: Some(1024),
            fee_stroops_pct: Some(5.0),
            fee_stroops_abs: Some(100),
            read_entries_pct: None,
            read_entries_abs: Some(0),
            write_entries_pct: None,
            write_entries_abs: Some(0),
            write_bytes_pct: Some(5.0),
            write_bytes_abs: Some(64),
            disk_read_bytes_pct: None,
            disk_read_bytes_abs: None,
            events_bytes_pct: None,
            events_bytes_abs: None,
        }
    }
}

/// `[wasm_limits]`; unset means report only.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WasmLimits {
    /// Hard maximum Wasm size.
    pub max_size_bytes: Option<u64>,
    /// Warning threshold.
    pub warn_size_bytes: Option<u64>,
}

/// Row ordering in reports.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Largest regressions first.
    Regression,
    /// Alphabetical by key.
    Name,
    /// Highest current instruction count first.
    Cost,
}

/// `[report]`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// Rows shown before collapsing the rest.
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
    /// Ordering.
    #[serde(default = "default_sort")]
    pub sort: SortOrder,
}

fn default_max_rows() -> usize {
    50
}
fn default_sort() -> SortOrder {
    SortOrder::Regression
}

impl Default for Report {
    fn default() -> Self {
        Report {
            max_rows: default_max_rows(),
            sort: default_sort(),
        }
    }
}

/// A loaded config plus the directory its relative paths resolve against.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// Parsed config.
    pub config: Config,
    /// Directory containing the config file.
    pub base_dir: PathBuf,
}

impl Config {
    /// Parses config text, rejecting unknown keys and duplicate contract names.
    pub fn parse(text: &str) -> Result<Config, CoreError> {
        let cfg: Config =
            toml::from_str(text).map_err(|e| CoreError::Config(format!("invalid config: {e}")))?;
        let mut seen = std::collections::BTreeSet::new();
        for c in &cfg.contracts {
            if !seen.insert(&c.name) {
                return Err(CoreError::Config(format!(
                    "invalid config: duplicate [[contract]] name `{}`",
                    c.name
                )));
            }
        }
        Ok(cfg)
    }

    /// Loads a config file.
    pub fn load(path: &Path) -> Result<LoadedConfig, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::Config(format!(
                    "config file not found: {} (run `soroban-gauge init`)",
                    path.display()
                ))
            } else {
                CoreError::io(path, e)
            }
        })?;
        let config = Config::parse(&text)?;
        let base_dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(LoadedConfig { config, base_dir })
    }
}

impl LoadedConfig {
    /// Resolves a config-relative path.
    pub fn resolve(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        }
    }

    /// Baseline file path.
    pub fn baseline_path(&self) -> PathBuf {
        self.resolve(&self.config.project.baseline)
    }

    /// Expands scenario globs into a sorted, de-duplicated file list.
    pub fn scenario_files(&self) -> Result<Vec<PathBuf>, CoreError> {
        let mut files = std::collections::BTreeSet::new();
        for pat in &self.config.project.scenarios {
            let full = self.resolve(pat);
            let pattern = full.to_string_lossy().to_string();
            let paths = glob::glob(&pattern)
                .map_err(|e| CoreError::Config(format!("bad scenarios glob `{pat}`: {e}")))?;
            for p in paths {
                let p = p.map_err(|e| CoreError::Io(e.to_string()))?;
                files.insert(p);
            }
        }
        Ok(files.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_uses_defaults() {
        let c = Config::parse("[[contract]]\nname = \"a\"\nwasm = \"a.wasm\"\n").unwrap();
        assert_eq!(c.project.baseline, ".soroban-gauge/baseline.json");
        assert_eq!(c.thresholds, Thresholds::default());
        assert_eq!(c.report.max_rows, 50);
    }

    #[test]
    fn unknown_keys_are_errors() {
        assert!(Config::parse("bogus = 1").is_err());
        assert!(Config::parse("[thresholds]\ninstructions_pcnt = 1.0").is_err());
        assert!(Config::parse("[[contract]]\nname=\"a\"\nwasm=\"a\"\nextra=1").is_err());
    }

    #[test]
    fn partial_thresholds_keep_other_defaults() {
        let c = Config::parse("[thresholds]\ninstructions_pct = 1.5").unwrap();
        assert_eq!(c.thresholds.instructions_pct, Some(1.5));
        assert_eq!(c.thresholds.instructions_abs, Some(10_000));
    }

    #[test]
    fn duplicate_contract_names_rejected() {
        let t = "[[contract]]\nname=\"a\"\nwasm=\"a\"\n[[contract]]\nname=\"a\"\nwasm=\"b\"\n";
        assert!(Config::parse(t).is_err());
    }

    #[test]
    fn constructor_table_parses() {
        let t = "[[contract]]\nname=\"t\"\nwasm=\"t\"\nconstructor = { admin = \"@admin\", decimals = 7 }\n";
        let c = Config::parse(t).unwrap();
        let ctor = c.contracts[0].constructor.as_ref().unwrap();
        assert_eq!(ctor.len(), 2);
    }
}
