#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;

use gauge_core::runner::{run, ContractDef};
use gauge_core::scenario::parse_scenarios;
use gauge_core::CoreError;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn def(alias: &str, file: &str, ctor: Option<&str>) -> ContractDef {
    let wasm = fixture(file);
    let mut spec = Vec::new();
    for p in wasmparser::Parser::new(0).parse_all(&wasm) {
        if let wasmparser::Payload::CustomSection(c) = p.unwrap() {
            if c.name() == "contractspecv0" {
                spec = gauge_wasm::decode_spec(c.data()).unwrap();
            }
        }
    }
    ContractDef {
        name: alias.into(),
        wasm,
        spec,
        constructor: ctor.map(|t| toml::from_str::<BTreeMap<String, toml::Value>>(t).unwrap()),
    }
}

fn contracts() -> BTreeMap<String, ContractDef> {
    let mut m = BTreeMap::new();
    m.insert("counter".to_string(), def("counter", "counter.wasm", None));
    m.insert(
        "regressed".to_string(),
        def("regressed", "counter_regressed.wasm", None),
    );
    m.insert(
        "token".to_string(),
        def("token", "token.wasm", Some("admin = \"@admin\"")),
    );
    m
}

const COUNTER: &str = r#"
[[scenario]]
name = "single"
contracts = ["counter"]
  [[scenario.step]]
  id = "inc"
  contract = "counter"
  function = "increment"

[[scenario]]
name = "double"
contracts = ["counter"]
  [[scenario.step]]
  id = "first"
  contract = "counter"
  function = "increment"
  [[scenario.step]]
  id = "second"
  contract = "counter"
  function = "increment"
  [[scenario.step]]
  id = "get"
  contract = "counter"
  function = "get"
"#;

fn run_text(text: &str) -> Result<gauge_core::Baseline, CoreError> {
    run(&contracts(), &parse_scenarios(text, "test").unwrap(), None)
}

#[test]
fn measures_real_numbers_per_call() {
    let b = run_text(COUNTER).unwrap();
    let inc = b.entries.iter().find(|e| e.key == "single/inc").unwrap();
    assert!(inc.metrics.instructions > 0, "{:?}", inc.metrics);
    assert!(inc.metrics.write_entries >= 1, "{:?}", inc.metrics);
    assert!(inc.metrics.fee_stroops > 0);
    assert_eq!(inc.function, "increment");
}

#[test]
fn consecutive_calls_are_independent() {
    let b = run_text(COUNTER).unwrap();
    let single = &b
        .entries
        .iter()
        .find(|e| e.key == "single/inc")
        .unwrap()
        .metrics;
    let first = &b
        .entries
        .iter()
        .find(|e| e.key == "double/first")
        .unwrap()
        .metrics;
    let second = &b
        .entries
        .iter()
        .find(|e| e.key == "double/second")
        .unwrap()
        .metrics;
    // The first call in a scenario matches the same call run alone.
    assert_eq!(single, first);
    // The second call is a separate measurement, not an accumulation of the first.
    assert!(
        second.instructions < first.instructions * 2,
        "{second:?} vs {first:?}"
    );
    assert!(second.instructions > 0);
}

#[test]
fn runs_are_deterministic() {
    let a = run_text(COUNTER).unwrap().to_canonical_json().unwrap();
    let b = run_text(COUNTER).unwrap().to_canonical_json().unwrap();
    assert_eq!(a, b);
}

#[test]
fn regressed_contract_costs_more() {
    let text = COUNTER.replace("counter", "regressed");
    let b = run_text(&text).unwrap();
    let base = run_text(COUNTER).unwrap();
    let r = &b
        .entries
        .iter()
        .find(|e| e.key == "single/inc")
        .unwrap()
        .metrics;
    let c = &base
        .entries
        .iter()
        .find(|e| e.key == "single/inc")
        .unwrap()
        .metrics;
    assert!(r.instructions > c.instructions, "{r:?} vs {c:?}");
    assert!(r.write_entries > c.write_entries, "{r:?} vs {c:?}");
}

const TOKEN: &str = r#"
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
  repeat = 2
  [[scenario.step]]
  id = "overdraw"
  contract = "token"
  function = "transfer"
  args = { from = "@alice", to = "@bob", amount = "999999" }
  expect = { error = 2 }
  measure = false
  [[scenario.step]]
  id = "no_auth"
  contract = "token"
  function = "transfer"
  args = { from = "@alice", to = "@bob", amount = "1" }
  auth = "none"
  expect = "error"
  measure = false
"#;

#[test]
fn token_scenario_with_setup_repeat_and_expected_errors() {
    let b = run_text(TOKEN).unwrap();
    let keys: Vec<&str> = b.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["tok/transfer#1", "tok/transfer#2"]);
}

#[test]
fn wrong_expectation_is_a_scenario_failure() {
    let t = TOKEN.replace("expect = { error = 2 }", "expect = { error = 9 }");
    let e = run_text(&t).unwrap_err();
    assert!(matches!(e, CoreError::Scenario(_)), "{e}");
    assert_eq!(e.exit_code(), 3);
    assert!(e.to_string().contains("overdraw"), "{e}");
}

#[test]
fn wrong_argument_type_names_scenario_step_argument_and_type() {
    let t = TOKEN.replace("amount = \"250\"", "amount = true");
    let e = run_text(&t).unwrap_err();
    assert_eq!(e.exit_code(), 2);
    let m = e.to_string();
    assert!(
        m.contains("tok") && m.contains("transfer") && m.contains("amount") && m.contains("i128"),
        "{m}"
    );
}

#[test]
fn unknown_function_lists_available_ones() {
    let t = COUNTER.replace("function = \"get\"", "function = \"nope\"");
    let m = run_text(&t).unwrap_err().to_string();
    assert!(m.contains("nope") && m.contains("increment"), "{m}");
}
