use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise, Timestamp};

use near_sdk::json_types::{U128, U64};

pub const CREATE_STREAM_DEPOSIT: Balance = 100_000_000_000_000_000_000_000; // 0.1 NEAR
pub const ONE_YOCTO: Balance = 1;
pub const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000; // 1 NEAR
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    current_id: u64,
    streams: UnorderedMap<u64, Stream>,
}

// Define the stream structure
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Stream {
    id: u64,
    sender: AccountId,
    receiver: AccountId,
    balance: Balance,
    rate: Balance,
    created: Timestamp,
    start_time: Timestamp,
    end_time: Timestamp,
    withdraw_time: Timestamp, // last withdraw time 
    is_paused: bool,
    paused_time: Timestamp, // last paused time 
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            current_id: 1,
            streams: UnorderedMap::new(b"p"),
        }
    }
}

#[near_bindgen]
impl Contract {
    #[init]
    #[private]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Default::default()
    }

    #[payable]
    pub fn create_stream(&mut self, receiver: AccountId, stream_rate: U128, start_time:Timestamp, end_time: Timestamp) {
        // convert id to native u128
        let rate: u128 = stream_rate.into();

        // Check the start and end timestamp is valid
        assert!(start_time >= env::block_timestamp(), "Start time cannot be in the past");
        assert!(end_time >= start_time, "Start time cannot be in the past");


        // check the rate is valid
        assert!(rate > 0, "Rate cannot be zero");
        assert!(rate < MAX_RATE, "Rate cannot be zero");

        // calculate the balance is enough
        let stream_duration = end_time - start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        // check the amount send to the stream
        assert!(env::attached_deposit() == stream_amount, "Not enough amount to fund the stream");
        
        // check that the receiver and sender are not the same
        assert!(env::predecessor_account_id() != receiver, "Sender and receiver cannot be the same");

        let params_key = self.current_id;

        let stream_params = Stream {
            id: params_key,
            sender: env::predecessor_account_id(),
            receiver,
            rate,
            is_paused: false,
            balance: env::attached_deposit(),
            created: env::block_timestamp(),
            start_time,
            end_time,
            withdraw_time: start_time,
            paused_time: start_time,
        };

        // Save the stream
        self.streams.insert(&params_key, &stream_params);

        // Update the global stream count for next stream
        self.current_id += 1;

        // Use env::log to record logs permanently to the blockchain!
        log!("Saving streams {}", stream_params.id);
    }

    pub fn withdraw(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.into();

        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(&id).unwrap();

        // assert the stream has started
        assert!(
            env::block_timestamp() > temp_stream.start_time,
            "The stream has not started yet"
        );

        // Case: sender withdraws excess amount from the stream after it has ended
        if env::predecessor_account_id() == temp_stream.sender {
            assert!(env::block_timestamp() > temp_stream.end_time);

            let withdrawal_amount: u128;
            
            if stream.is_paused {
                withdrawal_amount = temp_stream.rate * u128::from(temp_stream.paused_time - temp_stream.withdraw_time);
            } else {
                withdrawal_amount = temp_stream.rate * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
            }

            // Calculate the withdrawl amount
            let remaining_balance = temp_stream.balance - withdrawal_amount;

            // Transfer tokens to the sender
            let receiver = temp_stream.sender.clone();
            Promise::new(receiver).transfer(remaining_balance);

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

            // Transfer the tokens to the receiver
            let receiver = temp_stream.receiver.clone();
            assert!(withdrawal_amount > 0);
            Promise::new(receiver).transfer(withdrawal_amount);

            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;
            self.streams.insert(&id, &temp_stream);

        // 
        } else {
            // @todo proper error
            panic!();
        }
    }

    pub fn pause(&mut self, stream_id: U64) {
        // Only the sender can pause the stream
        assert!(env::predecessor_account_id() == stream.sender);

        // convert id to native u64
        let id: u64 = stream_id.into();

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // Can only be paused after the stream has started and before it has ended
        let can_pause = env::block_timestamp() > stream.start_time && env::block_timestamp() < stream.end_time;
        assert!(can_pause, "Can only be pause after stream starts and before it has ended");

        // assert that the stream is already paused
        assert!(!stream.is_paused, "Cannot pause already paused stream");

        // update the stream state
        stream.is_paused = true;
        stream.paused_time = env::block_timestamp();
        self.streams.insert(&id, &stream);

        // Log
        log!("Stream paused: {}", stream.id);
    }

    pub fn resume(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.into();

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // Only the sender can resume the stream
        assert!(env::predecessor_account_id() == stream.sender);

        // assert that the stream is already paused
        let is_paused = self.streams.get(&id).unwrap().is_paused;
        assert!(is_paused, "Cannot resume unpaused stream");

        // resume the stream
        stream.is_paused = false;

        // Update the withdraw_time so that the receiver will not be 
        // able to withdraw fund for paused time
        if env::block_timestamp() > stream.start_time {
            stream.withdraw_time += stream.end_time - stream.paused_time;
        } else {
            stream.withdraw_time += env::block_timestamp() - stream.paused_time;
        }
        // Reset the paused_time and save
        stream.paused_time = 0;
        self.streams.insert(&id, &stream);

        // Log
        log!("Stream resumed: {}", stream.id);
    }

    pub fn cancel(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.into();

        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();

        // Only the sender can cancel the stream
        assert!(env::predecessor_account_id() == temp_stream.sender);

        // Stream can only be cancelled if it has not ended
        assert!(
            temp_stream.end_time > env::block_timestamp(),
            "Stream already ended"
        );

        // Amounts to refund to the sender and the receiver
        let sender_amt: u128;
        let receiver_amt: u128;

        // Calculate the amount to refund to the receiver
        if temp_stream.is_paused {
            receiver_amt =
                u128::from(temp_stream.paused_time - temp_stream.withdraw_time) * temp_stream.rate;
        } else {
            receiver_amt =
                u128::from(env::block_timestamp() - temp_stream.withdraw_time) * temp_stream.rate;
        }

        // Calculate the amoun to refund to the sender
        sender_amt = temp_stream.balance - receiver_amt;

        // Refund the amounts to the sender and the receiver respectively
        let sender = temp_stream.sender.clone();
        let receiver = temp_stream.receiver.clone();
        Promise::new(sender).transfer(sender_amt);
        Promise::new(receiver).transfer(receiver_amt);

        // Update the stream balance and save
        temp_stream.balance = 0;
        self.streams.insert(&id, &temp_stream);

        // Log
        log!("Stream cancelled: {}", temp_stream.id);
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Sub;

    use super::*;
    use near_sdk::test_utils::accounts;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, VMContext};

    const NEAR: u128 = 1000000000000000000000000;
    // let receiver:AccountId = AccountId::new_unchecked!("receiver.near".to_string());

    #[test]
    fn initializes() {
        let contract = Contract::new();
        assert_eq!(contract.current_id, 1)
    }

    #[test]
    fn create_stream() {
        let start_time: Timestamp = env::block_timestamp();
        let end_time: Timestamp = start_time + 1728000; // 20 days
        let sender = "alice"; // alice
        let receiver = &accounts(1); // bob
        let rate = 1;

        set_context(sender, 1);
        let mut contract = Contract::new();

        set_context("sender", 100 * NEAR);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        assert_eq!(contract.current_id, 2);
        let params_key = 1;
        let stream = contract.streams.get(&params_key).unwrap();
        assert!(!stream.is_paused);
        // @todo add more here
    }

    #[test]
    fn test_withdraw_stream() {
        // 1. create_stream contract
        let start_time: Timestamp = env::block_timestamp();
        let end_time: Timestamp = start_time + 5;
        let sender = "alice"; // alice
        let receiver = &accounts(1); // bob
        let rate = 1 * NEAR;
        let mut contract = Contract::new();

        set_context_withdraw(sender, 100 * NEAR, start_time);
        // 2. create stream
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id:u64 = 1;
        // 3. call withdraw (action)
        set_context_withdraw("bob", 0, start_time + 2);
        contract.withdraw(&stream_id);
        let internal_balance = contract.streams.get(&stream_id).unwrap().balance;
        let contract_balance = env::account_balance();
        // 4. assert internal balance
        assert!(contract_balance == 98 * NEAR);
        assert!(internal_balance == 98 * NEAR);
        // 5. receiver balance


    }
    #[test]
    fn test_pause() {
        // 1. create_stream contract
        let start_time: Timestamp = env::block_timestamp();
        let end_time: Timestamp = start_time+ 1728000; // 20 days ?
        let sender = "alice"; // alice
        let receiver = &accounts(1); // bob
        let rate = 1;
        set_context(sender, 1);
        let mut contract = Contract::new();

        set_context("sender", 100 * NEAR);
        // 2. create stream
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = 1;
        // 3. pause
        contract.pause(&stream_id);
        // 4. assert
        assert!(contract.streams.get(&stream_id).unwrap().is_paused);
    }
    #[test]
    #[should_panic(expected="Cannot pause already paused stream")]
    fn double_pause_panic() {
        // 1. create_stream contract
        let start_time: Timestamp = env::block_timestamp();
        let end_time: Timestamp = start_time+ 1728000; // 20 days ?
        let sender = "alice"; // alice
        let receiver = &accounts(1); // bob
        let rate = 1;
        set_context(sender, 1);
        let mut contract = Contract::new();

        set_context("sender", 100 * NEAR);
        // 2. create stream
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = 1;
        // 3. pause
        contract.pause(&stream_id);
        // 4. double pause panic
        assert!(!contract.streams.get(&stream_id).unwrap().is_paused);
    }

    #[test]
    fn test_resume() {
        // 1. create_stream contract
        let start_time: Timestamp = env::block_timestamp();
        let end_time: Timestamp = start_time+ 1728000; // 20 days ?
        let sender = "alice"; // alice
        let receiver = &accounts(1); // bob
        let rate = 1;
        set_context(sender, 1);
        let mut contract = Contract::new();

        set_context("sender", 100 * NEAR);
        // 2. create stream
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = 1;
        // 3. pause first
        contract.pause(&stream_id);
        // 4. resume()
        contract.resume(&stream_id);
        // 5. assert not paused
        assert!(!contract.streams.get(&stream_id).unwrap().is_paused);
    }

    // Auxiliar fn: create a mock context
    fn set_context(predecessor: &str, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap());
        builder.attached_deposit(amount);
        // builder.block_index(block_index);
        testing_env!(builder.build());
    }

    fn set_context_withdraw(predecessor: &str, amount: Balance, ts: u64) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap());
        builder.attached_deposit(amount);
        // let mut current_block_time = env::block_timestamp();
        builder.block_timestamp(ts);
        testing_env!(builder.build());
    }
}
