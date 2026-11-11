//! Plain-text rendering of a [`WasmReport`](crate::WasmReport).
use std::fmt::Write;

use crate::{LimitStatus, WasmReport};

/// Renders a human-readable report.
pub fn render_table(r: &WasmReport, limit: LimitStatus) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "size_bytes: {}", r.size_bytes);
    let _ = writeln!(s, "sha256:     {}", r.sha256);
    let _ = writeln!(s, "size_limit: {limit:?}");
    if let Some(e) = &r.env_meta {
        let _ = writeln!(
            s,
            "protocol:   {} (pre-release {})",
            e.protocol, e.pre_release
        );
    }
    let _ = writeln!(
        s,
        "counts:     {} functions, {} data bytes, {} tables, {} memories",
        r.counts.functions, r.counts.data_segment_bytes, r.counts.tables, r.counts.memories
    );
    if !r.meta.is_empty() {
        let _ = writeln!(s, "\nmetadata:");
        for (k, v) in &r.meta {
            let _ = writeln!(s, "  {k} = {v}");
        }
    }
    let _ = writeln!(s, "\nspec functions:");
    for f in &r.spec_functions {
        let params: Vec<String> = f
            .inputs
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect();
        let ret = if f.outputs.is_empty() {
            "void".to_string()
        } else {
            f.outputs.join(", ")
        };
        let _ = writeln!(s, "  {}({}) -> {}", f.name, params.join(", "), ret);
    }
    for n in &r.spec_without_export {
        let _ = writeln!(s, "  warning: spec function `{n}` has no export");
    }
    for n in &r.export_without_spec {
        let _ = writeln!(s, "  warning: export `{n}` has no spec entry");
    }
    let _ = writeln!(s, "\nhost imports ({}):", r.imports.len());
    for i in &r.imports {
        let _ = writeln!(s, "  {i}");
    }
    let _ = writeln!(s, "\nsection sizes (largest first):");
    for sec in &r.sections {
        let _ = writeln!(s, "  {:<28} {:>8}", sec.name, sec.bytes);
    }
    s
}
