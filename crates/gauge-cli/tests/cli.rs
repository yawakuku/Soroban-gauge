#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn schema(name: &str) -> serde_json::Value {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../schemas/{name}.schema.json"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn assert_valid(schema_name: &str, instance: &serde_json::Value) {
    let validator = jsonschema::validator_for(&schema(schema_name)).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "schema violations: {errors:?}");
}

fn gauge(dir: &Path) -> Command {
    let mut c = Command::cargo_bin("soroban-gauge").unwrap();
    c.current_dir(dir).env_remove("NO_COLOR");
    c
}

const COUNTER_SCENARIOS: &str = r#"
[[scenario]]
name = "counter_basic"
contracts = ["counter"]
  [[scenario.step]]
  id = "increment"
  contract = "counter"
  function = "increment"
  repeat = 2
  [[scenario.step]]
  id = "get"
  contract = "counter"
  function = "get"
"#;

/// A project whose `counter` contract is the given fixture.
fn counter_project(fixture: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(fixtures().join(fixture), dir.path().join("counter.wasm")).unwrap();
    std::fs::write(
        dir.path().join("soroban-gauge.toml"),
        "[[contract]]\nname = \"counter\"\nwasm = \"counter.wasm\"\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("scenarios")).unwrap();
    std::fs::write(dir.path().join("scenarios/counter.toml"), COUNTER_SCENARIOS).unwrap();
    dir
}

fn token_project(scenarios: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(fixtures().join("token.wasm"), dir.path().join("token.wasm")).unwrap();
    std::fs::write(
        dir.path().join("soroban-gauge.toml"),
        "[[contract]]\nname = \"token\"\nwasm = \"token.wasm\"\nconstructor = { admin = \"@admin\" }\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("scenarios")).unwrap();
    std::fs::write(dir.path().join("scenarios/token.toml"), scenarios).unwrap();
    dir
}

fn baseline_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(".soroban-gauge/baseline.json")).unwrap()
}

#[test]
fn init_creates_files_and_refuses_to_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    gauge(dir.path()).arg("init").assert().success();
    assert!(dir.path().join("soroban-gauge.toml").exists());
    assert!(dir.path().join("scenarios/example.toml").exists());
    gauge(dir.path())
        .arg("init")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already exists"));
    gauge(dir.path())
        .args(["init", "--force"])
        .assert()
        .success();

    let cfg: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.path().join("soroban-gauge.toml")).unwrap())
            .unwrap();
    assert_valid("config", &serde_json::to_value(cfg).unwrap());
}

#[test]
fn inspect_table_matches_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let out = gauge(dir.path())
        .args(["inspect"])
        .arg(fixtures().join("counter.wasm"))
        .output()
        .unwrap();
    assert!(out.status.success());
    insta::assert_snapshot!("inspect_counter", String::from_utf8(out.stdout).unwrap());
}

#[test]
fn inspect_json_is_valid_json_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let out = gauge(dir.path())
        .args(["inspect", "--format", "json"])
        .arg(fixtures().join("token.wasm"))
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["size_limit"], "ok");
    assert!(v["spec_functions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "transfer"));
}

#[test]
fn inspect_rejects_non_wasm_with_exit_5() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.wasm"), b"nope").unwrap();
    gauge(dir.path())
        .args(["inspect", "x.wasm"])
        .assert()
        .code(5);
}

#[test]
fn snapshot_then_check_passes() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    gauge(dir.path())
        .arg("check")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("OK: no regressions"));
}

#[test]
fn snapshot_twice_is_byte_identical_and_schema_valid() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    let first = baseline_text(dir.path());
    gauge(dir.path()).arg("snapshot").assert().success();
    assert_eq!(first, baseline_text(dir.path()));
    assert!(first.ends_with("}\n"));
    assert!(!first.contains("timestamp"));
    assert!(
        !first.contains(dir.path().to_str().unwrap()),
        "no absolute paths in baselines"
    );
    let v: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_valid("baseline", &v);
    let keys: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "counter_basic/get",
            "counter_basic/increment#1",
            "counter_basic/increment#2"
        ]
    );
}

#[test]
fn run_report_is_schema_valid_and_includes_wasm_facts() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("run").assert().success();
    let text =
        std::fs::read_to_string(dir.path().join("target/soroban-gauge/report.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_valid("baseline", &v);
    assert!(v["wasm"]["counter"]["sha256"].is_string());
}

#[test]
fn regressed_contract_fails_check_and_names_the_function() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    // Swap in the deliberately more expensive implementation with the same interface.
    std::fs::copy(
        fixtures().join("counter_regressed.wasm"),
        dir.path().join("counter.wasm"),
    )
    .unwrap();
    let out = gauge(dir.path())
        .args(["check", "--format", "markdown"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let md = String::from_utf8(out.stdout).unwrap();
    assert!(md.starts_with("<!-- soroban-gauge:v1 -->\n"));
    assert!(md.contains("❌"));
    assert!(md.contains("counter_basic/increment#1"), "{md}");
    assert!(md.contains("regressed"));
    assert!(md.contains("Wasm size:"));
}

#[test]
fn fail_on_never_exits_zero_even_with_regression() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    std::fs::copy(
        fixtures().join("counter_regressed.wasm"),
        dir.path().join("counter.wasm"),
    )
    .unwrap();
    gauge(dir.path())
        .args(["check", "--fail-on", "never"])
        .assert()
        .code(0);
}

#[test]
fn json_output_is_machine_readable_on_stdout_only() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    let out = gauge(dir.path())
        .args(["check", "--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["failed"], false);
}

#[test]
fn environment_mismatch_exits_4_unless_allowed() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    let path = dir.path().join(".soroban-gauge/baseline.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["environment"]["protocol_version"] = serde_json::json!(1);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    gauge(dir.path())
        .arg("check")
        .assert()
        .code(4)
        .stderr(predicate::str::contains(
            "not comparable across host versions",
        ));
    gauge(dir.path())
        .args(["check", "--allow-version-mismatch"])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning"));
}

#[test]
fn diff_command_compares_two_files() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    gauge(dir.path()).arg("run").assert().success();
    gauge(dir.path())
        .args([
            "diff",
            "--baseline",
            ".soroban-gauge/baseline.json",
            "--current",
            "target/soroban-gauge/report.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("no regressions"));
}

const TOKEN_OK: &str = r#"
[[scenario]]
name = "tok"
contracts = ["token"]
  [[scenario.step]]
  id = "mint"
  contract = "token"
  function = "mint"
  args = { to = "@alice", amount = "1000" }
  measure = false
  [[scenario.step]]
  id = "transfer"
  contract = "token"
  function = "transfer"
  args = { from = "@alice", to = "@bob", amount = "250" }
"#;

#[test]
fn token_scenario_with_constructor_snapshots() {
    let dir = token_project(TOKEN_OK);
    gauge(dir.path()).arg("snapshot").assert().success();
    assert!(baseline_text(dir.path()).contains("tok/transfer"));
}

#[test]
fn wrong_argument_type_exits_2_with_precise_message() {
    let dir = token_project(&TOKEN_OK.replace("amount = \"250\"", "amount = true"));
    gauge(dir.path())
        .arg("snapshot")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "scenario `tok`, step `transfer`: argument `amount`: expected i128, got boolean true",
        ));
}

#[test]
fn unmet_expectation_exits_3() {
    let dir = token_project(&format!(
        "{TOKEN_OK}\n  [[scenario.step]]\n  id = \"overdraw\"\n  contract = \"token\"\n  function = \"transfer\"\n  args = {{ from = \"@alice\", to = \"@bob\", amount = \"999999\" }}\n  expect = \"ok\"\n  measure = false\n"
    ));
    gauge(dir.path()).arg("snapshot").assert().code(3).stderr(
        predicate::str::contains("overdraw").and(predicate::str::contains("contract error 2")),
    );
}

#[test]
fn snapshot_only_validates_names_and_keeps_other_entries() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    let before = baseline_text(dir.path());
    gauge(dir.path())
        .args(["snapshot", "--only", "counter_basic"])
        .assert()
        .success();
    assert_eq!(before, baseline_text(dir.path()));
    gauge(dir.path())
        .args(["snapshot", "--only", "nope"])
        .assert()
        .code(2);
}

#[test]
fn usage_and_config_errors_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    gauge(dir.path())
        .arg("check")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("config file not found"));
    gauge(dir.path())
        .arg("calibrate")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not implemented"));
    gauge(dir.path()).arg("--no-such-flag").assert().code(2);
    let p = counter_project("counter.wasm");
    gauge(p.path())
        .arg("check")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("baseline not found"));
}

#[test]
fn unknown_config_key_is_rejected() {
    let dir = counter_project("counter.wasm");
    std::fs::write(
        dir.path().join("soroban-gauge.toml"),
        "bogus = 1\n[[contract]]\nname = \"counter\"\nwasm = \"counter.wasm\"\n",
    )
    .unwrap();
    gauge(dir.path())
        .arg("snapshot")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("bogus"));
}

#[test]
fn wasm_size_limit_failure_exits_1() {
    let dir = counter_project("counter.wasm");
    gauge(dir.path()).arg("snapshot").assert().success();
    std::fs::write(
        dir.path().join("soroban-gauge.toml"),
        "[[contract]]\nname = \"counter\"\nwasm = \"counter.wasm\"\n[wasm_limits]\nmax_size_bytes = 100\n",
    )
    .unwrap();
    gauge(dir.path())
        .arg("check")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("over limit"));
}
