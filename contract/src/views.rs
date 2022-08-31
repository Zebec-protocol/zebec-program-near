use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise};

use crate::*;

// the `stream` structure is not json serialized 
// we need to convert it into json @todo

#[near_bindgen]
impl Contract {
    pub fn get_stream(&self, stream_id: Stream) -> Stream {
        self.streams.get(stream_id).unwrap()
    }
}
