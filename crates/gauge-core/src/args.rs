//! Converts TOML argument values into `ScVal`s using the contract's own spec.
//!
//! Every failure is returned as text naming the argument path, the expected
//! spec type and what was actually supplied. Nothing here panics.
use std::collections::BTreeMap;

use gauge_wasm::type_name;
use stellar_xdr::{
    Int128Parts, ScAddress, ScBytes, ScMap, ScMapEntry, ScSpecEntry, ScSpecFunctionInputV0,
    ScSpecTypeDef, ScSpecUdtUnionCaseV0, ScString, ScSymbol, ScVal, ScVec, UInt128Parts,
};

use crate::addr;

/// Context for conversion.
pub struct ArgContext<'a> {
    /// All spec entries of the contract (for user-defined types).
    pub spec: &'a [ScSpecEntry],
    /// Addresses of contracts deployed in the current scenario, for `$alias`.
    pub deployed: &'a BTreeMap<String, ScAddress>,
}

/// Converts a function's named arguments to an ordered `ScVal` list.
pub fn convert_args(
    inputs: &[ScSpecFunctionInputV0],
    given: &BTreeMap<String, toml::Value>,
    ctx: &ArgContext<'_>,
) -> Result<Vec<ScVal>, String> {
    let names: Vec<String> = inputs
        .iter()
        .map(|i| i.name.to_utf8_string_lossy())
        .collect();
    for k in given.keys() {
        if !names.contains(k) {
            return Err(format!(
                "unexpected argument `{k}` (the function takes: {})",
                if names.is_empty() {
                    "no arguments".to_string()
                } else {
                    names.join(", ")
                }
            ));
        }
    }
    let mut out = Vec::with_capacity(inputs.len());
    for (input, name) in inputs.iter().zip(&names) {
        out.push(convert(given.get(name), &input.type_, ctx, name)?);
    }
    Ok(out)
}

fn describe(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("string \"{s}\""),
        toml::Value::Integer(i) => format!("integer {i}"),
        toml::Value::Float(f) => format!("float {f}"),
        toml::Value::Boolean(b) => format!("boolean {b}"),
        toml::Value::Datetime(_) => "datetime".to_string(),
        toml::Value::Array(_) => "array".to_string(),
        toml::Value::Table(_) => "table".to_string(),
    }
}

fn mismatch(path: &str, ty: &ScSpecTypeDef, v: &toml::Value) -> String {
    format!(
        "argument `{path}`: expected {}, got {}",
        type_name(ty),
        describe(v)
    )
}

fn bad(path: &str, ty: &ScSpecTypeDef, why: impl std::fmt::Display) -> String {
    format!("argument `{path}`: expected {}: {why}", type_name(ty))
}

fn int_text(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::String(s) => Some(s.trim().to_string()),
        _ => None,
    }
}

fn address(s: &str, ctx: &ArgContext<'_>) -> Result<ScAddress, String> {
    if let Some(rest) = s.strip_prefix('@') {
        if rest.is_empty() {
            return Err("`@` needs a name, e.g. \"@alice\"".into());
        }
        return Ok(match rest.strip_suffix(":contract") {
            Some(name) if !name.is_empty() => addr::contract(name),
            _ => addr::account(rest),
        });
    }
    if let Some(alias) = s.strip_prefix('$') {
        return ctx
            .deployed
            .get(alias)
            .cloned()
            .ok_or_else(|| format!("`${alias}` is not a contract deployed in this scenario"));
    }
    s.parse::<ScAddress>().map_err(|_| {
        "use \"@name\", \"@name:contract\", \"$contract_alias\" or a G…/C… address".to_string()
    })
}

/// Converts one value. `v == None` means the argument was omitted.
pub fn convert(
    v: Option<&toml::Value>,
    ty: &ScSpecTypeDef,
    ctx: &ArgContext<'_>,
    path: &str,
) -> Result<ScVal, String> {
    if let ScSpecTypeDef::Option(inner) = ty {
        return match v {
            None => Ok(ScVal::Void),
            Some(v) => convert(Some(v), &inner.value_type, ctx, path),
        };
    }
    let Some(v) = v else {
        return Err(format!(
            "argument `{path}` is missing (expected {})",
            type_name(ty)
        ));
    };
    match ty {
        ScSpecTypeDef::Bool => match v {
            toml::Value::Boolean(b) => Ok(ScVal::Bool(*b)),
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Void => Ok(ScVal::Void),
        ScSpecTypeDef::U32 => small::<u32>(v, ty, path).map(ScVal::U32),
        ScSpecTypeDef::I32 => small::<i32>(v, ty, path).map(ScVal::I32),
        ScSpecTypeDef::U64 => small::<u64>(v, ty, path).map(ScVal::U64),
        ScSpecTypeDef::I64 => small::<i64>(v, ty, path).map(ScVal::I64),
        ScSpecTypeDef::Timepoint => small::<u64>(v, ty, path).map(|t| ScVal::Timepoint(t.into())),
        ScSpecTypeDef::Duration => small::<u64>(v, ty, path).map(|t| ScVal::Duration(t.into())),
        ScSpecTypeDef::U128 => small::<u128>(v, ty, path).map(|n| {
            ScVal::U128(UInt128Parts {
                hi: (n >> 64) as u64,
                lo: n as u64,
            })
        }),
        ScSpecTypeDef::I128 => small::<i128>(v, ty, path).map(|n| {
            ScVal::I128(Int128Parts {
                hi: (n >> 64) as i64,
                lo: n as u64,
            })
        }),
        ScSpecTypeDef::U256 | ScSpecTypeDef::I256 => Err(format!(
            "argument `{path}`: type {} is not supported in scenario arguments yet",
            type_name(ty)
        )),
        ScSpecTypeDef::Bytes => {
            let b = hex_bytes(v, ty, path)?;
            ScBytes::try_from(b)
                .map(ScVal::Bytes)
                .map_err(|e| bad(path, ty, e))
        }
        ScSpecTypeDef::BytesN(n) => {
            let b = hex_bytes(v, ty, path)?;
            if b.len() != n.n as usize {
                return Err(bad(
                    path,
                    ty,
                    format!("got {} bytes, need {}", b.len(), n.n),
                ));
            }
            ScBytes::try_from(b)
                .map(ScVal::Bytes)
                .map_err(|e| bad(path, ty, e))
        }
        ScSpecTypeDef::String => match v {
            toml::Value::String(s) => stellar_xdr::StringM::try_from(s.as_str())
                .map(|m| ScVal::String(ScString(m)))
                .map_err(|e| bad(path, ty, e)),
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Symbol => match v {
            toml::Value::String(s) => ScSymbol::try_from(s.as_str())
                .map(ScVal::Symbol)
                .map_err(|_| bad(path, ty, "too long for a symbol (max 32 characters)")),
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Address | ScSpecTypeDef::MuxedAddress => match v {
            toml::Value::String(s) => address(s, ctx)
                .map(ScVal::Address)
                .map_err(|e| bad(path, ty, e)),
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Vec(inner) => match v {
            toml::Value::Array(items) => {
                let vals = items
                    .iter()
                    .enumerate()
                    .map(|(i, it)| {
                        convert(Some(it), &inner.element_type, ctx, &format!("{path}[{i}]"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                vec_val(vals, ty, path)
            }
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Tuple(t) => match v {
            toml::Value::Array(items) if items.len() == t.value_types.len() => {
                let vals = items
                    .iter()
                    .zip(t.value_types.iter())
                    .enumerate()
                    .map(|(i, (it, ety))| convert(Some(it), ety, ctx, &format!("{path}.{i}")))
                    .collect::<Result<Vec<_>, _>>()?;
                vec_val(vals, ty, path)
            }
            toml::Value::Array(items) => Err(bad(
                path,
                ty,
                format!("got {} elements, need {}", items.len(), t.value_types.len()),
            )),
            _ => Err(mismatch(path, ty, v)),
        },
        ScSpecTypeDef::Map(m) => {
            let mut entries: Vec<ScMapEntry> = Vec::new();
            match v {
                toml::Value::Table(tbl) => {
                    for (k, val) in tbl {
                        let key = convert(
                            Some(&toml::Value::String(k.clone())),
                            &m.key_type,
                            ctx,
                            &format!("{path}.{k}"),
                        )?;
                        let value = convert(Some(val), &m.value_type, ctx, &format!("{path}.{k}"))?;
                        entries.push(ScMapEntry { key, val: value });
                    }
                }
                toml::Value::Array(pairs) => {
                    for (i, p) in pairs.iter().enumerate() {
                        match p {
                            toml::Value::Array(kv) if kv.len() == 2 => {
                                let key = convert(
                                    Some(&kv[0]),
                                    &m.key_type,
                                    ctx,
                                    &format!("{path}[{i}].key"),
                                )?;
                                let value = convert(
                                    Some(&kv[1]),
                                    &m.value_type,
                                    ctx,
                                    &format!("{path}[{i}].value"),
                                )?;
                                entries.push(ScMapEntry { key, val: value });
                            }
                            _ => {
                                return Err(bad(
                                    path,
                                    ty,
                                    format!("element {i} must be a [key, value] pair"),
                                ))
                            }
                        }
                    }
                }
                _ => return Err(mismatch(path, ty, v)),
            }
            map_val(entries, ty, path)
        }
        ScSpecTypeDef::Udt(u) => {
            let name = u.name.to_utf8_string_lossy();
            udt(v, ty, &name, ctx, path)
        }
        ScSpecTypeDef::Val
        | ScSpecTypeDef::Error
        | ScSpecTypeDef::Result(_)
        | ScSpecTypeDef::Option(_) => Err(format!(
            "argument `{path}`: type {} is not supported in scenario arguments",
            type_name(ty)
        )),
    }
}

fn small<T: std::str::FromStr>(
    v: &toml::Value,
    ty: &ScSpecTypeDef,
    path: &str,
) -> Result<T, String> {
    let Some(text) = int_text(v) else {
        return Err(mismatch(path, ty, v));
    };
    text.parse::<T>().map_err(|_| {
        bad(
            path,
            ty,
            format!(
                "`{text}` is not a valid {} (check range and digits)",
                type_name(ty)
            ),
        )
    })
}

fn hex_bytes(v: &toml::Value, ty: &ScSpecTypeDef, path: &str) -> Result<Vec<u8>, String> {
    match v {
        toml::Value::String(s) => {
            let h = s.strip_prefix("0x").unwrap_or(s);
            hex::decode(h).map_err(|_| bad(path, ty, "use a hex string like \"0a1b\""))
        }
        _ => Err(mismatch(path, ty, v)),
    }
}

fn vec_val(vals: Vec<ScVal>, ty: &ScSpecTypeDef, path: &str) -> Result<ScVal, String> {
    ScVec::try_from(vals)
        .map(|v| ScVal::Vec(Some(v)))
        .map_err(|e| bad(path, ty, e))
}

fn map_val(mut entries: Vec<ScMapEntry>, ty: &ScSpecTypeDef, path: &str) -> Result<ScVal, String> {
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    if entries.windows(2).any(|w| w[0].key == w[1].key) {
        return Err(bad(path, ty, "duplicate map key"));
    }
    ScMap::try_from(entries)
        .map(|m| ScVal::Map(Some(m)))
        .map_err(|e| bad(path, ty, e))
}

fn udt(
    v: &toml::Value,
    ty: &ScSpecTypeDef,
    name: &str,
    ctx: &ArgContext<'_>,
    path: &str,
) -> Result<ScVal, String> {
    let entry = ctx
        .spec
        .iter()
        .find(|e| match e {
            ScSpecEntry::UdtStructV0(s) => s.name.to_utf8_string_lossy() == name,
            ScSpecEntry::UdtUnionV0(s) => s.name.to_utf8_string_lossy() == name,
            ScSpecEntry::UdtEnumV0(s) => s.name.to_utf8_string_lossy() == name,
            ScSpecEntry::UdtErrorEnumV0(s) => s.name.to_utf8_string_lossy() == name,
            _ => false,
        })
        .ok_or_else(|| bad(path, ty, "type not found in the contract spec"))?;
    match entry {
        ScSpecEntry::UdtStructV0(s) => {
            let toml::Value::Table(tbl) = v else {
                // Tuple-like structs (fields named 0, 1, ...) accept arrays.
                if let toml::Value::Array(items) = v {
                    if items.len() == s.fields.len() {
                        let vals = items
                            .iter()
                            .zip(s.fields.iter())
                            .enumerate()
                            .map(|(i, (it, f))| {
                                convert(Some(it), &f.type_, ctx, &format!("{path}.{i}"))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        return vec_val(vals, ty, path);
                    }
                }
                return Err(mismatch(path, ty, v));
            };
            let known: Vec<String> = s
                .fields
                .iter()
                .map(|f| f.name.to_utf8_string_lossy())
                .collect();
            if let Some(extra) = tbl.keys().find(|k| !known.contains(k)) {
                return Err(bad(
                    path,
                    ty,
                    format!("unknown field `{extra}` (fields: {})", known.join(", ")),
                ));
            }
            let mut entries = Vec::new();
            for f in s.fields.iter() {
                let fname = f.name.to_utf8_string_lossy();
                let val = convert(tbl.get(&fname), &f.type_, ctx, &format!("{path}.{fname}"))?;
                let key = ScSymbol::try_from(fname.as_str())
                    .map_err(|_| bad(path, ty, "field name too long"))?;
                entries.push(ScMapEntry {
                    key: ScVal::Symbol(key),
                    val,
                });
            }
            map_val(entries, ty, path)
        }
        ScSpecEntry::UdtEnumV0(e) => {
            let cases: Vec<(String, u32)> = e
                .cases
                .iter()
                .map(|c| (c.name.to_utf8_string_lossy(), c.value))
                .collect();
            match v {
                toml::Value::Integer(i) => u32::try_from(*i)
                    .ok()
                    .filter(|n| cases.iter().any(|(_, val)| val == n))
                    .map(ScVal::U32)
                    .ok_or_else(|| bad(path, ty, format!("{i} is not a case value"))),
                toml::Value::String(s) => cases
                    .iter()
                    .find(|(n, _)| n == s)
                    .map(|(_, val)| ScVal::U32(*val))
                    .ok_or_else(|| {
                        bad(
                            path,
                            ty,
                            format!("`{s}` is not a case (cases: {})", names_of(&cases)),
                        )
                    }),
                _ => Err(mismatch(path, ty, v)),
            }
        }
        ScSpecEntry::UdtErrorEnumV0(_) => {
            Err(bad(path, ty, "error enums cannot be passed as arguments"))
        }
        ScSpecEntry::UdtUnionV0(u) => {
            let sym = |n: &str| {
                ScSymbol::try_from(n)
                    .map(ScVal::Symbol)
                    .map_err(|_| bad(path, ty, "case name too long"))
            };
            match v {
                toml::Value::String(case) => {
                    let found = u.cases.iter().any(|c| matches!(c, ScSpecUdtUnionCaseV0::VoidV0(c) if c.name.to_utf8_string_lossy() == *case));
                    if !found {
                        return Err(bad(
                            path,
                            ty,
                            format!("`{case}` is not a case without data"),
                        ));
                    }
                    vec_val(vec![sym(case)?], ty, path)
                }
                toml::Value::Table(tbl) if tbl.len() == 1 => {
                    let (case, payload) = tbl
                        .iter()
                        .next()
                        .ok_or_else(|| bad(path, ty, "empty table"))?;
                    let tuple = u.cases.iter().find_map(|c| match c {
                        ScSpecUdtUnionCaseV0::TupleV0(c)
                            if c.name.to_utf8_string_lossy() == *case =>
                        {
                            Some(c)
                        }
                        _ => None,
                    });
                    let Some(tuple) = tuple else {
                        return Err(bad(path, ty, format!("`{case}` is not a case with data")));
                    };
                    let items: Vec<&toml::Value> = match payload {
                        toml::Value::Array(a) => a.iter().collect(),
                        other => vec![other],
                    };
                    if items.len() != tuple.type_.len() {
                        return Err(bad(
                            path,
                            ty,
                            format!("case `{case}` needs {} value(s)", tuple.type_.len()),
                        ));
                    }
                    let mut vals = vec![sym(case)?];
                    for (i, (it, ety)) in items.iter().zip(tuple.type_.iter()).enumerate() {
                        vals.push(convert(Some(it), ety, ctx, &format!("{path}.{case}.{i}"))?);
                    }
                    vec_val(vals, ty, path)
                }
                _ => Err(bad(path, ty, "use \"Case\" or { Case = value-or-array }")),
            }
        }
        _ => Err(bad(path, ty, "not a user-defined type")),
    }
}

fn names_of(cases: &[(String, u32)]) -> String {
    cases
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{ScSpecTypeOption, ScSpecTypeVec};

    fn ctx<'a>(d: &'a BTreeMap<String, ScAddress>) -> ArgContext<'a> {
        ArgContext {
            spec: &[],
            deployed: d,
        }
    }
    fn conv(v: toml::Value, ty: ScSpecTypeDef) -> Result<ScVal, String> {
        let d = BTreeMap::new();
        convert(Some(&v), &ty, &ctx(&d), "x")
    }

    #[test]
    fn integers_and_ranges() {
        assert_eq!(conv(5.into(), ScSpecTypeDef::U32).unwrap(), ScVal::U32(5));
        assert!(conv((-1).into(), ScSpecTypeDef::U32)
            .unwrap_err()
            .contains("expected u32"));
        assert!(conv("abc".into(), ScSpecTypeDef::I64).is_err());
        assert!(conv(true.into(), ScSpecTypeDef::U32)
            .unwrap_err()
            .contains("got boolean"));
    }

    #[test]
    fn i128_from_decimal_string() {
        let v = conv("-1".into(), ScSpecTypeDef::I128).unwrap();
        assert_eq!(
            v,
            ScVal::I128(Int128Parts {
                hi: -1,
                lo: u64::MAX
            })
        );
        let big = conv(
            "340282366920938463463374607431768211455".into(),
            ScSpecTypeDef::U128,
        )
        .unwrap();
        assert_eq!(
            big,
            ScVal::U128(UInt128Parts {
                hi: u64::MAX,
                lo: u64::MAX
            })
        );
        assert!(conv(
            "340282366920938463463374607431768211456".into(),
            ScSpecTypeDef::U128
        )
        .is_err());
    }

    #[test]
    fn addresses() {
        let v = conv("@alice".into(), ScSpecTypeDef::Address).unwrap();
        assert_eq!(v, ScVal::Address(addr::account("alice")));
        let c = conv("@pool:contract".into(), ScSpecTypeDef::Address).unwrap();
        assert_eq!(c, ScVal::Address(addr::contract("pool")));
        assert!(conv("nonsense".into(), ScSpecTypeDef::Address).is_err());
        assert!(conv(1.into(), ScSpecTypeDef::Address)
            .unwrap_err()
            .contains("expected address"));
    }

    #[test]
    fn option_and_vec() {
        let opt = ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
            value_type: ScSpecTypeDef::U32.into(),
        }));
        let d = BTreeMap::new();
        assert_eq!(convert(None, &opt, &ctx(&d), "x").unwrap(), ScVal::Void);
        let vt = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: ScSpecTypeDef::U32.into(),
        }));
        let arr = toml::Value::Array(vec![1.into(), 2.into()]);
        assert!(matches!(
            conv(arr, vt.clone()).unwrap(),
            ScVal::Vec(Some(_))
        ));
        let bad_arr = toml::Value::Array(vec![1.into(), "z".into()]);
        assert!(conv(bad_arr, vt).unwrap_err().contains("x[1]"));
    }

    #[test]
    fn missing_required_argument_is_named() {
        let d = BTreeMap::new();
        let e = convert(None, &ScSpecTypeDef::I128, &ctx(&d), "amount").unwrap_err();
        assert!(e.contains("amount") && e.contains("i128"));
    }
}
