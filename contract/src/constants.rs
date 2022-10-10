// Default Id used for native streams, this id will be ignored for native stream
pub const NATIVE_NEAR_CONTRACT_ID: &str = "test.near";

// Valid token account ids for testnet, needs to build with feature="testnet"
pub const TESTNET_TOKEN_ACCOUNTS: [&'static str; 2] = [
    "usdn.testnet",
    "wrap.tesnet",
];

// @todo add valid stablecoins (from mainnet) address here later
pub const MAINNET_TOKEN_ACCOUNTS: [&'static str; 2] = [
    "usdn.near",
    "wrap.near",
];
