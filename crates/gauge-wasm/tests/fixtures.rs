#![allow(clippy::unwrap_used, clippy::expect_used)]
use gauge_wasm::{inspect, LimitStatus};

fn fixture(name: &str) -> Vec<u8> {
    let p = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(p).unwrap()
}

#[test]
fn counter_report_matches_contract() {
    let bytes = fixture("counter.wasm");
    let r = inspect(&bytes).unwrap();
    assert_eq!(r.size_bytes, bytes.len() as u64);
    assert_eq!(r.sha256.len(), 64);
    let names: Vec<&str> = r.spec_functions.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"increment") && names.contains(&"get"));
    let inc = r
        .spec_functions
        .iter()
        .find(|f| f.name == "increment")
        .unwrap();
    assert!(inc.inputs.is_empty());
    assert_eq!(inc.outputs, vec!["u32".to_string()]);
    assert!(r.exports.contains(&"increment".to_string()));
    assert!(
        r.spec_without_export.is_empty(),
        "{:?}",
        r.spec_without_export
    );
    assert!(r.env_meta.is_some());
    assert!(r.meta.contains_key("rssdkver"));
    assert!(r.imports.iter().all(|i| i.contains('.')));
    assert!(!r.sections.is_empty());
    // Sorted largest first.
    assert!(r.sections.windows(2).all(|w| w[0].bytes >= w[1].bytes));
}

#[test]
fn token_spec_has_typed_params() {
    let r = inspect(&fixture("token.wasm")).unwrap();
    let t = r
        .spec_functions
        .iter()
        .find(|f| f.name == "transfer")
        .unwrap();
    let got: Vec<(&str, &str)> = t
        .inputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![("from", "address"), ("to", "address"), ("amount", "i128")]
    );
}

#[test]
fn inspect_is_deterministic() {
    let b = fixture("token.wasm");
    assert_eq!(inspect(&b).unwrap(), inspect(&b).unwrap());
}

#[test]
fn limits() {
    let b = fixture("counter.wasm");
    let r = inspect(&b).unwrap();
    assert_eq!(
        gauge_wasm::check_limits(r.size_bytes, Some(1), None),
        LimitStatus::Fail
    );
}
