use near_sdk::Balance;

/// Default Id used for native streams, this id will be ignored for native stream
pub const NATIVE_NEAR_CONTRACT_ID: &str = "test.near";

/// Max rate of stream per second
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR

/// BPS used for fee calculation
pub const FEE_BPS_DIVISOR: u64 = 10_000;
