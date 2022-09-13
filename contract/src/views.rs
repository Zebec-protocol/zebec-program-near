use crate::*;
use near_sdk::{near_bindgen, AccountId, Balance};

// mainly for `ft_on_transfer`
#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StreamView {
    pub method_name: String,
    pub receiver: AccountId,
    pub stream_rate: U128,
    pub start: U64,
    pub end: U64,
}

#[near_bindgen]
impl Contract {
    pub fn get_stream(&self, stream_id: U64) -> Stream {
        let id: u64 = stream_id.into();
        self.streams.get(&id).unwrap()
    }

    pub fn get_streams(&self, from_index: Option<U128>, limit: Option<U64>) -> Vec<Stream> {
        let start = u128::from(from_index.unwrap_or(U128(0)));

        self.streams
            .keys()
            // skip to start
            .skip(start as usize)
            // take the first `limit` elements in the vec
            .take(limit.unwrap_or(U64(50)).0 as usize)
            .map(|id| self.streams.get(&id).unwrap())
            .collect()
    }

    pub fn get_streams_by_user(
        &self,
        user_id: AccountId,
        from_index: Option<U128>,
        limit: Option<U64>,
    ) -> Vec<Stream> {
        let start = u128::from(from_index.unwrap_or(U128(0)));

        self.streams
            .keys()
            // skip to start
            .skip(start as usize)
            // take the first `limit` elements in the vec
            .take(limit.unwrap_or(U64(50)).0 as usize)
            .map(|id| self.streams.get(&id).unwrap())
            .filter(|stream| stream.sender == user_id)
            .collect()
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
    fn set_context_with_balance(predecessor: AccountId, amount: Balance) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.attached_deposit(amount);
        testing_env!(builder.build());
    }

    #[test]
    fn test_get_stream() {
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
        let res_stream = contract.get_stream(near_sdk::json_types::U64(stream.id));
        println!("{}", res_stream.id);
    }
}
