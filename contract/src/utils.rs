use std::collections::HashMap;

use crate::*;

use constants::{
    NATIVE_NEAR_CONTRACT_ID,
    GAS_FOR_FT_TRANSFER,
    GAS_FOR_RESOLVE_TRANSFER,
    GAS_FOR_FT_TRANSFER_CALL,
    FEE_BPS_DIVISOR
};

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
        require!(
            end_time >= start_time,
            "End time cannot smaller than start time"
        );

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


    // ------------------- owner functions --------------------------------------
    fn assert_owner(&self) {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "Not owner");
    }

    /// Get the owner of this contract.
    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    fn assert_manager(&self) {
        require!(env::predecessor_account_id() == self.manager_id, "Not Manager");
    }

    /// Change owner. Only can be called by owner.
    #[payable]
    pub fn set_owner(&mut self, owner_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.owner_id = owner_id;
    }

    /// Extend whitelisted tokens with new tokens. Only can be called by owner.
    #[payable]
    pub fn extend_whitelisted_tokens(&mut self, tokens: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        for token in tokens {
            self.whitelisted_tokens.insert(&token);
        }
    }

    /// Remove whitelisted token. Only can be called by owner.
    #[payable]
    pub fn remove_whitelisted_tokens(&mut self, tokens: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_owner();
        for token in tokens {
            let exist = self.whitelisted_tokens.remove(&token);
            assert!(exist, "Token not in the list");
        }
    }

    // view whitelisted tokens
    pub fn get_whitelisted_tokens(&self) -> Vec<AccountId> {
        self.whitelisted_tokens.to_vec()
    }

    /// delete streams. Only can be called by manager.
    #[payable]
    pub fn delete_streams(&mut self, stream_ids: Vec<U64>) {
        assert_one_yocto();
        self.assert_manager();
        for stream_id in stream_ids  {
            self.delete_stream(stream_id);
        }
    }

    // deletes the stream and frees on-chain storage
    fn delete_stream(&mut self, stream_id: U64) {
        let stream = self.streams.get(&stream_id.0).unwrap();
        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        require!(stream.end_time < current_timestamp);
        require!(
            stream.balance == 0,
            "There are still some funds in the stream"
        );
        self.streams.remove(&stream.id);
    }

    // assert_one_yocto()
    #[payable]
    pub fn change_fee_rate(&mut self, new_rate: U64) {
        assert_one_yocto();
        self.assert_owner();
        require!(new_rate.0 <= self.max_fee_rate, "Rate cannot be greater than max fee_rate");
        self.fee_rate = new_rate.0;
    }

    #[payable]
    pub fn change_fee_receiver(&mut self, new_receiver: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        self.fee_receiver = new_receiver;
    }

    // claim accumulated fee by fee_receiver
    #[payable]
    pub fn claim_fee_ft(&mut self, contract_id: AccountId, amount: U128) -> PromiseOrValue<bool>{
        assert_one_yocto();
        require!(env::predecessor_account_id() == self.fee_receiver, "Not fee receiver!");

        let fee_amount = self.accumulated_fees.get(&contract_id).unwrap();

        require!(fee_amount >= amount.0, "cannot claim fee amount greater than accumulated fee!");

        let remaining_amount = fee_amount - amount.0;

        self.accumulated_fees.insert(&contract_id, &remaining_amount);
        require!((env::prepaid_gas() - env::used_gas()) > GAS_FOR_FT_TRANSFER_CALL, "More gas is required");
        ext_ft_transfer::ext(contract_id.clone())
            .with_attached_deposit(1)
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(self.fee_receiver.clone(), amount.into(), None)
            .then(
                Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                .internal_resolve_claim_fee_ft(
                    contract_id,
                    amount,
                ),
            )
            .into()
    }

    #[private]
    pub fn internal_resolve_claim_fee_ft(
        &mut self,
        contract_id: AccountId,
        amount: U128,

    ) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        if !res {
            let fee_amount = self.accumulated_fees.get(&contract_id).unwrap();
            let restore_amount = fee_amount + amount.0;
            self.accumulated_fees.insert(&contract_id, &restore_amount);
        }
        res
    }

    #[private]
    pub fn internal_resolve_claim_fee_native(
        &mut self,
        amount: U128,

    ) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        if !res {
            self.native_fees += amount.0;
        }
        res
    }

    // claim accumulated fee by fee_receiver
    #[payable]
    pub fn claim_fee_native(&mut self, amount: U128) -> PromiseOrValue<bool>{
        assert_one_yocto();
        require!(env::predecessor_account_id() == self.fee_receiver, "Not fee receiver!");
        let fee_amount = self.native_fees;
        require!(fee_amount >= amount.0, "cannot claim amount greater than accumulated!");
        self.native_fees = fee_amount - amount.0;
        Promise::new(self.fee_receiver.clone()).transfer(amount.0).then(
            Self::ext(env::current_account_id()).internal_resolve_claim_fee_native(
                amount.into(),
            )
        ).into()
    }

    // utility function to view the claimable fee, for testing purposes
    pub fn view_claimable_fee(&self) -> HashMap<AccountId, U128> {
        let mut _hashmap = HashMap::new();

        for a in self.accumulated_fees.keys() {
            _hashmap.insert(a.clone(), U128::from(self.accumulated_fees.get(&a).unwrap()));
        }

        // Native fees accumulated
        _hashmap.insert("native.testnet".parse().unwrap(), U128(self.native_fees));
        _hashmap
    }

    pub fn calculate_fee_amount(&self, amount:u128) -> u128 {
        (amount * u128::from(self.fee_rate)) / u128::from(FEE_BPS_DIVISOR)
    }

    pub fn valid_ft_sender(&self, account: AccountId) -> bool {
        self.whitelisted_tokens.contains(&account)
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
        let contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"
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
        let mut contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"
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
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 11);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // call withdraw by receiver
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 11);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // charlie as manager
        set_context_with_balance_timestamp(accounts(3), 1, stream_start_time + 11);
        let stream_ids: Vec<U64> = vec![stream_id];

        contract.delete_streams(stream_ids);
    }
}
