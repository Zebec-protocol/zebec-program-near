# Zebec contracts

## Structures

#### Basic structs

- `AccountId: string`
- `StreamId: string`

```rust
pub enum StreamStatus {
    Initialized,
    Active,
    Paused,
    Finished,
}
```

```rust
pub struct Stream {
    id: String,
    sender: AccountId,
    receiver: AccountId,
    balance: U128,
    rate: U128,  
    created: Timestamp,
    status: StreamStatus,
}
```

#### `Stream` status

- Enums of state
  - Initialized
  - Active
  - Paused
  - Finished

## Main methods

- `create_stream()`
- `withdraw()`
- `pause()`
- `resume()`

### Views

- `get_stream(stream_id)` : returns all the details of the `stream_id`

### Todos

- [x] data-structures and main functions (with input guards for input sanity)
- [x] view functions & events, uint tests
- [ ] finalize native token integration and handle gas (gas fee, Near deposits, refunds, storage staking)
- [ ] testnet deployment
- [ ] cross-contract calls and stablecoin integration
- [ ] finalize unit tests and integration
