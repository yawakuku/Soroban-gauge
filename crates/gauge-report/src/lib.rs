//! Diff engine and renderers for soroban-gauge result files.
//!
//! The comparison logic in [`diff`] is made of pure functions: no I/O, no
//! clocks, no randomness. All arithmetic uses `i128`/`u128`, so no input can
//! overflow it.
#![forbid(unsafe_code)]

mod engine;
mod render;

pub use engine::{
    diff, metric_fails, pct_change, DiffOptions, DiffReport, FailOn, MetricDiff, RowDiff,
    SizeFinding, Status, WasmDelta, DIFF_SCHEMA_VERSION,
};
pub use render::{render_json, render_markdown, render_table, RenderOptions, MARKER};
