use crate::*;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, Balance};

use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

impl Contract {
    pub(crate) fn internal_storage_balance_of(
        &self,
        account_id: &AccountId,
    ) -> Option<StorageBalance> {
        if self.accounts.get(account_id).is_some() {
            Some(self.accounts.get(&account_id).unwrap())
        } else {
            None
        }
    }

    pub(crate) fn internal_register_account(&mut self, account_id: &AccountId, amount: Balance) {
        let deposit_balance =
            amount - self.account_storage_usage as Balance * env::storage_byte_cost();
        let storage_balance = StorageBalance {
            total: amount.into(),
            available: deposit_balance.into(),
        };
        if self.accounts.insert(account_id, &storage_balance).is_some() {
            env::panic_str("The account is already registered");
        }
    }

    pub(crate) fn measure_account_storage_usage(&mut self) {
        let initial_storage_usage = env::storage_usage();
        let tmp_account_id = AccountId::new_unchecked("a".repeat(64));
        self.accounts.insert(
            &tmp_account_id,
            &StorageBalance {
                total: 0.into(),
                available: 0.into(),
            },
        );
        self.account_storage_usage = env::storage_usage() - initial_storage_usage;
        self.accounts.remove(&tmp_account_id);
    }
}

#[near_bindgen]
impl StorageManagement for Contract {
    // Deposit the balance for storage used by the user
    // If the user account doesn't exists then a new account will be created for the user
    // The minimum balance that needs to be deposited by the user is storage_balance_bounds.min
    //
    // If the user account already exists then the user may deposit as much balance as they wish
    //
    //
    // **`registration_only` doesn't affect the implementation
    // **Only the stream sender needs to be registered,
    #[allow(unused_variables)]
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount: Balance = env::attached_deposit();

        require!(amount > 0, "No deposit amount provided");

        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        if let Some(balance) = self.accounts.get(&account_id) {
            let _total = u128::from(balance.total) + amount;
            let _available: u128 = u128::from(balance.available) + amount;

            let storage_balance = StorageBalance {
                total: _total.into(),
                available: _available.into(),
            };
            self.accounts.insert(&account_id, &storage_balance);
        } else {
            let min_balance = self.storage_balance_bounds().min.0;
            println!("{}", min_balance);
            if amount < min_balance {
                env::panic_str("The attached deposit is less than the minimum storage balance");
            }
            self.internal_register_account(&account_id, amount);
        }
        self.internal_storage_balance_of(&account_id).unwrap()
    }

    /// storage_withdraw allows the caller to retrieve balance from `available` balance
    #[payable]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let predecessor_account_id = env::predecessor_account_id();

        if let Some(mut storage_balance) = self.internal_storage_balance_of(&predecessor_account_id)
        {
            let refund_amount: Balance;
            match amount {
                Some(withdraw_amt) if withdraw_amt.0 > storage_balance.available.0 => {
                    env::panic_str("The amount is greater than the available storage balance");
                }
                Some(withdraw_amt) if withdraw_amt.0 <= storage_balance.available.0 => {
                    refund_amount = withdraw_amt.0;
                }
                Some(_) => {
                    env::panic_str("The amount is greater than the available storage balance");
                }
                None => {
                    refund_amount = storage_balance.available.0;
                }
            };

            storage_balance.available = (storage_balance.available.0 - refund_amount).into();
            storage_balance.total = (storage_balance.total.0 - refund_amount).into();

            self.accounts
                .insert(&predecessor_account_id, &storage_balance);
            Promise::new(predecessor_account_id).transfer(refund_amount);

            storage_balance
        } else {
            env::panic_str(
                format!("The account {} is not registered", &predecessor_account_id).as_str(),
            );
        }
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();

        if let Some(f) = force {
            if f {
                panic!("We don't support force unregister");
            }
        }

        let account_id = env::predecessor_account_id();

        if self.accounts.get(&account_id).is_none() {
            return false;
        }
        let available_amount = self.accounts.get(&account_id).unwrap().available.0;

        self.accounts.remove(&account_id);

        if available_amount > 0 {
            Promise::new(account_id.clone()).transfer(available_amount);
        }
        true
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        // ~370 is required storage_usage for ft stream creation
        let storage_cost_for_stream = 370 * env::storage_byte_cost();
        let storage_cost_for_account =
            (self.account_storage_usage) as Balance * env::storage_byte_cost();
        let total_cost = storage_cost_for_account + storage_cost_for_stream;
        // max returns total stream creation of 20 stream
        StorageBalanceBounds {
            min: total_cost.into(),
            max: Some((storage_cost_for_stream * 20 + storage_cost_for_account).into()),
        }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id)
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

    #[test]
    fn test_storage_deposit() {
        let caller = accounts(0); // alice
        let deposit_amount = NEAR / 100;
        set_context_with_balance(caller.clone(), deposit_amount);
        let mut contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"

        let res = contract.storage_deposit(Some(caller.clone()), Some(false));
        assert!(res.total == U128(deposit_amount));
        assert!(
            res.available
                == U128(
                    deposit_amount
                        - contract.account_storage_usage as Balance * env::storage_byte_cost()
                )
        );
    }

    #[test]
    // #[should_panic(expected = "Stream cannot be cancelled")]
    fn test_storage_withdraw() {
        let caller = accounts(0); // alice
        let deposit_amount = NEAR / 100;
        set_context_with_balance(caller.clone(), deposit_amount);
        let mut contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"
        contract.storage_deposit(Some(caller.clone()), Some(false));

        set_context_with_balance(caller.clone(), 1);
        let res = contract.storage_withdraw(None);
        let ret = contract.storage_balance_of(caller).unwrap();
        assert!(res.available.0 == ret.available.0);
    }

    #[test]
    #[should_panic(expected = "The amount is greater than the available storage balance")]
    fn test_storage_deposit_fail() {
        let caller = accounts(0); // alice
        let deposit_amount = NEAR / 100;
        set_context_with_balance(caller.clone(), deposit_amount);
        let mut contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"
        contract.storage_deposit(Some(caller.clone()), Some(false));

        set_context_with_balance(caller, 1);
        contract.storage_withdraw(Some(U128(NEAR)));
    }

    #[test]
    fn test_storage_unregister() {
        let caller = accounts(0);
        let deposit_amount = NEAR / 100;
        set_context_with_balance(caller.clone(), deposit_amount);
        let mut contract = Contract::new(accounts(2), accounts(3), accounts(4), U64::from(25), U64::from(200)); // "charlie", "danny", "eugene"
        contract.storage_deposit(Some(caller.clone()), Some(false));
        set_context_with_balance(caller, 1);
        let res = contract.storage_unregister(Some(false));
        assert!(res);
    }
}
