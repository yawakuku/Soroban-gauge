#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;

use gauge_core::baseline::{ContractInfo, Entry, Environment, Metrics};
use gauge_core::Baseline;

pub fn env() -> Environment {
    Environment {
        tool_version: "0.1.0".into(),
        soroban_sdk_version: "29.0.1".into(),
        soroban_env_host_version: "29.0.1".into(),
        protocol_version: 29,
    }
}

pub fn metrics(instr: u64, mem: u64, writes: u64, fee: u64) -> Metrics {
    Metrics {
        instructions: instr,
        mem_bytes: mem,
        disk_read_entries: 0,
        memory_read_entries: 1,
        write_entries: writes,
        disk_read_bytes: 0,
        write_bytes: writes * 40,
        events_bytes: 0,
        fee_stroops: fee,
    }
}

pub fn entry(key: &str, m: Metrics) -> Entry {
    let (scenario, step) = key.split_once('/').unwrap();
    Entry {
        key: key.into(),
        scenario: scenario.into(),
        step: step.into(),
        contract: "counter".into(),
        function: step.into(),
        metrics: m,
        fee_components: BTreeMap::new(),
    }
}

pub fn baseline(sha: &str, size: u64, entries: Vec<Entry>) -> Baseline {
    let mut contracts = BTreeMap::new();
    contracts.insert(
        "counter".to_string(),
        ContractInfo {
            wasm_sha256: sha.into(),
            wasm_size_bytes: size,
        },
    );
    Baseline::with_environment(env(), contracts, entries).unwrap()
}
