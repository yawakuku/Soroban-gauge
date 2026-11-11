//! Core of soroban-gauge: configuration, scenario model, runner and baseline.
//!
//! Costs come from the Soroban host embedded through `soroban-sdk` (see
//! [`environment`]). They are estimates for that host and protocol, not network fees.
#![forbid(unsafe_code)]

pub mod addr;
pub mod args;
pub mod baseline;
pub mod config;
pub mod environment;
pub mod error;
#[cfg(feature = "runner")]
pub mod runner;
pub mod scenario;

pub use baseline::{Baseline, ContractInfo, Entry, Environment, Metrics};
pub use config::{Config, Thresholds};
pub use error::CoreError;
