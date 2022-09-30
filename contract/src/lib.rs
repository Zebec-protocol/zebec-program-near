use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, log, near_bindgen, require, AccountId, Balance, Gas, PanicOnDefault,
    Promise, PromiseOrValue, PromiseResult, Timestamp,
};
use near_sdk::utils::assert_one_yocto;

mod calls;
mod views;
mod utils;

pub const CREATE_STREAM_DEPOSIT: Balance = 100_000_000_000_000_000_000_000; // 0.1 NEAR
pub const ONE_YOCTO: Balance = 1;
pub const ONE_NEAR: Balance = 1_000_000_000_000_000_000_000_000; // 1 NEAR
pub const MAX_RATE: Balance = 100_000_000_000_000_000_000_000_000; // 100 NEAR
pub const NO_DEPOSIT: u128 = 0; // Attach no deposit.

/// 10T gas for basic operation
pub const GAS_FOR_BASIC_OP: Gas = Gas(10_000_000_000_000);

// @todo add gas as per the requirement of the mainnet before deployment

// const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(5_000_000_000_000);
// const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

/// Amount of gas for fungible token transfers, increased to 20T
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);

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
    is_cancelled: bool,
    paused_time: Timestamp, // last paused time
    contract_id: AccountId, // will be ignored for native stream
    can_update: bool,
    can_cancel: bool,
    is_native: bool,
}

#[ext_contract(ext_ft_transfer)]
trait FungibleTokenCore {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new() -> Self {
        require!(!env::state_exists(), "Already initialized");
        Self {
            current_id: 1,
            streams: UnorderedMap::new(b"p"),
        }
    }

    #[payable]
    pub fn create_stream(
        &mut self,
        receiver: AccountId,
        stream_rate: U128,
        start: U64,
        end: U64,
        can_cancel: bool,
        can_update: bool,
    ) -> U64 {
        // Check the receiver and sender are not same
        require!(receiver != env::predecessor_account_id(), "Sender and receiver cannot be Same");

        let params_key = self.current_id;

        let stream: Stream = self.validate_stream(
            U64::from(params_key),
            env::predecessor_account_id(),
            receiver,
            stream_rate,
            start,
            end,
            can_cancel,
            can_update,
            true,
            "near.testnet".parse().unwrap(),
        );

        // check the amount send to the stream
        require!(
            env::attached_deposit() == stream.balance,
            "The amount provided doesn't matches the stream"
        );

        // Save the stream
        self.streams.insert(&params_key, &stream);

        // Update the global stream count for next stream
        self.current_id += 1;

        log!("Saving streams {}", stream.id);

        U64::from(params_key)
    }

    #[payable]
    pub fn update(
        &mut self,
        stream_id: U64,
        start: Option<U64>,
        end: Option<U64>,
        rate: Option<U128>,
    ) {
        // convert to native u64
        let id: u64 = stream_id.0;
        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // check the stream can be udpated
        require!(env::predecessor_account_id() == stream.sender, "You are not authorized to update this stream");
        require!(stream.can_update, "Stream cannot be updated");
        require!(!stream.is_cancelled, "Stream has already been cancelled");

        // convert id to native u128
        let rate = u128::from(rate.unwrap_or(U128(stream.rate)));
        let start_time = u64::from(start.unwrap_or(U64(stream.start_time)));
        let end_time = u64::from(end.unwrap_or(U64(stream.end_time)));

        // Check the start and end timestamp is valid
        require!(
            stream.start_time > current_timestamp,
            "Cannot update: stream already started"
        );
        require!(
            start_time < end_time,
            "Start time should be less than end time"
        );

        if start_time != stream.start_time {
            require!(
                start_time >= current_timestamp,
                "Start time cannot be in the past"
            );
        }
        require!(rate > 0, "Rate cannot be zero");

        // check the rate is valid
        require!(rate < MAX_RATE, "Rate is too high");

        stream.start_time = start_time;
        stream.withdraw_time = start_time;
        stream.end_time = end_time;
        stream.rate = rate;

        // calculate the balance is enough
        let stream_duration = stream.end_time - stream.start_time;
        let stream_amount = u128::from(stream_duration) * rate;

        if stream_amount > stream.balance {
            // check the amount send to the stream
            require!(
                env::attached_deposit() >= stream_amount - stream.balance,
                "The amount provided is not enough for the stream"
            );

            stream.balance += env::attached_deposit();
        }

        self.streams.insert(&id, &stream);
    }

    #[private]
    pub fn internal_resolve_ft_withdraw(&mut self, stream_id: U64, temp_stream: Stream) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        if res {
            self.streams.insert(&stream_id.into(), &temp_stream);
        }
        return res;
    }

    #[private]
    pub fn internal_resolve_ft_claim(&mut self, stream_id: U64, temp_stream: &mut Stream) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        if res {
            temp_stream.balance = 0;
            self.streams.insert(&stream_id.into(), &temp_stream);
        }
        return res;
    }

    #[payable]
    pub fn withdraw(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(&id).unwrap();

        // Check 1 yocto token for ft_token call
        if !temp_stream.is_native {
            assert_one_yocto();
        }

        require!(temp_stream.balance > 0, "No balance to withdraw");
        require!(
            !temp_stream.is_cancelled,
            "Stream is cancelled by sender already!"
        );

        // assert the stream has started
        require!(
            current_timestamp > temp_stream.start_time,
            "The stream has not started yet"
        );

        require!(
            env::predecessor_account_id() == temp_stream.sender
                || env::predecessor_account_id() == temp_stream.receiver,
            "You dont have permissions to withdraw"
        );

        // Case: sender withdraws excess amount from the stream after it has ended
        if env::predecessor_account_id() == temp_stream.sender {
            require!(
                current_timestamp > temp_stream.end_time,
                "Cannot withdraw before the stream has ended"
            );

            // Amount that has been streamed to the receiver
            let withdrawal_amount: u128;

            if temp_stream.is_paused {
                withdrawal_amount = temp_stream.rate
                    * u128::from(temp_stream.paused_time - temp_stream.withdraw_time);
            } else {
                if temp_stream.end_time > temp_stream.withdraw_time {
                    // receiver has not withdrawn after stream ended
                    withdrawal_amount = temp_stream.rate
                        * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
                } else {
                    withdrawal_amount = 0;
                }
            }

            // Calculate the withdrawl amount
            let remaining_balance = temp_stream.balance - withdrawal_amount;
            require!(remaining_balance > 0, "Already withdrawn");

            // Update stream and save
            temp_stream.balance -= remaining_balance;
            // Transfer tokens to the sender
            let receiver = temp_stream.sender.clone();

            if temp_stream.is_native {
                self.streams.insert(&stream_id.into(), &temp_stream);
                Promise::new(receiver).transfer(remaining_balance).into()
            } else {
                // NEP141 : ft_transfer()
                ext_ft_transfer::ext(temp_stream.contract_id.clone())
                    .with_attached_deposit(1)
                    .ft_transfer(receiver, remaining_balance.into(), None)
                    .then(
                        Self::ext(env::current_account_id())
                            .internal_resolve_ft_withdraw(stream_id, temp_stream),
                    )
                    .into()
            }

        // Case: Receiver can withdraw the amount fromt the stream
        } else {
            let time_elapsed: u64;
            let withdraw_time: u64;

            // Calculate the elapsed time
            if current_timestamp >= temp_stream.end_time {
                require!(
                    temp_stream.withdraw_time < temp_stream.end_time,
                    "Already withdrawn"
                );
                withdraw_time = current_timestamp;

                if temp_stream.is_paused {
                    time_elapsed = temp_stream.paused_time - temp_stream.withdraw_time;
                } else {
                    time_elapsed = temp_stream.end_time - temp_stream.withdraw_time;
                }
            } else if temp_stream.is_paused {
                time_elapsed = temp_stream.paused_time - temp_stream.withdraw_time;
                withdraw_time = temp_stream.paused_time;
            } else {
                time_elapsed = current_timestamp - temp_stream.withdraw_time;
                withdraw_time = current_timestamp;
            }

            // Calculate the withdrawal amount
            let withdrawal_amount = temp_stream.rate * u128::from(time_elapsed);

            // Transfer the tokens to the receiver
            let receiver = temp_stream.receiver.clone();
            require!(withdrawal_amount > 0, "There is no balance to withdraw");

            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;

            if temp_stream.is_native {
                self.streams.insert(&stream_id.into(), &temp_stream);
                Promise::new(receiver).transfer(withdrawal_amount).into()
            } else {
                // NEP141 : ft_transfer()
                // require!(env::prepaid_gas() > GAS_FOR_FT_TRANSFER, "More gas is required");
                // log!("{:?}", temp_stream);
                ext_ft_transfer::ext(temp_stream.contract_id.clone())
                    // .with_static_gas(GAS_FOR_FT_TRANSFER)
                    .with_attached_deposit(1)
                    .ft_transfer(receiver, withdrawal_amount.into(), None)
                    .then(
                        // ext_self::ext(env::current_account_id())
                        // .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                        // .resolve_ft_withdraw(stream_id, temp_stream),
                        // ext_self::ft
                        Self::ext(env::current_account_id())
                            .internal_resolve_ft_withdraw(stream_id, temp_stream),
                    )
                    .into()
            }
        }
    }

    pub fn pause(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // Only the sender can pause the stream
        require!(env::predecessor_account_id() == stream.sender);

        // Can only be paused after the stream has started and before it has ended
        let can_pause =
            current_timestamp > stream.start_time && current_timestamp < stream.end_time;
        require!(
            can_pause,
            "Stream can only be pause after it starts and before it has ended"
        );
        require!(!stream.is_cancelled, "Cannot pause cancelled stream");


        // assert that the stream is already paused
        require!(!stream.is_paused, "Cannot pause already paused stream");

        // update the stream state
        stream.is_paused = true;
        stream.paused_time = current_timestamp;
        self.streams.insert(&id, &stream);

        // Log
        log!("Stream paused: {}", stream.id);
    }

    pub fn resume(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        // get the stream
        let mut stream = self.streams.get(&id).unwrap();

        // Only the sender can resume the stream
        require!(env::predecessor_account_id() == stream.sender);

        // assert that the stream is already paused
        require!(stream.is_paused, "Cannot resume unpaused stream");
        require!(!stream.is_cancelled, "Cannot resume cancelled stream");


        // resume the stream
        stream.is_paused = false;

        // Update the withdraw_time so that the receiver will not be
        // able to withdraw fund for paused time
        if current_timestamp > stream.end_time {
            stream.withdraw_time += stream.end_time - stream.paused_time;
        } else {
            stream.withdraw_time += current_timestamp - stream.paused_time;
        }

        // Reset the paused_time and save
        stream.paused_time = 0;
        self.streams.insert(&id, &stream);

        // Log
        log!("Stream resumed: {}", stream.id);
    }

    #[payable]
    pub fn cancel(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        //  only transfers the tokens to receiver
        //  sender can claim using ft_claim_sender

        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();

        // Check 1 yocto token for ft_token call
        if !temp_stream.is_native {
            assert_one_yocto();
        }

        // check that the stream can be cancelled
        require!(temp_stream.can_cancel, "Stream cannot be cancelled");

        // Only the sender can cancel the stream
        require!(env::predecessor_account_id() == temp_stream.sender);

        // Stream can only be cancelled if it has not ended
        require!(
            temp_stream.end_time > current_timestamp,
            "Stream already ended"
        );
        require!(!temp_stream.is_cancelled, "already cancelled!");

        // Amounts to refund to the sender and the receiver
        let sender_amt: u128;
        let receiver_amt: u128;

        // Calculate the amount to refund to the receiver
        if current_timestamp < temp_stream.start_time {
            receiver_amt = 0;
        } else if temp_stream.is_paused {
            receiver_amt =
                u128::from(temp_stream.paused_time - temp_stream.withdraw_time) * temp_stream.rate;
        } else {
            receiver_amt =
                u128::from(current_timestamp - temp_stream.withdraw_time) * temp_stream.rate;
        }

        // Calculate the amount to refund to the sender
        sender_amt = temp_stream.balance - receiver_amt;

        // Refund the amounts to the sender and the receiver respectively
        let sender = temp_stream.sender.clone();
        let receiver = temp_stream.receiver.clone();

        // Update the stream balance and save
        temp_stream.balance = sender_amt;
        temp_stream.is_cancelled = true;
        // self.streams.insert(&id, &temp_stream);

        // log
        log!("Stream cancelled: {}", temp_stream.id);

        if temp_stream.is_native {
            temp_stream.balance = 0;
            self.streams.insert(&id, &temp_stream);
            Promise::new(sender)
                .transfer(sender_amt)
                .then(Promise::new(receiver).transfer(receiver_amt))
                .into()
        } else {
            ext_ft_transfer::ext(temp_stream.contract_id.clone())
                .with_attached_deposit(1)
                .ft_transfer(receiver, receiver_amt.into(), None)
                .then(
                    Self::ext(env::current_account_id())
                        .internal_resolve_ft_withdraw(stream_id, temp_stream),
                )
                .into()
        }
    }

    // allows the sender to withdraw funds if the stream is_cancelled.
    pub fn ft_claim_sender(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        // convert id to native u64
        let id: u64 = stream_id.0;

        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();
        require!(
            temp_stream.sender == env::predecessor_account_id(),
            "not sender"
        );
        require!(temp_stream.is_cancelled, "stream is not cancelled!");
        ext_ft_transfer::ext(temp_stream.contract_id.clone())
            .with_attached_deposit(1)
            .ft_transfer(temp_stream.sender.clone(), temp_stream.balance.into(), None)
            .then(
                Self::ext(env::current_account_id())
                    .internal_resolve_ft_claim(stream_id, &mut temp_stream),
            )
            .into()
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
    #[should_panic(expected = "The amount provided doesn't matches the stream")]
    fn create_stream_invalid_amount() {
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 172800);
        let sender = accounts(0);
        let receiver = accounts(1);
        let rate = U128::from(1 * NEAR);

        let mut contract = Contract::new();

        set_context_with_balance(sender, 200000 * NEAR);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
    }

    #[test]
    #[should_panic(expected = "Sender and receiver cannot be Same")]
    fn create_stream_invalid_receipient() {
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 172800); // 2 days
        let sender = &accounts(0); // alice
        let receiver = &accounts(0); // alice
        let rate = U128::from(1 * NEAR);

        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 172800 * NEAR);

        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
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

        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
        assert_eq!(contract.current_id, 2);
        let params_key = 1;
        let stream = contract.streams.get(&params_key).unwrap();
        require!(!stream.is_paused);
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
        assert_eq!(stream.can_update, false);
        assert_eq!(stream.can_cancel, true);
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // 4. assert internal balance
        // Check the contract balance after stream is created
        set_context_with_balance_timestamp(env::current_account_id(), 10 * NEAR, start_time.0);
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        require!(internal_balance == 10 * NEAR);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
    #[should_panic(expected = "Cannot pause already paused stream")]
    fn test_sender_pauses_paused_stream() {
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // pause the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.pause(stream_id);
    }

    #[test]
    #[should_panic(expected = "Cannot resume unpaused stream")]
    fn test_sender_resume_unpaused_stream() {
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);
    }

    #[test]
    #[should_panic(expected = "Cannot pause cancelled stream")]
    fn test_sender_pauses_cancelled_stream() {
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, true);

        // pause the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.cancel(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.pause(stream_id);
    }

    #[test]
    #[should_panic(expected = "Cannot resume cancelled stream")]
    fn test_sender_resume_cancelled_stream() {
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, true);

        // pause the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 8);
        contract.pause(stream_id);

        // cancel the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.cancel(stream_id);

        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 13);
        contract.resume(stream_id);
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(sender.clone(), 0, start + 10);
        // 3. pause
        contract.pause(stream_id);

        // 4. assert
        require!(contract.streams.get(&stream_id.0).unwrap().is_paused);
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);
        contract.pause(stream_id);

        // 3. resume
        set_context_with_balance_timestamp(sender.clone(), 0, start + 4);
        contract.resume(stream_id);

        // 4. assert
        let stream = contract.streams.get(&stream_id.0).unwrap();
        require!(!stream.is_paused);
        assert_eq!(stream.withdraw_time, start + 3);
    }

    #[test]
    #[should_panic(expected = "Stream cannot be cancelled")]
    fn test_cancel_with_no_cancel() {
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
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);
        contract.cancel(stream_id);
    }

    #[test]
    fn test_cancel() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);
        contract.cancel(stream_id);

        // 3. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    fn test_cancel_before_start() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);
        contract.cancel(stream_id);

        // 3. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    #[should_panic(expected = "You are not authorized to update this stream")]
    fn test_update_unauthorized() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, true);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(receiver.clone(), 0, start + 11);

        contract.update(
            stream_id,
            Option::Some(U64::from(start + 12)),
            Option::Some(U64::from(start + 14)),
            Option::Some(U128::from(2 * NEAR)),
        );
    }


    #[test]
    #[should_panic(expected = "Cannot update: stream already started")]
    fn test_update_after_stream_start() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, true);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(sender.clone(), 0, start + 11);

        contract.update(
            stream_id,
            Option::Some(U64::from(start + 12)),
            Option::Some(U64::from(start + 14)),
            Option::Some(U128::from(2 * NEAR)),
        );
    }

    #[test]
    #[should_panic(expected = "The amount provided is not enough for the stream")]
    fn test_update_stream_insufficient_balance_1() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, true);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(sender.clone(), 0, start + 1);

        contract.update(
            stream_id,
            Option::Some(U64::from(start + 12)),
            Option::Some(U64::from(start + 14)),
            Option::Some(U128::from(70 * NEAR)), // Rate = 70 NEAR with balance of just 10 Near (should fail)
        );
    }

    #[test]
    fn test_update_stream() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new();

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, true);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, start + 1);

        contract.update(
            stream_id,
            Option::Some(U64::from(start + 12)),
            Option::Some(U64::from(start + 14)),
            Option::Some(U128::from(10 * NEAR)),
        );

        let params_key = 1;
        let stream = contract.streams.get(&params_key).unwrap();
        assert!(!stream.is_paused);
        assert_eq!(stream.id, 1);
        assert_eq!(stream.sender, sender.clone());
        assert_eq!(stream.receiver, accounts(1));
        assert_eq!(stream.balance, 20 * NEAR);
        assert_eq!(stream.rate, 10 * NEAR);
        assert_eq!(stream.start_time, start + 12);
        assert_eq!(stream.end_time, start + 14);
        assert_eq!(stream.withdraw_time, start + 12);
        assert_eq!(stream.paused_time, 0);
        assert_eq!(stream.can_update, true);
        assert_eq!(stream.can_cancel, false);
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
        builder.block_timestamp(ts * 1e9 as u64);
        testing_env!(builder.build());
    }
}
