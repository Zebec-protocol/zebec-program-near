use crate::*;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::{serde_json, PromiseOrValue};

pub use crate::views::*;

#[near_bindgen]
impl Contract {
    #[private]
    fn ft_create_stream(
        &mut self,
        stream_rate: U128,
        start_time: U64,
        end_time: U64,
        sender: AccountId,
        amount: U128,
        receiver: AccountId,
        contract_id: AccountId,
        can_cancel: bool,
        can_update: bool,
    ) -> bool {
        // check that the receiver and sender are not the same
        assert!(sender != receiver, "Sender and receiver cannot be the same");

        // convert id to native u128
        let rate: u128 = stream_rate.0;
        let start_time: u64 = start_time.0;
        let end_time: u64 = end_time.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        // Check the start and end timestamp is valid
        require!(
            start_time >= current_timestamp,
            "Start time cannot be in the past"
        );
        require!(end_time >= start_time, "Start time cannot be in the past");

        // check the rate is valid
        require!(rate > 0, "Rate cannot be zero");
        require!(rate < MAX_RATE, "Rate is too high");

        // calculate the balance is enough
        let stream_duration = end_time - start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        // check the amount send to the stream
        require!(
            amount.0 == stream_amount,
            "The amount provided doesn't matches the stream"
        );

        let params_key = self.current_id;

        let stream_params = Stream {
            id: params_key,
            sender,
            receiver,
            rate,
            is_paused: false,
            is_cancelled: false,
            balance: amount.0,
            created: current_timestamp,
            start_time,
            end_time,
            withdraw_time: start_time,
            paused_time: start_time,
            contract_id,
            can_cancel,
            can_update,
            is_native: false,
        };

        self.streams.insert(&params_key, &stream_params);
        self.current_id += 1;
        log!("Saving streams {}", stream_params.id);
        return true;
    }

    #[private]
    fn ft_update_stream(
        &mut self,
        stream_id: U64,
        sender: AccountId,
        start: U64,
        end: U64,
        rate: U128,
        update_amount: U128,
    ) -> bool {
        // convert to native u64
        let id: u64 = stream_id.0;
        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // check the stream can be udpated
        require!(sender == stream.sender, "You are not authorized to update this stream");
        require!(stream.can_update, "Stream cannot be updated");
        require!(!stream.is_cancelled, "Stream has already been cancelled");

        // convert id to native u128
        let rate = u128::from(rate);
        let start_time = u64::from(start);
        let end_time = u64::from(end);
        let amount = u128::from(update_amount);

        // Check the start and end timestamp is valid
        require!(
            stream.start_time > current_timestamp,
            "Cannot update: stream already started"
        );
        require!(
            start_time < end_time,
            "Start time should be less than end time"
        );

        if start_time != stream.start_time {
            require!(
                start_time >= current_timestamp,
                "Start time cannot be in the past"
            );
        }
        require!(rate > 0, "Rate cannot be zero");

        // check the rate is valid
        require!(rate < MAX_RATE, "Rate is too high");

        stream.start_time = start_time;
        stream.withdraw_time = start_time;
        stream.end_time = end_time;
        stream.rate = rate;

        // calculate the balance is enough
        let stream_duration = stream.end_time - stream.start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        if stream_amount > stream.balance {
            // check the amount send to the stream
            require!(
                amount == stream_amount - stream.balance,
                "The amount provided is not enough for the stream"
            );

            stream.balance += amount;
        }

        self.streams.insert(&id, &stream);

        return true;
    }

    pub fn valid_ft_sender(account: AccountId) -> bool {
        // can only be called by stablecoin contract
        // @todo add valid stablecoins (from mainnet) address here later
        let accounts: [AccountId; 2] = [
            "usdn.testnet".parse().unwrap(),
            "wrap.testnet".parse().unwrap(),
        ];
        if accounts.contains(&account) {
            // @todo: check if the accountID is in explicit (".near") or implicit format
            return true;
        } else {
            return false;
        }
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId, // EOA
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert!(Self::valid_ft_sender(env::predecessor_account_id()));
        // msg contains the structure of the stream
        let res: Result<FtStreamRequest, _> = serde_json::from_str(&msg);
        if res.is_err() {
            // if err then return everything back
            return PromiseOrValue::Value(amount);
        }

        match res.unwrap() {
            FtStreamRequest::Create(_stream) => {
                if self.ft_create_stream(
                    _stream.stream_rate,
                    _stream.start,
                    _stream.end,
                    sender_id, // EOA 
                    amount,
                    _stream.receiver,
                    env::predecessor_account_id(),
                    _stream.can_cancel,
                    _stream.can_update,
                ) {
                    return PromiseOrValue::Value(U128::from(0));
                } else {
                    return PromiseOrValue::Value(amount);
                }
            }
            FtStreamRequest::Update(_stream) => {
                if self.ft_update_stream(
                    _stream.stream_id,
                    sender_id,
                    _stream.start,
                    _stream.end,
                    _stream.stream_rate,
                    amount,
                ) {
                    return PromiseOrValue::Value(U128::from(0));
                } else {
                    return PromiseOrValue::Value(amount);
                }
            }
        }
    }
}
