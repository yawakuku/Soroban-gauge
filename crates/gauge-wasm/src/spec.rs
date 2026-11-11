//! Decoding of the `contractspecv0` custom section.
use std::io::Cursor;

use serde::Serialize;
use stellar_xdr::{Limited, Limits, ReadXdr, ScSpecEntry, ScSpecFunctionV0, ScSpecTypeDef};

use crate::WasmError;

/// Decodes every spec entry from the raw bytes of the `contractspecv0` section.
pub fn decode_spec(section: &[u8]) -> Result<Vec<ScSpecEntry>, WasmError> {
    let mut cursor = Cursor::new(section);
    let mut limited = Limited::new(&mut cursor, Limits::none());
    let mut out = Vec::new();
    for entry in ScSpecEntry::read_xdr_iter(&mut limited) {
        out.push(entry.map_err(|e| WasmError::CustomSection {
            section: "contractspecv0",
            reason: e.to_string(),
        })?);
    }
    Ok(out)
}

/// A function parameter from the contract spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecParam {
    /// Parameter name.
    pub name: String,
    /// Type rendered as text, e.g. `i128` or `vec<address>`.
    #[serde(rename = "type")]
    pub ty: String,
}

/// A function declared in the contract spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecFunction {
    /// Function name.
    pub name: String,
    /// Parameters in declaration order.
    pub inputs: Vec<SpecParam>,
    /// Return types (usually zero or one).
    pub outputs: Vec<String>,
}

fn from_v0(f: &ScSpecFunctionV0) -> SpecFunction {
    SpecFunction {
        name: f.name.to_utf8_string_lossy(),
        inputs: f
            .inputs
            .iter()
            .map(|i| SpecParam {
                name: i.name.to_utf8_string_lossy(),
                ty: type_name(&i.type_),
            })
            .collect(),
        outputs: f.outputs.iter().map(type_name).collect(),
    }
}

/// Extracts the function entries, in declaration order.
pub fn spec_functions(entries: &[ScSpecEntry]) -> Vec<SpecFunction> {
    entries
        .iter()
        .filter_map(|e| match e {
            ScSpecEntry::FunctionV0(f) => Some(from_v0(f)),
            _ => None,
        })
        .collect()
}

/// Renders a spec type as short text (`u32`, `option<address>`, `map<symbol, i128>`).
pub fn type_name(t: &ScSpecTypeDef) -> String {
    match t {
        ScSpecTypeDef::Val => "val".into(),
        ScSpecTypeDef::Bool => "bool".into(),
        ScSpecTypeDef::Void => "void".into(),
        ScSpecTypeDef::Error => "error".into(),
        ScSpecTypeDef::U32 => "u32".into(),
        ScSpecTypeDef::I32 => "i32".into(),
        ScSpecTypeDef::U64 => "u64".into(),
        ScSpecTypeDef::I64 => "i64".into(),
        ScSpecTypeDef::Timepoint => "timepoint".into(),
        ScSpecTypeDef::Duration => "duration".into(),
        ScSpecTypeDef::U128 => "u128".into(),
        ScSpecTypeDef::I128 => "i128".into(),
        ScSpecTypeDef::U256 => "u256".into(),
        ScSpecTypeDef::I256 => "i256".into(),
        ScSpecTypeDef::Bytes => "bytes".into(),
        ScSpecTypeDef::String => "string".into(),
        ScSpecTypeDef::Symbol => "symbol".into(),
        ScSpecTypeDef::Address => "address".into(),
        ScSpecTypeDef::MuxedAddress => "muxed_address".into(),
        ScSpecTypeDef::Option(o) => format!("option<{}>", type_name(&o.value_type)),
        ScSpecTypeDef::Result(r) => format!(
            "result<{}, {}>",
            type_name(&r.ok_type),
            type_name(&r.error_type)
        ),
        ScSpecTypeDef::Vec(v) => format!("vec<{}>", type_name(&v.element_type)),
        ScSpecTypeDef::Map(m) => format!(
            "map<{}, {}>",
            type_name(&m.key_type),
            type_name(&m.value_type)
        ),
        ScSpecTypeDef::Tuple(t) => format!(
            "({})",
            t.value_types
                .iter()
                .map(type_name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScSpecTypeDef::BytesN(b) => format!("bytes{}", b.n),
        ScSpecTypeDef::Udt(u) => u.name.to_utf8_string_lossy(),
    }
}
