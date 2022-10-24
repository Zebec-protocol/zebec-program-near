use near_sdk::{Balance, Gas};


// Default Id used for native streams, this id will be ignored for native stream
pub const NATIVE_NEAR_CONTRACT_ID: &str = "test.near";

// Max rate of stream per second
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR

pub const FEE_BPS_DIVISOR: u64 = 10_000; // divisor for fee

/// Attach no deposit.
pub const NO_DEPOSIT: u128 = 0;

/// 10T gas for basic operation
pub const GAS_FOR_BASIC_OP: Gas = Gas(10_000_000_000_000);

pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(20_000_000_000_000);

pub const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

/// Amount of gas for fungible token transfers
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);
