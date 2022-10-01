use crate::*;
use near_sdk::{assert_one_yocto, env, log, near_bindgen, AccountId, Balance};

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
        self.accounts.insert(&tmp_account_id, &StorageBalance{total:0.into(), available:0.into()});
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

            let storage_balance = StorageBalance{total:_total.into() , available: _available.into()};
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

        if let Some(mut storage_balance) = self.internal_storage_balance_of(&predecessor_account_id) {
            let refund_amount: Balance;
            match amount {
                Some(withdraw_amt) if withdraw_amt.0 > storage_balance.available.0 => {
                    env::panic_str("The amount is greater than the available storage balance");
                },
                Some(withdraw_amt) if withdraw_amt.0 <= storage_balance.available.0 => {
                    refund_amount = withdraw_amt.0;
                },
                Some(_) => {
                    env::panic_str("The amount is greater than the available storage balance");
                },
                None => {
                    refund_amount = storage_balance.available.0;
                }
            };

            storage_balance.available = (storage_balance.available.0 - refund_amount).into();
            storage_balance.total = (storage_balance.total.0 - refund_amount).into();

            self.accounts.insert(&predecessor_account_id, &storage_balance);
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
        self.accounts.remove(&account_id);
        true
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let required_storage_balance = (self.account_storage_usage)
            as Balance
            * env::storage_byte_cost();
        StorageBalanceBounds {
            min: required_storage_balance.into(),
            max: Some(required_storage_balance.into()),
        }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id)
    }
}
