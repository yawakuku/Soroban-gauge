//! Static report on a Wasm file.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

use serde::Serialize;
use sha2::{Digest, Sha256};
use stellar_xdr::{Limited, Limits, ReadXdr, ScEnvMetaEntry, ScMetaEntry};
use wasmparser::{ExternalKind, Imports, Parser, Payload, TypeRef};

use crate::spec::{decode_spec, spec_functions, SpecFunction};
use crate::WasmError;

/// Item counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Functions defined in the module (imports not included).
    pub functions: u64,
    /// Sum of all data segment payloads, in bytes.
    pub data_segment_bytes: u64,
    /// Tables defined in the module.
    pub tables: u64,
    /// Memories defined in the module.
    pub memories: u64,
}

/// Size of one Wasm section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionSize {
    /// Section name (`code`, `data`, `custom:contractspecv0`, ...).
    pub name: String,
    /// Payload size in bytes.
    pub bytes: u64,
}

/// Decoded `contractenvmetav0` information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvMetaInfo {
    /// Protocol (interface) version the contract was built against.
    pub protocol: u32,
    /// Pre-release counter of the interface version.
    pub pre_release: u32,
}

/// Result of checking the file size against configured limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitStatus {
    /// Within limits (or no limits configured).
    Ok,
    /// Above the warning threshold.
    Warn,
    /// Above the hard maximum.
    Fail,
}

/// Static facts about a Wasm file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WasmReport {
    /// File size.
    pub size_bytes: u64,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: String,
    /// Sorted, de-duplicated `module.name` host imports.
    pub imports: Vec<String>,
    /// Sorted exported function names.
    pub exports: Vec<String>,
    /// Functions declared in `contractspecv0`.
    pub spec_functions: Vec<SpecFunction>,
    /// Spec functions that have no matching export.
    pub spec_without_export: Vec<String>,
    /// Exported functions that have no spec entry (the `_` marker is ignored).
    pub export_without_spec: Vec<String>,
    /// `contractenvmetav0`, when present.
    pub env_meta: Option<EnvMetaInfo>,
    /// `contractmetav0` key/value pairs.
    pub meta: BTreeMap<String, String>,
    /// Item counts.
    pub counts: Counts,
    /// Sections ranked by size, largest first.
    pub sections: Vec<SectionSize>,
}

fn section_name(id: u8) -> String {
    match id {
        1 => "type",
        2 => "import",
        3 => "function",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "datacount",
        13 => "tag",
        _ => "unknown",
    }
    .to_string()
}

fn parse_err(e: impl std::fmt::Display) -> WasmError {
    WasmError::Parse(e.to_string())
}

fn xdr_err(section: &'static str, e: impl std::fmt::Display) -> WasmError {
    WasmError::CustomSection {
        section,
        reason: e.to_string(),
    }
}

/// Inspects `bytes` without executing them.
pub fn inspect(bytes: &[u8]) -> Result<WasmReport, WasmError> {
    let mut imports = BTreeSet::new();
    let mut exports = BTreeSet::new();
    let mut counts = Counts::default();
    let mut sizes: BTreeMap<String, u64> = BTreeMap::new();
    let mut spec_bytes: Option<&[u8]> = None;
    let mut env_bytes: Option<&[u8]> = None;
    let mut meta_bytes: Option<&[u8]> = None;

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(parse_err)?;
        if let Some((id, range)) = payload.as_section() {
            let name = match &payload {
                Payload::CustomSection(c) => format!("custom:{}", c.name()),
                _ => section_name(id),
            };
            *sizes.entry(name).or_default() += range.end.saturating_sub(range.start);
        }
        match payload {
            Payload::ImportSection(reader) => {
                for group in reader {
                    match group.map_err(parse_err)? {
                        Imports::Single(_, i) => {
                            if matches!(i.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                                imports.insert(format!("{}.{}", i.module, i.name));
                            }
                        }
                        Imports::Compact1 { module, items } => {
                            for item in items {
                                let item = item.map_err(parse_err)?;
                                imports.insert(format!("{module}.{}", item.name));
                            }
                        }
                        Imports::Compact2 { module, names, .. } => {
                            for name in names {
                                let name = name.map_err(parse_err)?;
                                imports.insert(format!("{module}.{name}"));
                            }
                        }
                    }
                }
            }
            Payload::FunctionSection(r) => counts.functions += u64::from(r.count()),
            Payload::TableSection(r) => counts.tables += u64::from(r.count()),
            Payload::MemorySection(r) => counts.memories += u64::from(r.count()),
            Payload::ExportSection(reader) => {
                for e in reader {
                    let e = e.map_err(parse_err)?;
                    if e.kind == ExternalKind::Func {
                        exports.insert(e.name.to_string());
                    }
                }
            }
            Payload::DataSection(reader) => {
                for d in reader {
                    let d = d.map_err(parse_err)?;
                    counts.data_segment_bytes += d.data.len() as u64;
                }
            }
            Payload::CustomSection(c) => match c.name() {
                "contractspecv0" => spec_bytes = Some(c.data()),
                "contractenvmetav0" => env_bytes = Some(c.data()),
                "contractmetav0" => meta_bytes = Some(c.data()),
                _ => {}
            },
            _ => {}
        }
    }

    let functions = match spec_bytes {
        Some(b) => spec_functions(&decode_spec(b)?),
        None => Vec::new(),
    };

    let env_meta = match env_bytes {
        Some(b) => {
            let mut cursor = Cursor::new(b);
            let mut lim = Limited::new(&mut cursor, Limits::none());
            let mut found = None;
            for e in ScEnvMetaEntry::read_xdr_iter(&mut lim) {
                let ScEnvMetaEntry::ScEnvMetaKindInterfaceVersion(v) =
                    e.map_err(|e| xdr_err("contractenvmetav0", e))?;
                found = Some(EnvMetaInfo {
                    protocol: v.protocol,
                    pre_release: v.pre_release,
                });
            }
            found
        }
        None => None,
    };

    let mut meta = BTreeMap::new();
    if let Some(b) = meta_bytes {
        let mut cursor = Cursor::new(b);
        let mut lim = Limited::new(&mut cursor, Limits::none());
        for e in ScMetaEntry::read_xdr_iter(&mut lim) {
            let ScMetaEntry::ScMetaV0(kv) = e.map_err(|e| xdr_err("contractmetav0", e))?;
            meta.insert(kv.key.to_utf8_string_lossy(), kv.val.to_utf8_string_lossy());
        }
    }

    let spec_names: BTreeSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    let spec_without_export = spec_names
        .iter()
        .filter(|n| !exports.contains(**n))
        .map(|n| n.to_string())
        .collect();
    let export_without_spec = exports
        .iter()
        .filter(|n| n.as_str() != "_" && !spec_names.contains(n.as_str()))
        .cloned()
        .collect();

    let mut sections: Vec<SectionSize> = sizes
        .into_iter()
        .map(|(name, bytes)| SectionSize { name, bytes })
        .collect();
    sections.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));

    Ok(WasmReport {
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(bytes)),
        imports: imports.into_iter().collect(),
        exports: exports.into_iter().collect(),
        spec_functions: functions,
        spec_without_export,
        export_without_spec,
        env_meta,
        meta,
        counts,
        sections,
    })
}

/// Checks a size against optional warn/max limits (`max` wins over `warn`).
pub fn check_limits(size_bytes: u64, max: Option<u64>, warn: Option<u64>) -> LimitStatus {
    if max.is_some_and(|m| size_bytes > m) {
        LimitStatus::Fail
    } else if warn.is_some_and(|w| size_bytes > w) {
        LimitStatus::Warn
    } else {
        LimitStatus::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_order() {
        assert_eq!(check_limits(10, None, None), LimitStatus::Ok);
        assert_eq!(check_limits(10, Some(10), Some(5)), LimitStatus::Warn);
        assert_eq!(check_limits(11, Some(10), Some(5)), LimitStatus::Fail);
        assert_eq!(check_limits(6, None, Some(5)), LimitStatus::Warn);
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(inspect(b"not wasm"), Err(WasmError::Parse(_))));
    }
}
