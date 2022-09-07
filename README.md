# Zebec contracts
Implementation of Zebec in near protocol

## Structures

#### Basic structs

```rust
pub struct Contract {
    current_id: u64,
    streams: UnorderedMap<u64, Stream>,
}

```

```
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
    paused_time: Timestamp, // last paused time
}
```


## Main methods

### public functions
- `create_stream(&mut self, receiver: AccountId, stream_rate: U128, start: U64, end: U64)` - Create a new stream with given information

- `withdraw(&mut self, stream_id: U64)` - Withdraw amount accrued in the stream or the excess amount after the stream has ended
- `pause(&mut self, stream_id: U64)` - Pause the stream
- `resume(&mut self, stream_id: U64)` - Resume the stream
- `cancel(&mut self, stream_id: U64)` - Cancel the stream

### Views

- `get_stream(stream_id)` : returns all the details of the `stream_id`

