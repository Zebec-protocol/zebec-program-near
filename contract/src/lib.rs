use events::NStreamCreationLog;
use near_contract_standards::storage_management::StorageBalance;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, UnorderedSet};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::utils::assert_one_yocto;
use near_sdk::{
    env, ext_contract, log, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise,
    PromiseOrValue, PromiseResult, StorageUsage, Timestamp,
};

mod calls;
mod constants;
mod events;
mod storage_spec;
mod utils;
mod views;

use constants::MAX_RATE;
use constants::NATIVE_NEAR_CONTRACT_ID;

use crate::constants::{GAS_FOR_FT_TRANSFER, GAS_FOR_FT_TRANSFER_CALL, GAS_FOR_RESOLVE_TRANSFER};
use crate::events::{StreamUpdateLog, WithdrawNativeSenderLog, WithdrawTokenSenderLog, WithdrawNativeReceiverLog, WithdrawTokenReceiverLog, StreamPauseLog, CancelNativeLog, CancelTokenLog, ClaimNativeLog, ClaimTokenLog, StreamResumeLog};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    current_id: u64,
    streams: UnorderedMap<u64, Stream>,
    pub accounts: LookupMap<AccountId, StorageBalance>,
    account_storage_usage: StorageUsage,
    owner_id: AccountId,   // owner of the contract
    manager_id: AccountId, // delete stagnant streams
    whitelisted_tokens: UnorderedSet<AccountId>,
    fee_receiver: AccountId,
    fee_rate: u64,     // in BPS based on constants::FEE_BPS_DIVISOR(10_000)
    max_fee_rate: u64, // in BPS based on constants::FEE_BPS_DIVISOR(10_000)
    accumulated_fees: UnorderedMap<AccountId, u128>, // fee_amount for the receiver per token
    native_fees: u128,
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
    locked: bool, // A mutex to block any execution before callback completes
    paused_amount: Balance,
    total_amount: Balance,
    withdrawn_amount: Balance, // only for receiver
}

#[ext_contract(ext_ft_transfer)]
trait FungibleTokenCore {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        owner_id: AccountId,
        manager_id: AccountId,
        fee_receiver: AccountId,
        fee_rate: U64,
        max_fee_rate: U64,
    ) -> Self {
        require!(!env::state_exists(), "Already initialized");

        let mut this = Self {
            current_id: 1,
            streams: UnorderedMap::new(b"p"),
            accounts: LookupMap::new(b"m"),
            account_storage_usage: 0,
            owner_id,
            manager_id,
            whitelisted_tokens: UnorderedSet::new(b"s"),
            fee_receiver,
            fee_rate: fee_rate.0,
            max_fee_rate: max_fee_rate.0,
            accumulated_fees: UnorderedMap::new(b"a"), // only for tokens
            native_fees: 0,
        };
        this.measure_account_storage_usage();
        this
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
        // predecessor_account_id() registered
        require!(
            self.accounts.get(&env::predecessor_account_id()).is_some(),
            "Not registered!"
        );

        let initial_storage_usage = env::storage_usage();

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
            NATIVE_NEAR_CONTRACT_ID.parse().unwrap(),
        );

        // check the amount send to the stream
        require!(
            env::attached_deposit() == stream.balance,
            "The amount provided doesn't matches the stream"
        );

        // Save the stream
        self.streams.insert(&params_key, &stream);

        // Verify that the user has enough balance to cover for storage used
        let mut storage_balance = self.accounts.get(&env::predecessor_account_id()).unwrap();
        let final_storage_usage = env::storage_usage();
        let required_storage_balance =
            (final_storage_usage - initial_storage_usage) as Balance * env::storage_byte_cost();
        
        require!(
            storage_balance.available >= required_storage_balance.into(),
            "Deposit more storage balance!"
        );

        // Update the global stream count for next stream
        self.current_id += 1;

        // Update the account as per the storage balance used
        storage_balance.available = (storage_balance.available.0 - required_storage_balance).into();

        self.accounts
            .insert(&env::predecessor_account_id(), &storage_balance);

        let nslog: NStreamCreationLog = NStreamCreationLog {
            stream_id: stream.id,
            sender: env::predecessor_account_id(),
            receiver: stream.receiver,
            rate: stream.rate,
            created: stream.created,
            start_time: stream.start_time,
            end_time: stream.end_time,
            can_cancel: stream.can_cancel,
            can_update: stream.can_update,
            balance: stream.balance,
            is_native: stream.is_native,
        };
        env::log_str(&nslog.to_string());

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
        
        require!(stream.is_native, "not native stream!");

        require!(
            !stream.locked,
            "Some other operation is happening in the stream"
        );

        require!(
            env::predecessor_account_id() == stream.sender,
            "You are not authorized to update this stream"
        );
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
        } else {
            assert_one_yocto();
        }
        // logging functionalities
        let update_log: StreamUpdateLog = StreamUpdateLog{
            stream_id: stream.id,
            start: Some(stream.start_time),
            end: Some(stream.end_time),
            rate: Some(stream.rate),
            balance: Some(stream.balance)
        };
        env::log_str(&update_log.to_string());

        self.streams.insert(&id, &stream);
    }

    #[private]
    pub fn internal_resolve_withdraw_stream(
        &mut self,
        stream_id: U64,

        // Values to revert back in case of failure
        withdrawal_amount: U128,
        withdraw_time: U64,
        fee_amount: U128,
    ) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        let mut temp_stream = self.streams.get(&stream_id.into()).unwrap();
        temp_stream.locked = false;
        if !res {
            // In case of failure revert the changed states

            // Revert the balance of the stream
            temp_stream.balance += withdrawal_amount.0;

            // Revert the withdraw time
            if withdraw_time.0 < temp_stream.withdraw_time {
                temp_stream.withdraw_time = withdraw_time.0;
            }

            // Revert the accumulated total fee calculation
            if temp_stream.is_native {
                self.native_fees -= fee_amount.0;
            } else {
                let total_fee = self
                    .accumulated_fees
                    .get(&temp_stream.contract_id)
                    .unwrap_or(0)
                    - fee_amount.0;
                self.accumulated_fees
                    .insert(&temp_stream.contract_id, &total_fee);
            }
        }
        self.streams.insert(&stream_id.into(), &temp_stream);
        res
    }

    #[payable]
    pub fn withdraw(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        // Check 1 yocto token
        assert_one_yocto();

        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;

        // get the stream with id: stream_id
        let mut temp_stream = self.streams.get(&id).unwrap();
        require!(
            !temp_stream.locked,
            "Some other operation is happening in the stream"
        );

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
                if temp_stream.end_time > temp_stream.withdraw_time {
                    withdrawal_amount = temp_stream.rate
                    * u128::from(temp_stream.paused_time - temp_stream.withdraw_time);
                } else {
                    withdrawal_amount = 0;
                }
            } else {
                if temp_stream.end_time > temp_stream.withdraw_time {
                    // receiver has not withdrawn after stream ended
                    withdrawal_amount = temp_stream.rate
                        * u128::from(temp_stream.end_time - temp_stream.withdraw_time);
                } else {
                    withdrawal_amount = 0;
                }
            }

            // Calculate the withdrawal amount
            let remaining_balance = temp_stream.balance - withdrawal_amount;
            require!(remaining_balance > 0, "Already withdrawn");

            // Update stream and save
            temp_stream.balance -= remaining_balance;
            temp_stream.locked = true;

            // Transfer tokens to the sender
            let sender = temp_stream.sender.clone();

            // Values to revert in case of failure to transfer the tokens
            let withdrawal_amount_revert = U128::from(remaining_balance);
            let withdrawal_time_revert = U64::from(temp_stream.withdraw_time); // withdrawal_time is not changed but the callback function requires it

            if temp_stream.is_native {
                self.streams.insert(&stream_id.into(), &temp_stream);
                
                let withdraw_log: WithdrawNativeSenderLog = WithdrawNativeSenderLog{
                    stream_id: temp_stream.id,
                    withdraw_amount: remaining_balance,
                    withdraw_time: current_timestamp,
                    sender: sender.clone(),
                };
                env::log_str(&withdraw_log.to_string());

                // result is not in the current block, confirmation is in next block
                Promise::new(sender)
                    .transfer(remaining_balance)
                    .then(
                        Self::ext(env::current_account_id()).internal_resolve_withdraw_stream(
                            stream_id,
                            withdrawal_amount_revert,
                            withdrawal_time_revert,
                            U128::from(0),
                        ),
                    )
                    .into()
            } else {
                self.streams.insert(&stream_id.into(), &temp_stream);

                let withdraw_log: WithdrawTokenSenderLog = WithdrawTokenSenderLog{
                    stream_id: temp_stream.id,
                    withdraw_amount: remaining_balance,
                    withdraw_time: current_timestamp,
                    sender: sender.clone(),
                };
                env::log_str(&withdraw_log.to_string());

                // NEP141 : ft_transfer()
                // 50TGas - 20(for FT transfer) - 20 (for resolve), only 5 for internal operations
                require!(
                    (env::prepaid_gas() - env::used_gas()) > GAS_FOR_FT_TRANSFER_CALL,
                    "More gas is required"
                );
                ext_ft_transfer::ext(temp_stream.contract_id.clone())
                    .with_static_gas(GAS_FOR_FT_TRANSFER)
                    .with_attached_deposit(1)
                    .ft_transfer(sender, remaining_balance.into(), None)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                            .internal_resolve_withdraw_stream(
                                stream_id,
                                withdrawal_amount_revert,
                                withdrawal_time_revert,
                                U128::from(0),
                            ),
                    )
                    .into()
            }

        // case: when receiver withdraws from the stream
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
            let mut withdrawal_amount = temp_stream.rate * u128::from(time_elapsed);

            // Transfer the tokens to the receiver
            let receiver = temp_stream.receiver.clone();
            require!(withdrawal_amount > 0, "There is no balance to withdraw");

            // Values to revert incase the transfer fails
            let withdrawal_amount_revert = U128::from(withdrawal_amount);
            let withdrawal_time_revert = U64::from(withdraw_time);

            // Update the stream struct and save
            temp_stream.balance -= withdrawal_amount;
            temp_stream.withdraw_time = withdraw_time;
            temp_stream.withdrawn_amount += withdrawal_amount;
            temp_stream.locked = true;

            // Update the stream
            self.streams.insert(&stream_id.into(), &temp_stream);

            // Calculate fee amount
            let fee_amount = self.calculate_fee_amount(withdrawal_amount);

            // fee caclulation
            if fee_amount > 0 {
                if temp_stream.is_native {
                    self.native_fees += fee_amount;
                } else {
                    let total_fee = self
                        .accumulated_fees
                        .get(&temp_stream.contract_id)
                        .unwrap_or(0)
                        + fee_amount;
                    self.accumulated_fees
                        .insert(&temp_stream.contract_id, &total_fee);
                }
                withdrawal_amount = withdrawal_amount - fee_amount;
            }

            if temp_stream.is_native {
                let withdraw_log: WithdrawNativeReceiverLog = WithdrawNativeReceiverLog{
                    stream_id: temp_stream.id,
                    withdraw_amount: withdrawal_amount,
                    withdraw_time: current_timestamp,
                    sender: temp_stream.receiver,
                };
                env::log_str(&withdraw_log.to_string());

                Promise::new(receiver)
                    .transfer(withdrawal_amount)
                    .then(
                        Self::ext(env::current_account_id()).internal_resolve_withdraw_stream(
                            stream_id,
                            withdrawal_amount_revert,
                            withdrawal_time_revert,
                            U128::from(fee_amount),
                        ),
                    )
                    .into()
            } else {
                let withdraw_log: WithdrawTokenReceiverLog = WithdrawTokenReceiverLog{
                    stream_id: temp_stream.id,
                    contract_id: temp_stream.contract_id.clone(),
                    withdraw_amount: withdrawal_amount,
                    withdraw_time: current_timestamp,
                    sender: temp_stream.receiver,
                };
                env::log_str(&withdraw_log.to_string());

                // NEP141 : ft_transfer()
                require!(
                    (env::prepaid_gas() - env::used_gas()) > GAS_FOR_FT_TRANSFER_CALL,
                    "More gas is required"
                );
                ext_ft_transfer::ext(temp_stream.contract_id.clone())
                    .with_static_gas(GAS_FOR_FT_TRANSFER)
                    .with_attached_deposit(1)
                    .ft_transfer(receiver, withdrawal_amount.into(), None)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                            .internal_resolve_withdraw_stream(
                                stream_id,
                                withdrawal_amount_revert,
                                withdrawal_time_revert,
                                U128::from(fee_amount),
                            ),
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
        require!(
            !stream.locked,
            "Some other operation is happening in the stream"
        );

        // Only the sender can pause the stream
        require!(
            env::predecessor_account_id() == stream.sender,
            "Stream can only be paused by the sender"
        );

        require!(!stream.is_cancelled, "Cannot pause cancelled stream");

        // assert that the stream is not already paused
        require!(!stream.is_paused, "Cannot pause already paused stream");

        // Can only be paused after the stream has started and before it has ended
        let can_pause =
            current_timestamp > stream.start_time && current_timestamp < stream.end_time;
        require!(
            can_pause,
            "Stream can only be pause after it starts and before it has ended"
        );

        // update the stream state
        stream.is_paused = true;
        stream.paused_time = current_timestamp;
        self.streams.insert(&id, &stream);


        let pause_log: StreamPauseLog = StreamPauseLog{
            stream_id: stream.id,
            time: current_timestamp,
        };
        env::log_str(&pause_log.to_string());
    }

    pub fn resume(&mut self, stream_id: U64) {
        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        // get the stream
        let mut stream = self.streams.get(&id).unwrap();
        require!(
            !stream.locked,
            "Some other operation is happening in the stream"
        );

        // Only the sender can resume the stream
        require!(
            env::predecessor_account_id() == stream.sender,
            "Stream can only be resumed by the sender"
        );

        // assert that the stream is already paused
        require!(stream.is_paused, "Cannot resume unpaused stream");
        require!(!stream.is_cancelled, "Cannot resume cancelled stream");

        // resume the stream
        stream.is_paused = false;

        // Update the withdraw_time so that the receiver will not be
        // able to withdraw fund for paused time
        if current_timestamp > stream.end_time {
            stream.withdraw_time += stream.end_time - stream.paused_time;
            stream.paused_amount += u128::from(stream.end_time - stream.paused_time ) * stream.rate;
        } else {
            stream.withdraw_time += current_timestamp - stream.paused_time;
            stream.paused_amount += u128::from(current_timestamp - stream.paused_time ) * stream.rate;
        }

        // Reset the paused_time and save
        stream.paused_time = 0;
        self.streams.insert(&id, &stream);

        // Log
        let resume_log: StreamResumeLog = StreamResumeLog{
            stream_id: stream.id,
            time: current_timestamp,
        };
        env::log_str(&resume_log.to_string());
    }

    #[payable]
    pub fn cancel(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        //  only transfers the tokens to receiver
        //  sender can claim using ft_claim_sender

        // Check 1 yocto token
        assert_one_yocto();

        // convert id to native u64
        let id: u64 = stream_id.0;

        let current_timestamp: u64 = env::block_timestamp_ms() / 1000;
        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();
        require!(
            !temp_stream.locked,
            "Some other operation is happening in the stream"
        );

        // check that the stream can be cancelled
        require!(temp_stream.can_cancel, "Stream cannot be cancelled");

        // Only the sender can cancel the stream
        require!(
            env::predecessor_account_id() == temp_stream.sender,
            "Stream can only be cancelled by the stream sender"
        );

        // Stream can only be cancelled if it has not ended
        require!(
            temp_stream.end_time > current_timestamp,
            "Stream already ended"
        );
        require!(!temp_stream.is_cancelled, "already cancelled!");

        // Amounts to refund to the sender and the receiver
        let mut receiver_amt: u128;


        temp_stream.withdraw_time = current_timestamp;

        // Calculate the amount to refund to the receiver
        if current_timestamp < temp_stream.start_time {
            receiver_amt = 0;
        } else if temp_stream.is_paused {
            receiver_amt =
                u128::from(temp_stream.paused_time - temp_stream.withdraw_time) * temp_stream.rate;
            temp_stream.withdraw_time = temp_stream.paused_time;
        } else {
            receiver_amt =
                u128::from(current_timestamp - temp_stream.withdraw_time) * temp_stream.rate;
        }

        // Values to revert in case the transfer fails
        let revert_balance = U128::from(receiver_amt);

        let receiver = temp_stream.receiver.clone();

        // Update the stream balance and save
        temp_stream.balance -= receiver_amt;
        temp_stream.withdrawn_amount += receiver_amt;
        temp_stream.is_cancelled = true;

        // Lock only if transfer will occur
        if receiver_amt > 0 {
            temp_stream.locked = true;
        }

        // Update the stream
        self.streams.insert(&id, &temp_stream);

        if receiver_amt == 0 {
            return PromiseOrValue::Value(true);
        }

        // fee caclulation
        let fee_amount = self.calculate_fee_amount(receiver_amt);

        if fee_amount > 0 {
            if temp_stream.is_native {
                self.native_fees += fee_amount
            } else {
                let total_fee = self
                    .accumulated_fees
                    .get(&temp_stream.contract_id)
                    .unwrap_or(0)
                    + fee_amount;
                self.accumulated_fees
                    .insert(&temp_stream.contract_id, &total_fee);
            }
            receiver_amt = receiver_amt - fee_amount;
        }

        // log
        log!("Stream cancelled: {}", temp_stream.id);

        if temp_stream.is_native {

            let cancel_log: CancelNativeLog = CancelNativeLog{
                stream_id: temp_stream.id,
                time: current_timestamp,
            };
            env::log_str(&cancel_log.to_string());
            Promise::new(receiver)
                    .transfer(receiver_amt)
                    .then(
                        Self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                            .internal_resolve_cancel_stream(
                                stream_id,
                                revert_balance,
                                U128::from(fee_amount),
                            ),
                    )
                    .into()
        } else {
            require!(
                (env::prepaid_gas() - env::used_gas()) > GAS_FOR_FT_TRANSFER_CALL,
                "More gas is required"
            );

            let cancel_log: CancelTokenLog = CancelTokenLog{
                stream_id: temp_stream.id,
                time: current_timestamp,
                contract_id: temp_stream.contract_id.clone(),
            };
            env::log_str(&cancel_log.to_string());
            
            ext_ft_transfer::ext(temp_stream.contract_id.clone())
                .with_static_gas(GAS_FOR_FT_TRANSFER)
                .with_attached_deposit(1)
                .ft_transfer(receiver, receiver_amt.into(), None)
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                        .internal_resolve_cancel_stream(
                            stream_id,
                            revert_balance,
                            U128::from(fee_amount),
                        ),
                )
                .into()
        }
    }

    #[private]
    pub fn internal_resolve_cancel_stream(
        &mut self,
        stream_id: U64,
        withdrawal_amount: U128,
        fee_amount: U128,
    ) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        let mut temp_stream = self.streams.get(&stream_id.into()).unwrap();
        temp_stream.locked = false;
        if !res {
            // In case of failure revert the withdrawal_amount and the is_cancelled state
            temp_stream.balance += withdrawal_amount.0;
            temp_stream.is_cancelled = false;
            if temp_stream.is_native {
                self.native_fees -= fee_amount.0;
            } else {
                let total_fee = self
                    .accumulated_fees
                    .get(&temp_stream.contract_id)
                    .unwrap_or(0)
                    - fee_amount.0;
                self.accumulated_fees
                    .insert(&temp_stream.contract_id, &total_fee);
            }
        }
        self.streams.insert(&stream_id.into(), &temp_stream);
        res
    }

    #[private]
    pub fn internal_resolve_claim_stream(
        &mut self,
        stream_id: U64,
        withdrawal_amount: U128,
    ) -> bool {
        let res: bool = match env::promise_result(0) {
            PromiseResult::Successful(_) => true,
            _ => false,
        };
        let mut temp_stream = self.streams.get(&stream_id.into()).unwrap();
        temp_stream.locked = false;
        if !res {
            // In case of failure revert the withdrawal_amount
            temp_stream.balance += withdrawal_amount.0;
        }
        self.streams.insert(&stream_id.into(), &temp_stream);
        res
    }

    // allows the sender to withdraw funds if the stream is_cancelled.
    #[payable]
    pub fn claim(&mut self, stream_id: U64) -> PromiseOrValue<bool> {
        // Check 1 yocto token
        assert_one_yocto();

        // convert id to native u64
        let id: u64 = stream_id.0;

        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();
        require!(
            !temp_stream.locked,
            "Some other operation is happening in the stream"
        );

        // Only the sender can claim
        require!(
            temp_stream.sender == env::predecessor_account_id(),
            "not sender"
        );
        require!(temp_stream.is_cancelled, "stream is not cancelled!");
        let balance = temp_stream.balance;
        require!(balance > 0, "amount <= 0");

        // update stream state
        temp_stream.balance = 0;
        temp_stream.locked = true;
        self.streams.insert(&stream_id.into(), &temp_stream);

        let sender = temp_stream.sender.clone();
        let revert_balance = U128::from(balance);

        if temp_stream.is_native {
            let claim_log: ClaimNativeLog = ClaimNativeLog{
                stream_id: temp_stream.id,
                time: env::block_timestamp(),
                balance: balance,
            };
            env::log_str(&claim_log.to_string());
            
            Promise::new(sender)
                .transfer(balance.into())
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                        .internal_resolve_claim_stream(stream_id, revert_balance),
                )
                .into()
        } else {
            require!(
                env::prepaid_gas() - env::used_gas() > GAS_FOR_FT_TRANSFER_CALL,
                "More gas is required"
            );

            let claim_log: ClaimTokenLog = ClaimTokenLog{
                stream_id: temp_stream.id,
                contract_id: temp_stream.contract_id.clone(),
                time: env::block_timestamp(),
                balance: balance,
            };
            
            env::log_str(&claim_log.to_string());
            ext_ft_transfer::ext(temp_stream.contract_id.clone())
                .with_static_gas(GAS_FOR_FT_TRANSFER)
                .with_attached_deposit(1)
                .ft_transfer(sender, balance.into(), None)
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                        .internal_resolve_claim_stream(stream_id, revert_balance),
                )
                .into()
        }
    }

    // method only to facilitate unit tests
    // streams cannot be unlocked in unit tests because callbacks don't work
    #[cfg(test)]
    pub fn unlock(&mut self, stream_id: U64) -> bool {
        // convert id to native u64
        let id: u64 = stream_id.0;

        // Get the stream
        let mut temp_stream = self.streams.get(&id).unwrap();
        temp_stream.locked = false;
        self.streams.insert(&stream_id.into(), &temp_stream);

        return true;
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
        let contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
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

        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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

        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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

        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 2);

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, start_time.0);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // 3. call withdraw (action)
        let stream_start_time: u64 = start_time.0;
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 2);
        contract.withdraw(stream_id);
    }

    #[test]
    fn withdraw_stream_sender_after_end() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 10);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 11);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;
        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 10 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 4);
        contract.pause(stream_id);

        // 3. call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 11);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 21);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 21);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 21);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 16 * NEAR);

        // 3. receiver call withdraw
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 25);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 21);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 4 * NEAR);

        // 3. receiver call withdraw
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 25);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 21);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 4 * NEAR);

        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 21);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);
        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, true);

        // pause the stream
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 9);
        contract.cancel(stream_id);
        contract.unlock(stream_id);

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);
        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, true);

        // pause the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 8);
        contract.pause(stream_id);

        // cancel the stream
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 9);
        contract.cancel(stream_id);
        contract.unlock(stream_id);

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 21);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 16 * NEAR);

        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 21);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // pause and resume the stream
        set_context_with_balance_timestamp(sender.clone(), 0, stream_start_time + 9);
        contract.pause(stream_id);

        // 3. sender call withdraw after stream has ended (action)
        set_context_with_balance_timestamp(sender.clone(), 1, stream_start_time + 21);
        contract.withdraw(stream_id);
        contract.unlock(stream_id);

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 9 * NEAR);

        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 25);
        contract.withdraw(stream_id); // panics here

        // 4. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 0);
    }

    #[test]
    fn test_withdraw_with_fee() {
        // 1. create_stream contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        let stream_id = U64::from(1);

        let stream_start_time: u64 = start_time.0;

        // 2. create stream
        set_context_with_balance_timestamp(sender.clone(), 20 * NEAR, stream_start_time);
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);

        // pause and resume the stream
        set_context_with_balance_timestamp(receiver.clone(), 1, stream_start_time + 9);
        contract.withdraw(stream_id);

        let fee_amount = contract.calculate_fee_amount(9 * NEAR);

        assert_eq!(contract.native_fees, fee_amount);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        set_context_with_balance(sender.clone(), 10000 * NEAR);

        // 2. create stream and pause
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 1, start + 1);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 1, start + 1);
        contract.cancel(stream_id);

        // 3. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 9 * NEAR);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, true, false);
        let stream_id = U64::from(1);
        set_context_with_balance_timestamp(sender.clone(), 1, start + 1);
        contract.cancel(stream_id);

        // 3. assert internal balance
        let internal_balance = contract.streams.get(&stream_id.0).unwrap().balance;
        assert_eq!(internal_balance, 10 * NEAR);
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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

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


    #[test]
    fn test_updates_withdrawn_balance() {
        // 1. Create the contract
        let start = env::block_timestamp();
        let start_time: U64 = U64::from(start + 10);
        let end_time: U64 = U64::from(start + 20);
        let sender = &accounts(0); // alice
        let receiver = &accounts(1); // bob
        let rate = U128::from(1 * NEAR);
        let mut contract = Contract::new(
            accounts(2),
            accounts(3),
            accounts(4),
            U64::from(25),
            U64::from(200),
        ); // "charlie", "danny", "eugene"
        register_user(&mut contract, sender.clone());

        set_context_with_balance(sender.clone(), 10 * NEAR);

        // 2. create stream and cancel
        contract.create_stream(receiver.clone(), rate, start_time, end_time, false, true);
        let stream_id = U64::from(1);

        set_context_with_balance_timestamp(receiver.clone(), 1, start + 15);


        contract.withdraw(stream_id);

        let params_key = 1;
        let stream = contract.streams.get(&params_key).unwrap();
        
        assert!(!stream.is_paused);
        assert_eq!(stream.id, 1);
        assert_eq!(stream.sender, sender.clone());
        assert_eq!(stream.receiver, accounts(1));
        assert_eq!(stream.balance, 5 * NEAR);

        assert_eq!(stream.withdrawn_amount, 5 * NEAR);
        assert_eq!(stream.rate, 10 * NEAR);
        assert_eq!(stream.end_time, start + 14);
        assert_eq!(stream.withdraw_time, start + 15);
        assert_eq!(stream.paused_time, 0);
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

    fn register_user(contract: &mut Contract, user_id: AccountId) {
        set_context_with_balance(user_id.clone(), 1 * NEAR);
        contract.storage_deposit(Some(user_id), Some(false));
    }
}
