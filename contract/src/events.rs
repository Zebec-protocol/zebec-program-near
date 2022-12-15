use std::fmt;

use near_sdk::{
    serde::{Deserialize, Serialize},
    serde_json, AccountId, Balance, Timestamp
};

/// An event log to capture native token creation
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct NStreamCreationLog {
    pub stream_id: u64,
    pub sender: AccountId,
    pub receiver: AccountId,
    pub created: Timestamp,
    pub rate: Balance,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub can_cancel: bool,
    pub can_update: bool,
    pub balance: Balance,
    pub is_native: bool,
}

impl fmt::Display for NStreamCreationLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Native stream created", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct FStreamCreationLog {
    pub stream_id: u64,
    pub sender: AccountId,
    pub receiver: AccountId,
    pub rate: Balance,
    pub created: Timestamp,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub can_cancel: bool,
    pub can_update: bool,
    pub balance: Balance,
    pub contract_id: AccountId,
}

impl fmt::Display for FStreamCreationLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"EVENT_JSON:{{"event": "Token stream created", "data":{}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// sender withdraw native
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct WithdrawNativeSenderLog {
    pub stream_id: u64,
    pub withdraw_amount: u128,
    pub withdraw_time: Timestamp,
    pub sender: AccountId,
}
impl fmt::Display for WithdrawNativeSenderLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Sender withdraws Native stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Sender withdraw token
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct WithdrawTokenSenderLog {
    pub stream_id: u64,
    pub withdraw_amount: u128,
    pub withdraw_time: Timestamp,
    pub sender: AccountId,
}
impl fmt::Display for WithdrawTokenSenderLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Sender withdraws Token stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Receiver withdraw native
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct WithdrawNativeReceiverLog {
    pub stream_id: u64,
    pub withdraw_amount: u128,
    pub withdraw_time: Timestamp,
    pub sender: AccountId,
}
impl fmt::Display for WithdrawNativeReceiverLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Receiver withdraws Native stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Receiver withdraws token
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct WithdrawTokenReceiverLog {
    pub stream_id: u64,
    pub contract_id: AccountId,
    pub withdraw_amount: u128,
    pub withdraw_time: Timestamp,
    pub sender: AccountId,
}
impl fmt::Display for WithdrawTokenReceiverLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Receiver withdraws Token stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Pause log
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StreamPauseLog {
    pub stream_id: u64,
    pub time: Timestamp,
}
impl fmt::Display for StreamPauseLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Stream paused", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}
// Resume log
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StreamResumeLog {
    pub stream_id: u64,
    pub time: Timestamp,
}
impl fmt::Display for StreamResumeLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Stream Resume", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Native stream cancelled
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct CancelNativeLog {
    pub stream_id: u64,
    pub time: Timestamp,
}
impl fmt::Display for CancelNativeLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Native stream cancelled", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// Token stream cancelled
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct CancelTokenLog {
    pub stream_id: u64,
    pub time: Timestamp,
    pub contract_id: AccountId,
}
impl fmt::Display for CancelTokenLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Token stream cancelled", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// sender claims native
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct ClaimNativeLog {
    pub stream_id: u64,
    pub time: Timestamp,
    pub balance: Balance,
}
impl fmt::Display for ClaimNativeLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Sender claims from native stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}

// sender claims token
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct ClaimTokenLog {
    pub stream_id: u64,
    pub contract_id: AccountId,
    pub time: Timestamp,
    pub balance: Balance,
}
impl fmt::Display for ClaimTokenLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Sender claims from token stream", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}


// stream update
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StreamUpdateLog {
    pub stream_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<Balance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<Balance>,
}
impl fmt::Display for StreamUpdateLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            r#"{{"EVENT_JSON":{{"event": "Stream updated", "data":{}}}}}"#,
            &serde_json::to_string(self).map_err(|_| fmt::Error)?
        ))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ns_creation() {
        let expected = r#"{"EVENT_JSON":{"event": "Native stream created", "data":{"stream_id":1,"sender":"sender.near","receiver":"receiver.near","created":100,"rate":100,"start_time":100,"end_time":100,"can_cancel":true,"can_update":true,"balance":100,"is_native":true}}}"#;

        let log = NStreamCreationLog {
            stream_id: 1,
            sender: "sender.near".parse().unwrap(),
            receiver: "receiver.near".parse().unwrap(),
            rate: 100,
            created: 100,
            start_time: 100,
            end_time: 100,
            can_cancel: true,
            can_update: true,
            balance: 100,
            is_native: true,
        };
        assert_eq!(expected, log.to_string());
    }
}
