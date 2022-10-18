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
            locked: false,
        }
    }

    pub fn delete_streams(&mut self, stream_ids: Vec<U64>) {
        require!(env::predecessor_account_id() == self.manager, "only the manager can delete streams");
        for stream_id in stream_ids  {
            self.delete_stream(stream_id);
        }
    }

    // deletes the stream and frees on-chain storage
    #[private]
    fn delete_stream(&mut self, stream_id: U64) {
        let stream = self.streams.get(&stream_id.0).unwrap();
        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        require!(stream.end_time < current_timestamp);
        require!(stream.balance == 0, "There are still some funds in the stream");
        self.streams.remove(&stream.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_contract_standards::storage_management::StorageManagement;
    use near_sdk::test_utils::accounts;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const NEAR: u128 = 1000000000000000000000000;

    #[test]
    fn initializes() {
        let contract = Contract::new(accounts(2));
        assert_eq!(contract.current_id, 1);
        assert_eq!(contract.streams.len(), 0);
    }

    fn set_context_with_balance(predecessor: AccountId, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.attached_deposit(amount);
        testing_env!(builder.build());
    }

    fn register_user(contract: &mut Contract, user_id: AccountId) {
        set_context_with_balance(user_id.clone(), 1 * NEAR);
        contract.storage_deposit(Some(user_id), Some(false));
    }

    fn set_context_with_balance_timestamp(predecessor: AccountId, amount: Balance, ts: u64) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.attached_deposit(amount);
        builder.block_timestamp(ts * 1e9 as u64);
        testing_env!(builder.build());
    }

    #[test]
    fn test_delete_stream() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new(accounts(2)); // charlie
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;
        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 2); // 2
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4); // 2
        contract.resume(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 11);
        contract.withdraw(stream_id);

        // call withdraw by receiver
        set_context_with_balance_timestamp(receiver.clone(), 0, stream_start_time + 11);
        contract.withdraw(stream_id);

        // charlie as manager
        set_context_with_balance_timestamp(accounts(2), 0, stream_start_time + 11);
        let stream_ids: Vec<U64>  = vec![stream_id];
        contract.delete_streams(stream_ids);
    }
}
