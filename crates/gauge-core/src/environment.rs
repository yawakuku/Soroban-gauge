//! Versions of the embedded host. These are recorded in every baseline because
//! costs are only comparable between identical hosts.

/// This tool's version.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Pinned `soroban-sdk` version (checked against `Cargo.lock` by a unit test).
pub const SOROBAN_SDK_VERSION: &str = "29.0.1";
/// `soroban-env-host` version resolved for that SDK (checked against `Cargo.lock`).
pub const SOROBAN_ENV_HOST_VERSION: &str = "29.0.1";

/// Ledger protocol version the embedded host implements.
#[cfg(feature = "runner")]
pub fn protocol_version() -> u32 {
    use soroban_sdk::testutils::Ledger as _;
    soroban_sdk::Env::default().ledger().get().protocol_version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_match_lockfile() {
        let lock =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
                .unwrap_or_default();
        for (name, ver) in [
            ("soroban-sdk", SOROBAN_SDK_VERSION),
            ("soroban-env-host", SOROBAN_ENV_HOST_VERSION),
        ] {
            let needle = format!("name = \"{name}\"\nversion = \"{ver}\"");
            assert!(
                lock.contains(&needle),
                "Cargo.lock does not pin {name} {ver}"
            );
        }
    }
}
