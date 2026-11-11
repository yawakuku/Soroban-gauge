//! Deterministic addresses for scenario arguments (`@alice`, `@pool:contract`).
use sha2::{Digest, Sha256};
use stellar_xdr::{AccountId, ContractId, Hash, PublicKey, ScAddress, Uint256};

fn digest(domain: &str, name: &str) -> [u8; 32] {
    Sha256::digest(format!("soroban-gauge/{domain}/{name}").as_bytes()).into()
}

/// A stable account address derived from `name`.
pub fn account(name: &str) -> ScAddress {
    ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(digest(
        "account", name,
    )))))
}

/// A stable contract address derived from `name`.
pub fn contract(name: &str) -> ScAddress {
    ScAddress::Contract(ContractId(Hash(digest("contract", name))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_distinct() {
        assert_eq!(account("alice"), account("alice"));
        assert_ne!(account("alice"), account("bob"));
        assert_ne!(account("alice"), contract("alice"));
    }
}
