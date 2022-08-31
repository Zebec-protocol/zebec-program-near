use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise, Timestamp};

// Define the default message

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
    id: String,
    sender: AccountId,
    receiver: AccountId,
    balance: Balance, // 10^-24 yocto
    rate: Balance,
    created: Timestamp,
    status: StreamStatus,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, Serialize, PartialEq, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub enum StreamStatus {
    Initialized,
    Active,
    Paused,
    Finished,
}

// new @todo

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
    pub fn create_stream(&mut self, receiver: AccountId, rate: u128, status: StreamStatus) {
        // input validation
        let params_key = self.current_id; // @todo self.current_id ++;

        let stream_params = Stream {
            id: String::from("1"),
            sender: env::predecessor_account_id(),
            receiver,
            rate,
            status: StreamStatus::Initialized, //staus to initialzed
            balance: env::attached_deposit(),
            created: env::block_timestamp(),
        };
        // save the sent native balance to this contract

        self.streams.insert(&params_key, &stream_params);
        // Use env::log to record logs permanently to the blockchain!
        log!("Saving streams {}", stream_params.id);
    }

    pub fn withdraw(&mut self, stream_id: &u64, amount: Balance) {
        // add input guards/ data sanity
        // guards
        //  status : active
        // amount > withdrawal balance
        // @todo settle this reference
        let mut temp_stream = self.streams.get(stream_id).unwrap(); // panic on error

        // assert that the caller has enough balance to withdraw
        let temp_amount = temp_stream.balance;
        // calculate the withdrawable amount
        //    let time_elapsed = (temp_params.created - env::block_timestamp()).into(U128);
        let time_elapsed = temp_stream.created - env::block_timestamp();

        // U128::from(from_index.unwrap_or(U128(0)));

        let withdrawal_amount = temp_stream.rate * u128::from(time_elapsed);

        assert!(u128::from(withdrawal_amount) == amount, "amount mismatch! ");
        let receiver = temp_stream.receiver.clone();
        Promise::new(receiver).transfer(withdrawal_amount);
        temp_stream.balance -= withdrawal_amount;
        self.streams.insert(stream_id, &temp_stream);
    }
    pub fn pause(&mut self, stream_id: &u64) {
        assert!(env::predecessor_account_id() == self.streams.get(stream_id).unwrap().sender);
        // update the status to paused
        let mut temp_stream = self.streams.get(stream_id).unwrap();
        temp_stream.status = StreamStatus::Paused;
        self.streams.insert(stream_id, &temp_stream);
    }

    pub fn resume(&mut self, stream_id: &u64) {
        let currnet_status = self.streams.get(stream_id).unwrap().status;
        assert!(currnet_status == StreamStatus::Paused);
        let mut temp_stream = self.streams.get(stream_id).unwrap();
        temp_stream.status = StreamStatus::Active;
        self.streams.insert(stream_id, &temp_stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const BENEFICIARY: &str = "beneficiary";
    const BENEFICIARY2: &str = "beneficiary2";
    const NEAR: u128 = 1000000000000000000000000;

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
