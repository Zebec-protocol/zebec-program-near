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
        // Check the receiver and sender are not same
        require!(receiver != sender, "Sender and receiver cannot be Same");

        let params_key = self.current_id;

        let stream: Stream = self.validate_stream(
            U64::from(params_key),
            sender,
            receiver,
            stream_rate,
            start_time,
            end_time,
            can_cancel,
            can_update,
            false,
            contract_id,
        );

        // check the amount send to the stream
        require!(
            amount.0 == stream.balance,
            "The amount provided doesn't matches the stream"
        );

        // Save the stream
        self.streams.insert(&params_key, &stream);

        // Update the global stream count for next stream
        self.current_id += 1;

        log!("Saving streams {}", stream.id);

        true
    }

    pub fn valid_ft_sender(account: AccountId) -> bool {
        // can only be called by stablecoin contract
        // @todo add valid stablecoins (from mainnet) address here later
        let accounts: [AccountId; 2] = [
            "usdn.testnet".parse().unwrap(),
            "wrap.testnet".parse().unwrap(),
        ];
        // @todo: check if the accountID is in explicit (".near") or implicit format
        accounts.contains(&account)
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
        let res: Result<StreamView, _> = serde_json::from_str(&msg);
        if res.is_err() {
            // if err then return everything back
            return PromiseOrValue::Value(amount);
        }
        let _stream = res.unwrap();
        require!(_stream.method_name == "create_stream".to_string());
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
}
