use near_sdk::{near_bindgen, AccountId, Balance};
use crate::*;

// mainly for `ft_on_transfer`
#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StreamView {
    pub id: u64,
    pub receiver: AccountId,
    pub balance: Balance,
    pub rate: U128,
    pub created: Timestamp,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
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

    pub fn get_streams_by_user(&self, user_id: AccountId, from_index: Option<U128>, limit: Option<U64>) -> Vec<Stream> {
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
