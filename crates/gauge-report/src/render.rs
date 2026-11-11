//! Renderers: plain table, Markdown (PR comment body) and JSON.
use std::fmt::Write;

use crate::engine::{DiffReport, MetricDiff, RowDiff, Status};

/// Hidden marker that identifies the PR comment this tool owns.
pub const MARKER: &str = "<!-- soroban-gauge:v1 -->";

/// Rendering options.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Rows shown before the rest are collapsed.
    pub max_rows: usize,
}

/// Comments larger than this are cut before the limit GitHub enforces (65,536).
const MAX_COMMENT_CHARS: usize = 55_000;

fn group(digits: &str) -> String {
    let chunks: Vec<&str> = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .filter_map(|c| std::str::from_utf8(c).ok())
        .collect();
    chunks.join(",")
}

fn num(n: u64) -> String {
    group(&n.to_string())
}

fn signed(d: i128) -> String {
    let sign = if d > 0 {
        "+"
    } else if d < 0 {
        "-"
    } else {
        ""
    };
    format!("{sign}{}", group(&d.unsigned_abs().to_string()))
}

fn pct(m: &MetricDiff) -> String {
    if m.delta == 0 {
        return "0.0%".to_string();
    }
    match m.pct {
        Some(p) => format!("{p:+.1}%"),
        None if m.delta != 0 => "new".to_string(),
        None => "0.0%".to_string(),
    }
}

fn delta_cell(r: &RowDiff, name: &str) -> String {
    match r.metric(name) {
        Some(m) if m.delta == 0 => "0".to_string(),
        Some(m) => {
            let mark = if m.failed { " ❌" } else { "" };
            format!("{} ({}){mark}", signed(m.delta), pct(m))
        }
        None => "-".to_string(),
    }
}

fn status_cell(r: &RowDiff) -> String {
    if r.fails && r.status != Status::Regressed {
        format!("{} (over threshold)", r.status.label())
    } else {
        r.status.label().to_string()
    }
}

/// Aligned plain text for terminals.
pub fn render_table(d: &DiffReport, opts: RenderOptions) -> String {
    let head = ["KEY", "INSTRUCTIONS", "MEMORY", "FEE", "WRITES", "STATUS"];
    let shown: Vec<&RowDiff> = d.rows.iter().filter(|r| r.changed).collect();
    let mut cells: Vec<[String; 6]> = Vec::new();
    for r in shown.iter().take(opts.max_rows) {
        cells.push([
            r.key.clone(),
            delta_cell(r, "instructions"),
            delta_cell(r, "mem_bytes"),
            delta_cell(r, "fee_stroops"),
            delta_cell(r, "write_entries"),
            status_cell(r),
        ]);
    }
    let mut out = String::new();
    let verdict = verdict_text(d);
    let _ = writeln!(out, "{verdict}");
    if cells.is_empty() {
        let _ = writeln!(out, "No metric changes ({} entries compared).", d.total);
    } else {
        let mut widths: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
        for row in &cells {
            for (i, c) in row.iter().enumerate() {
                widths[i] = widths[i].max(c.chars().count());
            }
        }
        let line = |cols: Vec<&str>| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| format!("{c:<w$}", w = widths[i]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };
        let _ = writeln!(out, "{}", line(head.to_vec()));
        for row in &cells {
            let _ = writeln!(out, "{}", line(row.iter().map(String::as_str).collect()));
        }
        if shown.len() > opts.max_rows {
            let _ = writeln!(
                out,
                "... and {} more changed rows",
                shown.len() - opts.max_rows
            );
        }
        let _ = writeln!(out, "{} of {} entries changed.", d.changed, d.total);
    }
    for w in d.wasm.iter().filter(|w| w.changed) {
        let _ = writeln!(out, "wasm {}: {}", w.contract, wasm_change(w));
    }
    for f in &d.size_findings {
        let _ = writeln!(
            out,
            "wasm size {}: {} {} B over limit {} B",
            f.contract,
            f.status,
            num(f.size_bytes),
            num(f.limit_bytes)
        );
    }
    if d.environment_mismatch {
        let _ = writeln!(
            out,
            "warning: baseline and current host versions differ; numbers are not comparable."
        );
    }
    out
}

fn size_failed(d: &DiffReport) -> bool {
    d.size_findings.iter().any(|f| f.status == "fail")
}

fn verdict_text(d: &DiffReport) -> String {
    if d.regressions > 0 {
        let s = if d.regressions == 1 { "" } else { "s" };
        format!("FAIL: {} regression{s} over threshold", d.regressions)
    } else if size_failed(d) {
        "FAIL: wasm size over the configured maximum".to_string()
    } else if d.failed {
        "FAIL: changes detected (--fail-on any-change)".to_string()
    } else {
        "OK: no regressions".to_string()
    }
}

fn wasm_change(w: &crate::engine::WasmDelta) -> String {
    match (w.baseline_size_bytes, w.current_size_bytes) {
        (Some(b), Some(c)) => {
            let d = i128::from(c) - i128::from(b);
            format!("{} B -> {} B ({} B)", num(b), num(c), signed(d))
        }
        (None, Some(c)) => format!("new, {} B", num(c)),
        (Some(b), None) => format!("removed (was {} B)", num(b)),
        (None, None) => "-".to_string(),
    }
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

fn md_row(r: &RowDiff) -> String {
    format!(
        "| `{}` | {} | {} | {} | {} | {} |\n",
        md_escape(&r.key),
        delta_cell(r, "instructions"),
        delta_cell(r, "mem_bytes"),
        delta_cell(r, "fee_stroops"),
        delta_cell(r, "write_entries"),
        status_cell(r)
    )
}

const MD_HEAD: &str = "| Function | Instructions Δ | Memory Δ | Fee Δ | Writes Δ | Status |\n|---|---:|---:|---:|---:|---|\n";

/// The PR comment body.
pub fn render_markdown(d: &DiffReport, opts: RenderOptions) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{MARKER}");
    let icon = if d.failed { "❌" } else { "✅" };
    let line = if d.regressions > 0 {
        let s = if d.regressions == 1 { "" } else { "s" };
        format!("{} regression{s}", d.regressions)
    } else if size_failed(d) {
        "wasm size over the configured maximum".to_string()
    } else if d.failed {
        "changes detected".to_string()
    } else {
        "no regressions".to_string()
    };
    let _ = writeln!(out, "## soroban-gauge: {icon} {line}\n");
    if d.environment_mismatch {
        let _ = writeln!(
            out,
            "> ⚠️ **Host versions differ** between the baseline (`soroban-sdk {}`, protocol {}) and this run (`soroban-sdk {}`, protocol {}). Costs are not comparable; re-run `soroban-gauge snapshot`.\n",
            d.baseline_environment.soroban_sdk_version,
            d.baseline_environment.protocol_version,
            d.current_environment.soroban_sdk_version,
            d.current_environment.protocol_version
        );
    }

    let changed: Vec<&RowDiff> = d.rows.iter().filter(|r| r.changed).collect();
    if changed.is_empty() {
        let _ = writeln!(
            out,
            "No metric changes against the baseline ({} entries compared).\n",
            d.total
        );
    } else {
        let _ = writeln!(out, "{} of {} entries changed.\n", changed.len(), d.total);
        out.push_str(MD_HEAD);
        for r in changed.iter().take(opts.max_rows) {
            out.push_str(&md_row(r));
        }
        if changed.len() > opts.max_rows {
            let rest = &changed[opts.max_rows..];
            let _ = writeln!(
                out,
                "\n<details><summary>{} more changed rows</summary>\n",
                rest.len()
            );
            out.push_str(MD_HEAD);
            for r in rest {
                out.push_str(&md_row(r));
            }
            out.push_str("\n</details>\n");
        }
        out.push('\n');
        let _ = writeln!(out, "<details><summary>Full metric breakdown</summary>\n");
        for r in &changed {
            if out.len() > MAX_COMMENT_CHARS {
                let _ = writeln!(out, "_Breakdown truncated to fit the comment size limit; see the `report.json` artifact._\n");
                break;
            }
            let _ = writeln!(out, "**`{}`** — {}\n", md_escape(&r.key), status_cell(r));
            out.push_str("| Metric | Baseline | Current | Δ | % |\n|---|---:|---:|---:|---:|\n");
            for m in &r.metrics {
                let mark = if m.failed { " ❌" } else { "" };
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {}{mark} | {} |",
                    m.name,
                    num(m.baseline),
                    num(m.current),
                    signed(m.delta),
                    pct(m)
                );
            }
            out.push('\n');
        }
        out.push_str("</details>\n\n");
    }

    if !d.wasm.is_empty() {
        let parts: Vec<String> = d
            .wasm
            .iter()
            .map(|w| format!("`{}` {}", w.contract, wasm_change(w)))
            .collect();
        let _ = writeln!(out, "Wasm size: {}\n", parts.join("; "));
    }
    for f in &d.size_findings {
        let icon = if f.status == "fail" { "❌" } else { "⚠️" };
        let _ = writeln!(
            out,
            "{icon} `{}` is {} B, over the {} limit of {} B.\n",
            f.contract,
            num(f.size_bytes),
            if f.status == "fail" {
                "maximum"
            } else {
                "warning"
            },
            num(f.limit_bytes)
        );
    }
    let _ = writeln!(
        out,
        "<sub>Estimates from soroban-env-host {} / protocol {} on a local host VM, not exact network fees. See docs/limitations.md.</sub>",
        d.current_environment.soroban_env_host_version, d.current_environment.protocol_version
    );
    out
}

/// Machine-readable diff.
pub fn render_json(d: &DiffReport) -> Result<String, serde_json::Error> {
    let mut s = serde_json::to_string_pretty(d)?;
    s.push('\n');
    Ok(s)
}
