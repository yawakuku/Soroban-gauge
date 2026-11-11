//! Static inspection of Soroban Wasm contracts.
//!
//! Nothing in this crate executes the Wasm: it parses the binary with
//! `wasmparser` and decodes the Soroban custom sections (`contractspecv0`,
//! `contractenvmetav0`, `contractmetav0`) with `stellar-xdr`.
#![forbid(unsafe_code)]

mod inspect;
mod render;
mod spec;

pub use inspect::{
    check_limits, inspect, Counts, EnvMetaInfo, LimitStatus, SectionSize, WasmReport,
};
pub use render::render_table;
pub use spec::{decode_spec, spec_functions, type_name, SpecFunction, SpecParam};

/// Errors from inspecting a Wasm file.
#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    /// The bytes are not a valid Wasm module.
    #[error("invalid Wasm: {0}")]
    Parse(String),
    /// A Soroban custom section could not be decoded.
    #[error("cannot decode custom section `{section}`: {reason}")]
    CustomSection {
        /// Section name.
        section: &'static str,
        /// Decoder message.
        reason: String,
    },
}
