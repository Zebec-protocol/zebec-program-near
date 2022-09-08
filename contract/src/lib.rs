use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, PanicOnDefault, Promise, Timestamp};
use near_sdk::serde::{Serialize, Deserialize};
use near_sdk::json_types::{U128, U64};

pub const CREATE_STREAM_DEPOSIT: Balance = 100_000_000_000_000_000_000_000; // 0.1 NEAR
pub const ONE_YOCTO: Balance = 1;
pub const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000; // 1 NEAR
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR

mod views;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    current_id: u64,
    streams: UnorderedMap<u64, Stream>,
}

// Define the stream structure
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
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

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            current_id: 1,
            streams: UnorderedMap::new(b"p"),
        }
    }

    #[payable]
    pub fn create_stream(&mut self, receiver: AccountId, stream_rate: U128, start: U64, end: U64) {
        // convert id to native u128
        let rate: u128 = stream_rate.0;
        let start_time: u64 = start.0;
        let end_time: u64 = end.0;

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

        // check the amount send to the stream
        assert!(
            env::attached_deposit() == stream_amount,
            "The amount provided doesn't matches the stream {} {}",
            env::attached_deposit(),
            stream_amount
        );

        // check that the receiver and sender are not the same
        assert!(
            env::predecessor_account_id() != receiver,
            "Sender and receiver cannot be the same"
        );

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

        log!("Saving streams {}", stream_params.id);
    }

    pub fn withdraw(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.0;

        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(&id).unwrap();

        assert!(
            temp_stream.balance > 0,
            "No balance to withdraw"
        );

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
                if temp_stream.end_time > temp_stream.withdraw_time { // receiver has not withdrawn after stream ended
                    withdrawal_amount =
                    temp_stream.rate * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
                } else {
                    withdrawal_amount = 0;
                }
            }

            // Calculate the withdrawl amount
            let remaining_balance = temp_stream.balance - withdrawal_amount;
            assert!(remaining_balance > 0, "Already withdrawn");

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
                assert!(temp_stream.withdraw_time < temp_stream.end_time, "Already withdrawn");
                println!("{}, {}", temp_stream.end_time, temp_stream.withdraw_time);
                withdraw_time = env::block_timestamp();

                if temp_stream.is_paused {
                    time_elapsed = temp_stream.paused_time - temp_stream.withdraw_time;
                } else {
                    time_elapsed = temp_stream.end_time - temp_stream.withdraw_time;
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

            println!("{} {}", temp_stream.balance, withdrawal_amount);
            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;
            self.streams.insert(&id, &temp_stream);
        } else {
            // @todo proper error
            assert!(false, "You dont have permission to withdraw");
        }
    }

    pub fn pause(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.0;

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // Only the sender can pause the stream
        assert!(env::predecessor_account_id() == stream.sender);

        // Can only be paused after the stream has started and before it has ended
        let can_pause =
            env::block_timestamp() > stream.start_time && env::block_timestamp() < stream.end_time;
        assert!(
            can_pause,
            "Can only be pause after stream starts and before it has ended"
        );

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
        let id: u64 = stream_id.0;

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
        if env::block_timestamp() > stream.end_time {
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
        let id: u64 = stream_id.0;

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
    use super::*;
    use near_sdk::test_utils::accounts;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const NEAR: u128 = 1000000000000000000000000;

    #[test]
    fn initializes() {
        let contract = Contract::new();
        assert_eq!(contract.current_id, 1);
        assert_eq!(contract.streams.len(), 0);
    }

    #[test]
    #[should_panic]
    fn create_stream_invalid_amount() {
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 172800); // 2 days
        let sender = accounts(0);
        let receiver = accounts(1);
        let rate = U128::from(1 * NEAR);

        let mut contract = Contract::new();

        set_context_with_balance(sender, 200000 * NEAR);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
    }

    #[test]
    fn create_stream() {
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 172800); // 2 days
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);

        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 172800 * NEAR);

        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        assert_eq!(contract.current_id, 2);
        let params_key = 1;
        let stream = contract.streams.get(&params_key).unwrap();
        assert!(!stream.is_paused);
        assert_eq!(stream.id, 1);
        assert_eq!(stream.sender, sender.clone());
        assert_eq!(stream.receiver, accounts(1));
        assert_eq!(stream.balance, 172800 * NEAR);
        assert_eq!(stream.rate, rate.0);

        let stream_start_time: u64 = start_time.0;
        let stream_end_time: u64 = end_time.0;

        assert_eq!(stream.start_time, stream_start_time);
        assert_eq!(stream.end_time, stream_end_time);
        assert_eq!(stream.withdraw_time, stream_start_time);
        assert_eq!(stream.paused_time, 0);
    }

    #[test]
    fn withdraw_stream_receiver() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, start_time.0);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // 4. assert internal balance
        // Check the contract balance after stream is created
        set_context_with_balance_timestamp(env::current_account_id(), 10 * NEAR, start_time.0);
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert!(internal_balance == 10 * NEAR);

        // 3. call withdraw (action)
        let stream_start_time: u64 = start_time.0;
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 2);

        contract.withdraw(stream_id);

        // 4. assert internal balance
        let stream = contract.streams.get(&stream_id.0).unwrap();
        let internal_balance = stream.balance;

        assert_eq!(internal_balance, 8 * NEAR);
        assert_eq!(stream.withdraw_time, stream_start_time + 2);
    }

    #[test]
    #[should_panic(expected = "Cannot withdraw before the stream has ended")]
    fn withdraw_stream_sender_before_end() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, start_time.0);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // 3. call withdraw (action)
        let stream_start_time: u64 = start_time.0;
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 2);
        contract.withdraw(stream_id);
    }

    #[test]
    fn withdraw_stream_sender_after_end() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;
        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 2);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4);
        contract.resume(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 11);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 8 * NEAR);
    }

    #[test]
    fn withdraw_stream_sender_after_end_paused_stream() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;
        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4);
        contract.pause(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 11);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 4 * NEAR);
    }

    #[test]
    fn withdraw_stream_sender_after_end_multiple_pauses() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 6);
        contract.resume(stream_id);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 15);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 17);
        contract.resume(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 12 * NEAR);
    }

    #[test]
    fn withdraw_stream_receiver_after_end_multiple_pauses() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 6);
        contract.resume(stream_id);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 15);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 17);
        contract.resume(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 8 * NEAR);
    }


    #[test]
    fn test_sender_withdraws_before_sender() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // 3. sender call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 16 * NEAR);

        // 3. receiver call withdraw
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 25);
        contract.withdraw(stream_id);
        
        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    fn test_receiver_withdraws_before_sender() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // 3. sender call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 4 * NEAR);

        // 3. receiver call withdraw
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 25);
        contract.withdraw(stream_id);
        
        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    #[should_panic(expected = "Already withdrawn")]
    fn test_receiver_tries_multiple_withdraw() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // 3. receiver call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 4 * NEAR);

        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id); // panics here
    }

    #[test]
    #[should_panic(expected = "Already withdrawn")]
    fn test_sender_tries_multiple_withdraw() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);

        // 3. sender call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 16 * NEAR);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id); // panics here

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 16 * NEAR);
    }

    #[test]
    fn test_withdraw_after_end_on_paused() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        // 3. sender call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 21);
        contract.withdraw(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 9 * NEAR);

        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 25);
        contract.withdraw(stream_id); // panics here

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    fn test_pause() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10000);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10000 * NEAR);

        // 2. create stream
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(sender.clone(), 0, start + 10);
        // 3. pause
        contract.pause(stream_id);

        // 4. assert
        assert!(contract.streams.get(&stream_id.0).unwrap().is_paused);
    }

    #[test]
    #[should_panic(expected = "Cannot pause already paused stream")]
    fn double_pause_panic() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10000);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10000 * NEAR);

        // 2. create stream and pause
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 10);
        contract.pause(stream_id);

        // 3. pause
        contract.pause(stream_id);
    }

    #[test]
    fn test_resume() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10000);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10000 * NEAR);

        // 2. create stream and pause
        contract.create_stream(receiver.clone(), rate, start_time, end_time);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);
        contract.pause(stream_id);

        // 3. resume
        set_context_with_balance_timestamp(sender.clone(), 0, start + 4);
        contract.resume(stream_id);

        // 4. assert
        let stream = contract.streams.get(&stream_id.0).unwrap();
        assert!(!stream.is_paused);
        assert_eq!(stream.withdraw_time, start + 3);
    }

    // fn set_context(predecessor: AccountId) {
    //     let mut builder = VMContextBuilder::new();
    //     builder.predecessor_account_id(predecessor);
    //     testing_env!(builder.build());
    // }

    fn set_context_with_balance(predecessor: AccountId, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.attached_deposit(amount);
        testing_env!(builder.build());
    }

    fn set_context_with_balance_timestamp(predecessor: AccountId, amount: Balance, ts: u64) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.attached_deposit(amount);
        builder.block_timestamp(ts);
        testing_env!(builder.build());
    }
}
