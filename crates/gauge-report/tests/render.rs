#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::*;
use gauge_core::config::SortOrder;
use gauge_core::Thresholds;
use gauge_report::{
    diff, render_json, render_markdown, render_table, DiffOptions, FailOn, RenderOptions, MARKER,
};

fn sample() -> gauge_report::DiffReport {
    let base = baseline(
        "aaaa",
        4048,
        vec![
            entry("counter/get", metrics(120_000, 3_000, 0, 150)),
            entry("counter/increment", metrics(310_000, 9_000, 1, 480)),
            entry("counter/old", metrics(1_000, 100, 0, 10)),
        ],
    );
    let cur = baseline(
        "bbbb",
        4142,
        vec![
            entry("counter/get", metrics(120_000, 3_000, 0, 150)),
            entry("counter/increment", metrics(1_310_000, 19_000, 4, 1_480)),
            entry("counter/new", metrics(2_000, 200, 0, 20)),
        ],
    );
    diff(
        &base,
        &cur,
        &DiffOptions {
            thresholds: Thresholds::default(),
            fail_on: FailOn::Regression,
            sort: SortOrder::Regression,
        },
    )
}

#[test]
fn markdown_starts_with_marker_and_names_regression() {
    let md = render_markdown(&sample(), RenderOptions { max_rows: 50 });
    assert!(md.starts_with(&format!("{MARKER}\n")));
    assert!(md.contains("❌ 1 regression"));
    assert!(md.contains("`counter/increment`"));
    assert!(md.contains("<details>") && md.contains("</details>"));
    assert!(md.contains("Wasm size:"));
    assert!(md.contains("soroban-env-host 29.0.1"));
    insta::assert_snapshot!("markdown", md);
}

#[test]
fn markdown_collapses_rows_beyond_max_rows() {
    let md = render_markdown(&sample(), RenderOptions { max_rows: 1 });
    assert!(md.contains("more changed rows"));
}

#[test]
fn clean_diff_says_no_regressions() {
    let b = baseline("aa", 10, vec![entry("s/x", metrics(1, 1, 1, 1))]);
    let d = diff(
        &b,
        &b,
        &DiffOptions {
            thresholds: Thresholds::default(),
            fail_on: FailOn::Regression,
            sort: SortOrder::Name,
        },
    );
    let md = render_markdown(&d, RenderOptions { max_rows: 50 });
    assert!(md.contains("✅ no regressions"));
    assert!(md.contains("No metric changes"));
}

#[test]
fn table_snapshot() {
    insta::assert_snapshot!(
        "table",
        render_table(&sample(), RenderOptions { max_rows: 50 })
    );
}

#[test]
fn json_snapshot_has_schema_version() {
    let j = render_json(&sample()).unwrap();
    assert!(j.contains("\"schema_version\": 1"));
    insta::assert_snapshot!("json", j);
}
