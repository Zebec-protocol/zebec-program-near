use crate::*;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::{serde_json, AccountId, PromiseOrValue};

use constants::{MAINNET_TOKEN_ACCOUNTS, TESTNET_TOKEN_ACCOUNTS};

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
        // storage staking part
        let initial_storage_usage = env::storage_usage();
        let sender_account = sender.clone();

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

        // Verify that the user has enough balance to cover for storage used
        let storage_balance = self.accounts.get(&sender_account).unwrap();
        let final_storage_usage = env::storage_usage();
        let required_storage_balance =
            (final_storage_usage - initial_storage_usage) as Balance * env::storage_byte_cost();

        require!(
            storage_balance.available >= required_storage_balance.into(),
            format!(
                "Deposit more storage balance!, {}",
                required_storage_balance
            ),
        );

        // Update the global stream count for next stream
        self.current_id += 1;

        log!("Saving streams {}", stream.id);

        true
    }

    pub fn valid_ft_sender(account: AccountId) -> bool {
        // can only be called by stablecoin contract

        let req_account = account.as_str();

        // This is for testing purposes only, testing requires compiling with feature="testnet" so
        // that correct fungible token ids will be valid, this will not work on the mainnet
        if cfg!(feature = "testnet") {
            TESTNET_TOKEN_ACCOUNTS.contains(&req_account)
        } else if cfg!(feature = "mainnet") {
            MAINNET_TOKEN_ACCOUNTS.contains(&req_account)
        } else {
            env::panic_str("Error in compilation!");
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
        assert!(
            Self::valid_ft_sender(env::predecessor_account_id()),
            "Invalid or unknown fungible token used"
        );

        // checks that the sender_id is registered for staking storage
        require!(
            self.accounts.get(&sender_id).is_some(),
            "Sender account not registered!"
        );
        // msg contains the structure of the stream
        let res: Result<StreamView, _> = serde_json::from_str(&msg);
        if res.is_err() {
            // if err then return everything back
            return PromiseOrValue::Value(amount);
        }
        let _stream = res.unwrap();
        require!(
            _stream.method_name == "create_stream",
            "Invalid method name for creating fungible token stream"
        );
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
            PromiseOrValue::Value(U128::from(0))
        } else {
            PromiseOrValue::Value(amount)
        }
    }
}
