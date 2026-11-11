//! Scenario files: declarative lists of contract calls.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::CoreError;

/// A scenario file (`[[scenario]]` tables).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    /// Scenarios in file order.
    #[serde(default, rename = "scenario")]
    pub scenarios: Vec<Scenario>,
}

/// One scenario: fresh contracts plus an ordered list of steps.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Unique name; the first part of every entry key.
    pub name: String,
    /// Free text.
    #[serde(default)]
    pub description: Option<String>,
    /// Contract aliases deployed fresh for this scenario.
    pub contracts: Vec<String>,
    /// Steps in execution order.
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
}

/// Authorization mode for a step.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    /// All `require_auth` checks succeed (`mock_all_auths`).
    All,
    /// No authorization is provided.
    None,
}

/// What a step is expected to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// Succeeds (default).
    Ok,
    /// Fails with any error.
    Error,
    /// Fails with this contract error code.
    ErrorCode(u32),
}

impl<'de> Deserialize<'de> for Expect {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Word(String),
            Code { error: u32 },
        }
        match Raw::deserialize(d)? {
            Raw::Word(w) if w == "ok" => Ok(Expect::Ok),
            Raw::Word(w) if w == "error" => Ok(Expect::Error),
            Raw::Word(w) => Err(serde::de::Error::custom(format!(
                "expect must be \"ok\", \"error\" or {{ error = <code> }}, got \"{w}\""
            ))),
            Raw::Code { error } => Ok(Expect::ErrorCode(error)),
        }
    }
}

fn default_auth() -> Auth {
    Auth::All
}
fn default_measure() -> bool {
    true
}
fn default_expect() -> Expect {
    Expect::Ok
}

/// One contract call.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Unique within the scenario; the second part of the entry key.
    pub id: String,
    /// Contract alias (must be listed in the scenario's `contracts`).
    pub contract: String,
    /// Function name.
    pub function: String,
    /// Arguments keyed by the parameter names in the contract spec.
    #[serde(default)]
    pub args: BTreeMap<String, toml::Value>,
    /// Authorization mode (default `all`).
    #[serde(default = "default_auth")]
    pub auth: Auth,
    /// Record this call (default true). `false` runs it as setup.
    #[serde(default = "default_measure")]
    pub measure: bool,
    /// Expected outcome (default `ok`).
    #[serde(default = "default_expect")]
    pub expect: Expect,
    /// Run the call N times, recording `id#1..id#N`.
    #[serde(default)]
    pub repeat: Option<u32>,
}

/// Parses scenario text and validates names.
pub fn parse_scenarios(text: &str, origin: &str) -> Result<Vec<Scenario>, CoreError> {
    let file: ScenarioFile = toml::from_str(text)
        .map_err(|e| CoreError::Config(format!("invalid scenario file {origin}: {e}")))?;
    for s in &file.scenarios {
        let mut ids = BTreeSet::new();
        for st in &s.steps {
            if st.id.is_empty() || st.id.contains('/') || st.id.contains('#') {
                return Err(CoreError::Config(format!(
                    "{origin}: scenario `{}` step id `{}` must be non-empty and contain no `/` or `#`",
                    s.name, st.id
                )));
            }
            if !ids.insert(&st.id) {
                return Err(CoreError::Config(format!(
                    "{origin}: scenario `{}` has duplicate step id `{}`",
                    s.name, st.id
                )));
            }
            if st.repeat == Some(0) {
                return Err(CoreError::Config(format!(
                    "{origin}: scenario `{}` step `{}`: repeat must be at least 1",
                    s.name, st.id
                )));
            }
            if !s.contracts.contains(&st.contract) {
                return Err(CoreError::Config(format!(
                    "{origin}: scenario `{}` step `{}` uses contract `{}` which is not in its `contracts` list",
                    s.name, st.id, st.contract
                )));
            }
        }
        if s.name.is_empty() || s.name.contains('/') {
            return Err(CoreError::Config(format!(
                "{origin}: scenario name `{}` must be non-empty and contain no `/`",
                s.name
            )));
        }
    }
    Ok(file.scenarios)
}

/// Loads every scenario file; scenario names must be unique across files.
/// Order: file order (sorted paths), then scenarios in file order.
pub fn load_scenarios(files: &[std::path::PathBuf]) -> Result<Vec<Scenario>, CoreError> {
    let mut all = Vec::new();
    let mut names = BTreeSet::new();
    for f in files {
        let text = std::fs::read_to_string(f).map_err(|e| CoreError::io(f, e))?;
        for s in parse_scenarios(&text, &f.display().to_string())? {
            if !names.insert(s.name.clone()) {
                return Err(CoreError::Config(format!(
                    "duplicate scenario name `{}` (in {})",
                    s.name,
                    Path::new(f).display()
                )));
            }
            all.push(s);
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"
[[scenario]]
name = "s"
contracts = ["c"]
  [[scenario.step]]
  id = "a"
  contract = "c"
  function = "f"
  args = { x = 1 }
  expect = { error = 3 }
  repeat = 2
"#;

    #[test]
    fn parses_example() {
        let s = parse_scenarios(OK, "t").unwrap();
        assert_eq!(s[0].steps[0].expect, Expect::ErrorCode(3));
        assert_eq!(s[0].steps[0].auth, Auth::All);
        assert!(s[0].steps[0].measure);
    }

    #[test]
    fn rejects_bad_expect_and_unknown_contract_and_dupes() {
        assert!(parse_scenarios(&OK.replace("{ error = 3 }", "\"maybe\""), "t").is_err());
        assert!(parse_scenarios(&OK.replace("contract = \"c\"", "contract = \"z\""), "t").is_err());
        let dup = format!(
            "{OK}\n  [[scenario.step]]\n  id = \"a\"\n  contract = \"c\"\n  function = \"f\"\n"
        );
        assert!(parse_scenarios(&dup, "t").is_err());
        assert!(parse_scenarios(&OK.replace("repeat = 2", "repeat = 0"), "t").is_err());
        assert!(parse_scenarios(&OK.replace("id = \"a\"", "id = \"a#1\""), "t").is_err());
    }
}
