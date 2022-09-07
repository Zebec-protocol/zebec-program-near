use crate::*;

// use near_contract_standards::fungible_token::FungibleToken;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use near_sdk::serde_json;
use near_sdk::{ext_contract, Promise, PromiseOrValue, Timestamp};

pub use crate::views::*;

#[ext_contract(ext_ft_transfer)]
trait NEP141 {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

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

    // currently only supports only EOA for withdraw
    // @todo think what if the receiver is a contract
    pub fn ft_withdraw(&mut self, stream_id: U64) {
        // returns FT tokens instead of NEAR

        // USN decimals() : 18

        let id: u64 = stream_id.into();

        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(&id).unwrap();

        assert!(temp_stream.contract_id == "NEAR".parse().unwrap());

        // assert the stream has started
        assert!(
            env::block_timestamp() > temp_stream.start_time,
            "The stream has not started yet"
        );

        // Case: sender withdraws excess amount from the stream after it has ended
        if env::predecessor_account_id() == temp_stream.sender {
            assert!(
                env::block_timestamp() > temp_stream.end_time,
                "Cannot withdraw before the stream has ended"
            );

            // Amount that has been streamed to the receiver
            let withdrawal_amount: u128;

            if temp_stream.is_paused {
                withdrawal_amount = temp_stream.rate
                    * u128::from(temp_stream.paused_time - temp_stream.withdraw_time);
            } else {
                withdrawal_amount =
                    temp_stream.rate * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
            }

            // Calculate the withdrawl amount
            let remaining_balance = temp_stream.balance - withdrawal_amount;

            // Transfer tokens to the sender
            let receiver = temp_stream.sender.clone();
            let contract_id = temp_stream.contract_id.clone();
            // NEP141 : ft_transfer()
            // @todo
            ext_ft_transfer::ext(contract_id).ft_transfer(receiver, remaining_balance.into(), None);

            // Update stream and save
            temp_stream.balance -= remaining_balance;
            self.streams.insert(&id, &temp_stream);

        // Case: Receiver can withdraw the amount fromt the stream
        } else if env::predecessor_account_id() == temp_stream.receiver {
            let time_elapsed: u64;
            let withdraw_time: u64;

            // Calculate the elapsed time
            if env::block_timestamp() >= temp_stream.end_time {
                time_elapsed = temp_stream.end_time - temp_stream.withdraw_time;
                withdraw_time = env::block_timestamp();

                // this block is not necessary
                if temp_stream.is_paused {
                    temp_stream.withdraw_time += temp_stream.end_time - temp_stream.paused_time;
                }
            } else if temp_stream.is_paused {
                time_elapsed = temp_stream.paused_time - temp_stream.withdraw_time;
                withdraw_time = temp_stream.paused_time;
            } else {
                time_elapsed = env::block_timestamp() - temp_stream.withdraw_time;
                withdraw_time = env::block_timestamp();
            }

            // Calculate the withdrawal amount
            let withdrawal_amount = temp_stream.rate * u128::from(time_elapsed);
            assert!(withdrawal_amount > 0);

            // Transfer the tokens to the receiver
            let receiver = temp_stream.receiver.clone();
        
            let contract_id = temp_stream.contract_id.clone();
            ext_ft_transfer::ext(contract_id).ft_transfer(receiver, withdrawal_amount.into(), None);
            // Promise::new(receiver).transfer(withdrawal_amount);

            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;
            self.streams.insert(&id, &temp_stream);
        } else {
            // @todo proper error
            panic!();
        }
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
            // @todo
            return PromiseOrValue::Value(amount);
        }
        let _stream = key.unwrap(); // stream struct sent from via stablecoin in msg:String
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
