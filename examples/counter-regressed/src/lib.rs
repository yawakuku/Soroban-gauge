#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Env, Symbol};

const COUNT: Symbol = symbol_short!("COUNT");

/// Same interface as `examples/counter`, but `increment` deliberately does extra
/// work: a busy loop and additional persistent storage writes.
#[contract]
pub struct Counter;

#[contractimpl]
impl Counter {
    pub fn increment(env: Env) -> u32 {
        let mut count: u32 = env.storage().instance().get(&COUNT).unwrap_or(0);
        count += 1;
        env.storage().instance().set(&COUNT, &count);

        // Extra persistent writes (one ledger entry each).
        for i in 0u32..3 {
            let key = (symbol_short!("HIST"), i);
            env.storage().persistent().set(&key, &count);
        }
        // Extra CPU work.
        let mut acc: u32 = 0;
        for i in 0u32..2000 {
            acc = acc.wrapping_add(i ^ count);
        }
        env.storage().persistent().set(&symbol_short!("ACC"), &acc);
        count
    }

    pub fn get(env: Env) -> u32 {
        env.storage().instance().get(&COUNT).unwrap_or(0)
    }
}
