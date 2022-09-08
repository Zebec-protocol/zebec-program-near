use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise, serde};
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
// #[serde(crate = "near_sdk::serde")]
impl Contract {
    pub fn get_stream(&self, stream_id: U64) -> Stream {
        let id: u64 = stream_id.into();
        self.streams.get(&id).unwrap()
    }

    // for testing only
    pub fn get_streams(&self) -> Vec<Stream> {
        let mut res: Vec<Stream> = [].to_vec();
        for i in 0..self.streams.len() {
            res.push(self.streams.get(&i).unwrap());
        }
        return res;
    }
}
