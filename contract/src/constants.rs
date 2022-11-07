use near_sdk::{Balance, Gas};


// Default Id used for native streams, this id will be ignored for native stream
pub const NATIVE_NEAR_CONTRACT_ID: &str = "test.near";

// Max rate of stream per second
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR

// Divisor for fee percentage (use 10000 for 1% fee)
pub const FEE_BPS_DIVISOR: u64 = 10_000;

// Amount of gas for promise resolve
pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(20_000_000_000_000);

/// Amount of gas for fungible token transfers
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);

// Amount of gas for fungible token transfer and resolve method
pub const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

