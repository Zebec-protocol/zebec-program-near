use crate::*;

use constants::NATIVE_NEAR_CONTRACT_ID;

#[near_bindgen]
impl Contract {
    // Create a stream struct from the given parameters
    pub fn validate_stream(
        &mut self,
        stream_id: U64,
        sender: AccountId,
        receiver: AccountId,
        stream_rate: U128,
        start: U64,
        end: U64,
        can_cancel: bool,
        can_update: bool,
        is_native: bool,
        contract_id: AccountId,
    ) -> Stream {
        // convert id to native u128/u64
        let id: u64 = stream_id.0;
        let rate: u128 = stream_rate.0;
        let start_time: u64 = start.0;
        let end_time: u64 = end.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // Check the receiver and sender are not same
        require!(receiver != sender, "Sender and receiver cannot be Same");

        // Check the start and end timestamp is valid
        require!(
            start_time >= current_timestamp,
            "Start time cannot be in the past"
        );
        require!(end_time >= start_time, "End time cannot smaller than start time");

        // check the rate is valid
        require!(rate > 0, "Rate cannot be zero");
        require!(rate < MAX_RATE, "Rate is too high");

        // calculate the balance
        let stream_duration = end_time - start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        let near_token_id: AccountId;
        if is_native {
            near_token_id = NATIVE_NEAR_CONTRACT_ID.parse().unwrap(); // this will be ignored for native stream
        } else {
            near_token_id = contract_id;
        }

        Stream {
            id,
            sender,
            receiver,
            rate,
            is_paused: false,
            is_cancelled: false,
            balance: stream_amount,
            created: current_timestamp,
            start_time,
            end_time,
            withdraw_time: start_time,
            paused_time: 0,
            contract_id: near_token_id,
            can_cancel,
            can_update,
            is_native,
        }
    }
}
