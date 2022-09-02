use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise, Timestamp};


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
    withdraw_time: Timestamp,
    is_paused: bool,
    paused_time: Timestamp,
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
    #[private] // Public - but only callable by env::current_account_id()
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            current_id: 1,
            streams: UnorderedMap::new(b"p"),
        }
    }

    #[payable]
    pub fn create_stream(&mut self, receiver: AccountId, rate: u128, start_time:Timestamp, end_time: Timestamp) {
        // input validation
        let params_key = self.current_id;

        // calculate the balance is enough
        let stream_duration = end_time - start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        // check the amount send to the stream
        assert!(near_sdk::env::attached_deposit() >= stream_amount, "Not enough amount to fund the stream");

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

        // Update the global stream count
        self.current_id += 1;

        // Use env::log to record logs permanently to the blockchain!
        log!("Saving streams {}", stream_params.id);
    }

    pub fn withdraw(&mut self, stream_id: &u64) {
        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(stream_id).unwrap();

        // assert the stream has started
        assert!(env::block_timestamp() > temp_stream.start_time, "The stream has not started yet");

        // assert the stream is not paused
        assert!(temp_stream.is_paused == false, "The stream is paused");

        // Case: sender withdraws excess amount from the stream after it has ended
        if env::predecessor_account_id() == temp_stream.sender {
            assert!(env::block_timestamp() > temp_stream.end_time);

            // Calculate the withdrawl amount
            let withdrawal_amount = temp_stream.rate * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
            let remaining_balance = temp_stream.balance - withdrawal_amount;
            
            // Transfer tokens to the sender
            let receiver = temp_stream.sender.clone();
            Promise::new(receiver).transfer(remaining_balance);

            // Update stream and save
            temp_stream.balance -= withdrawal_amount;
            self.streams.insert(stream_id, &temp_stream);
            return;
        } else if env::predecessor_account_id() == temp_stream.receiver {
            let time_elapsed: u64;
            let withdraw_time: u64;

            // Calculate the elapsed time
            if env::block_timestamp() >= temp_stream.end_time {
                time_elapsed = temp_stream.withdraw_time - temp_stream.end_time;
                withdraw_time = env::block_timestamp();

                if temp_stream.is_paused {
                    temp_stream.withdraw_time += temp_stream.end_time - temp_stream.paused_time;
                }
            } else if temp_stream.is_paused {
                time_elapsed = temp_stream.withdraw_time - temp_stream.paused_time;
                withdraw_time = temp_stream.paused_time;
            } else {
                time_elapsed = temp_stream.withdraw_time - env::block_timestamp();
                withdraw_time = env::block_timestamp();
            }

            // Calculate the withdrawal amount
            let withdrawal_amount = temp_stream.rate * u128::from(time_elapsed);

            // Transfer the tokens to the receiver
            let receiver = temp_stream.receiver.clone();
            Promise::new(receiver).transfer(withdrawal_amount);

            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;
            self.streams.insert(stream_id, &temp_stream);
            return;
        } else {
            // @todo proper error
            panic!();
        }
    }

    pub fn pause(&mut self, stream_id: &u64) {
        // Only the sender can pause the stream
        assert!(env::predecessor_account_id() == self.streams.get(stream_id).unwrap().sender);

        // assert that the stream is already paused
        let is_paused = self.streams.get(stream_id).unwrap().is_paused;
        assert!(is_paused == false, "Cannot pause already paused stream");

        // update the stream state
        let mut temp_stream = self.streams.get(stream_id).unwrap();
        temp_stream.is_paused = true;
        temp_stream.paused_time = env::block_timestamp();
        self.streams.insert(stream_id, &temp_stream);

        // Log
        log!("Stream paused: {}", temp_stream.id);
    }

    pub fn resume(&mut self, stream_id: &u64) {
        // Only the sender can resume the stream
        assert!(env::predecessor_account_id() == self.streams.get(stream_id).unwrap().sender);

        // assert that the stream is already paused
        let is_paused = self.streams.get(stream_id).unwrap().is_paused;
        assert!(is_paused == true, "Cannot resume unpaused stream");

        let mut temp_stream = self.streams.get(stream_id).unwrap();
        temp_stream.is_paused = false;
        temp_stream.withdraw_time += env::block_timestamp() - temp_stream.paused_time;
        self.streams.insert(stream_id, &temp_stream);

        // Log
        log!("Stream resumed: {}", temp_stream.id);
    }

    pub fn cancel(&mut self, stream_id: &u64) {
        // Get the stream
        let mut temp_stream = self.streams.get(stream_id).unwrap();

        // Only the sender can cancel the stream
        assert!(env::predecessor_account_id() == temp_stream.sender);

        // Stream can only be cancelled if it has not ended
        assert!(temp_stream.end_time > env::block_timestamp(), "Stream already ended");

        // Amounts to refund to the sender and the receiver
        let sender_amt: u128;
        let receiver_amt: u128;

        // Calculate the amount to refund to the receiver
        if temp_stream.is_paused {
            receiver_amt = u128::from(temp_stream.paused_time - temp_stream.withdraw_time) * temp_stream.rate;
        } else {
            receiver_amt = u128::from(env::block_timestamp() - temp_stream.withdraw_time) * temp_stream.rate;
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
        self.streams.insert(stream_id, &temp_stream);

        // Log
        log!("Stream cancelled: {}", temp_stream.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    // const BENEFICIARY: &str = "beneficiary";
    // const BENEFICIARY2: &str = "beneficiary2";
    // const NEAR: u128 = 1000000000000000000000000;

    #[test]
    fn initializes() {
        let contract = Contract::new();
        // current_id: U128(1),
        assert_eq!(contract.current_id, 1)
    }

    #[test]
    fn create_stream() {
        let mut contract = Contract::new();

        // Make a payment
        // set_context("caller_a", 1 * NEAR);
        // contract.send_payment(BENEFICIARY.parse().unwrap());

        // let sent_amount = contract.get_balanceof(BENEFICIARY.parse().unwrap());

        // // Check the donation was recorded correctly
        // assert_eq!(sent_amount.amount.0, 1 * NEAR);

        // // Make another donation
        // set_context("caller2", 2 * NEAR);
        // contract.send_payment(BENEFICIARY2.parse().unwrap());
        // let sent_amount2 = contract.get_balanceof(BENEFICIARY2.parse().unwrap());

        // // Check the donation was recorded correctly
        // assert_eq!(sent_amount2.amount.0, 2 * NEAR);
    }

    // Auxiliar fn: create a mock context
    fn set_context(predecessor: &str, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap());
        builder.attached_deposit(amount);

        testing_env!(builder.build());
    }

    // Auxiliar fn: create a mock context
    fn set_context(predecessor: &str, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap());
        builder.attached_deposit(amount);

        testing_env!(builder.build());
    }

}
