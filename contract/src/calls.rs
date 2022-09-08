use crate::*;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::serde_json;
use near_sdk::{ PromiseOrValue, Timestamp};

pub use crate::views::*;

#[near_bindgen]
impl Contract {
    #[private]
    fn ft_create_stream(
        &mut self,
        stream_rate: U128,
        start_time: Timestamp,
        end_time: Timestamp,
        sender: AccountId,
        amount: U128,
        receiver: AccountId,
        contract_id: AccountId,
    ) -> bool {
        // check that the receiver and sender are not the same
        assert!(sender != receiver, "Sender and receiver cannot be the same");
        // convert id to native u128
        let rate: u128 = stream_rate.into();

        // Check the start and end timestamp is valid
        assert!(
            start_time >= env::block_timestamp(),
            "Start time cannot be in the past"
        );
        assert!(end_time >= start_time, "Start time cannot be in the past");

        // check the rate is valid
        assert!(rate > 0, "Rate cannot be zero");
        assert!(rate < MAX_RATE, "Rate is too high");

        // calculate the balance is enough
        let stream_duration = end_time - start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        assert!(stream_amount == amount.into());

        let params_key = self.current_id;

        let stream_params = Stream {
            id: params_key,
            sender: sender,
            receiver,
            rate,
            is_paused: false,
            balance: amount.into(),
            created: env::block_timestamp(),
            start_time,
            end_time,
            withdraw_time: start_time,
            paused_time: start_time,
            contract_id: contract_id,
        };

        self.streams.insert(&params_key, &stream_params);
        self.current_id += 1;
        log!("Saving streams {}", stream_params.id);
        return true;
    }

    pub fn valid_ft_sender(account: AccountId) -> bool {
        // can only be called by stablecoin contract
        // @todo add valid stablecoins (from mainnet) address here later
        // pub const accounts: [AccountId; 1] = ["usdn.testnet".parse().unwrap()];
        if account == "usdn.testnet".parse().unwrap() {
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
        let key: Result<StreamView, _> = serde_json::from_str(&msg);
        if key.is_err() {
            // if err then return everything back
            return PromiseOrValue::Value(amount);
        }
        let _stream = key.unwrap();
        if self.ft_create_stream(
            _stream.rate,
            _stream.start_time,
            _stream.end_time,
            sender_id, // EOA
            amount,
            _stream.receiver,
            env::predecessor_account_id(),
        ) {
            return PromiseOrValue::Value(U128::from(0));
        } else {
            return PromiseOrValue::Value(amount);
        }
    }
}