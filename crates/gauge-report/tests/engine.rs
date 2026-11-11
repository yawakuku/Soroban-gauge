#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use common::*;
use gauge_core::config::SortOrder;
use gauge_core::Thresholds;
use gauge_report::{diff, metric_fails, pct_change, DiffOptions, FailOn, Status};
use proptest::prelude::*;

fn opts(fail_on: FailOn) -> DiffOptions {
    DiffOptions {
        thresholds: Thresholds::default(),
        fail_on,
        sort: SortOrder::Regression,
    }
}

#[test]
fn identical_runs_are_unchanged_and_pass() {
    let b = baseline("aa", 100, vec![entry("s/inc", metrics(1000, 500, 1, 200))]);
    let d = diff(&b, &b, &opts(FailOn::AnyChange));
    assert_eq!(d.rows[0].status, Status::Unchanged);
    assert!(!d.failed && d.regressions == 0 && d.changed == 0);
}

#[test]
fn regression_needs_both_pct_and_abs() {
    let t = Thresholds::default(); // instructions: 5% and 10,000
                                   // +50% but only +500 instructions: abs not exceeded -> pass.
    assert!(!metric_fails("instructions", 1_000, 1_500, &t));
    // +20,000 but only +2%: pct not exceeded -> pass.
    assert!(!metric_fails("instructions", 1_000_000, 1_020_000, &t));
    // +30% and +300,000: both exceeded -> fail.
    assert!(metric_fails("instructions", 1_000_000, 1_300_000, &t));
}

#[test]
fn any_write_entry_increase_fails_by_default() {
    let t = Thresholds::default();
    assert!(metric_fails("write_entries", 1, 2, &t));
    assert!(!metric_fails("write_entries", 2, 2, &t));
}

#[test]
fn single_configured_limit_decides() {
    let t = Thresholds {
        instructions_abs: None,
        instructions_pct: Some(10.0),
        ..Thresholds::default()
    };
    assert!(metric_fails("instructions", 100, 111, &t));
    assert!(!metric_fails("instructions", 100, 110, &t));
}

#[test]
fn unconfigured_metrics_never_fail() {
    let t = Thresholds::default();
    assert!(!metric_fails("events_bytes", 0, 1_000_000, &t));
}

#[test]
fn zero_baseline_is_an_infinite_increase() {
    let pct_only = Thresholds {
        fee_stroops_abs: None,
        fee_stroops_pct: Some(1000.0),
        ..Thresholds::default()
    };
    assert!(metric_fails("fee_stroops", 0, 1, &pct_only));
    assert_eq!(pct_change(0, 5), None);
}

#[test]
fn statuses_new_removed_improved_drift() {
    let base = baseline(
        "aa",
        100,
        vec![
            entry("s/gone", metrics(10, 10, 1, 10)),
            entry("s/better", metrics(100_000, 500, 2, 400)),
            entry("s/same", metrics(5, 5, 1, 5)),
        ],
    );
    let cur = baseline(
        "bb",
        120,
        vec![
            entry("s/better", metrics(50_000, 500, 2, 300)),
            entry("s/fresh", metrics(7, 7, 1, 7)),
            entry("s/same", metrics(5, 5, 1, 5)),
        ],
    );
    let d = diff(&base, &cur, &opts(FailOn::Regression));
    let st = |k: &str| d.rows.iter().find(|r| r.key == k).unwrap().status;
    assert_eq!(st("s/gone"), Status::Removed);
    assert_eq!(st("s/fresh"), Status::New);
    assert_eq!(st("s/better"), Status::Improved);
    assert_eq!(st("s/same"), Status::Unchanged);
    assert!(!d.failed, "new/removed never fail by default");
    assert!(diff(&base, &cur, &opts(FailOn::AnyChange)).failed);
    assert!(d.wasm[0].changed);

    // Same wasm hash but different numbers => environment drift.
    let drift_cur = baseline(
        "aa",
        100,
        vec![entry("s/better", metrics(50_000, 500, 2, 300))],
    );
    let drift_base = baseline(
        "aa",
        100,
        vec![entry("s/better", metrics(100_000, 500, 2, 400))],
    );
    let d2 = diff(&drift_base, &drift_cur, &opts(FailOn::Regression));
    assert_eq!(d2.rows[0].status, Status::EnvironmentDrift);
}

#[test]
fn regressed_rows_sort_first_and_verdict_fails() {
    let base = baseline(
        "aa",
        100,
        vec![
            entry("a/ok", metrics(1_000_000, 500, 1, 200)),
            entry("b/bad", metrics(1_000_000, 500, 1, 200)),
        ],
    );
    let cur = baseline(
        "bb",
        100,
        vec![
            entry("a/ok", metrics(1_000_000, 500, 1, 200)),
            entry("b/bad", metrics(2_000_000, 500, 3, 900)),
        ],
    );
    let d = diff(&base, &cur, &opts(FailOn::Regression));
    assert_eq!(d.rows[0].key, "b/bad");
    assert_eq!(d.rows[0].status, Status::Regressed);
    assert!(d.failed && d.regressions == 1);
    assert!(!diff(&base, &cur, &opts(FailOn::Never)).failed);
}

#[test]
fn environment_mismatch_is_flagged() {
    let a = baseline("aa", 1, vec![]);
    let mut b = baseline("aa", 1, vec![]);
    b.environment.protocol_version = 30;
    assert!(diff(&a, &b, &opts(FailOn::Regression)).environment_mismatch);
    assert!(!diff(&a, &a, &opts(FailOn::Regression)).environment_mismatch);
}

fn any_metrics() -> impl Strategy<Value = gauge_core::Metrics> {
    (any::<u64>(), any::<u64>(), any::<u32>(), any::<u64>())
        .prop_map(|(i, m, w, f)| metrics(i, m, u64::from(w), f))
}

proptest! {
    #[test]
    fn decreases_and_equal_never_fail(b in any::<u64>(), d in any::<u64>()) {
        let cur = b.saturating_sub(d);
        for name in gauge_core::Metrics::NAMES {
            prop_assert!(!metric_fails(name, b, cur, &Thresholds::default()));
        }
    }

    #[test]
    fn swapping_sides_never_fails_both_ways(a in any::<u64>(), b in any::<u64>()) {
        for name in gauge_core::Metrics::NAMES {
            let t = Thresholds::default();
            prop_assert!(!(metric_fails(name, a, b, &t) && metric_fails(name, b, a, &t)));
        }
    }

    #[test]
    fn extreme_values_do_not_overflow(a in any_metrics(), b in any_metrics()) {
        let base = baseline("aa", 1, vec![entry("s/x", a)]);
        let cur = baseline("bb", 2, vec![entry("s/x", b)]);
        let d = diff(&base, &cur, &opts(FailOn::Regression));
        prop_assert_eq!(d.rows.len(), 1);
        for m in &d.rows[0].metrics {
            prop_assert_eq!(m.delta, i128::from(m.current) - i128::from(m.baseline));
        }
        prop_assert!(gauge_report::render_json(&d).is_ok());
        let _ = gauge_report::render_markdown(&d, gauge_report::RenderOptions { max_rows: 5 });
    }

    #[test]
    fn diff_against_self_is_clean(a in any_metrics()) {
        let base = baseline("aa", 1, vec![entry("s/x", a)]);
        let d = diff(&base, &base, &opts(FailOn::AnyChange));
        prop_assert!(!d.failed);
        prop_assert_eq!(d.rows[0].status, Status::Unchanged);
    }

    #[test]
    fn row_order_is_independent_of_input_order(ms in proptest::collection::vec(any_metrics(), 1..6)) {
        let forward: Vec<_> = ms.iter().enumerate().map(|(i, m)| entry(&format!("s/k{i}"), m.clone())).collect();
        let mut reversed = forward.clone();
        reversed.reverse();
        let cur: Vec<_> = ms.iter().enumerate().map(|(i, m)| {
            let mut m = m.clone();
            m.instructions = m.instructions.saturating_add(i as u64 * 1000);
            entry(&format!("s/k{i}"), m)
        }).collect();
        let a = diff(&baseline("aa", 1, forward), &baseline("bb", 1, cur.clone()), &opts(FailOn::Regression));
        let b = diff(&baseline("aa", 1, reversed), &baseline("bb", 1, cur), &opts(FailOn::Regression));
        prop_assert_eq!(a, b);
    }
}
