use near_sdk::{near_bindgen};

use crate::*;

// the `stream` structure is not json serialized 
// we need to convert it into json @todo

#[near_bindgen]
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
