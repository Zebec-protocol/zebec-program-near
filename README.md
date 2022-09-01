# Zebec contracts

## Structures

#### Basic structs

- `AccountId: string`
- `StreamId: string`


```rust
pub struct Stream {
    id: String,
    sender: AccountId,
    receiver: AccountId,
    balance: U128,
    rate: U128,  
    created: Timestamp,
    status: StreamStatus,
    startTime: Timestamp,
    endTime: Timestamp,
    withdrawTime: Timestamp,
    isPaused: bool,
}
```


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


### Edge Cases
- [ ] How does funding works, does all amount needs to be funded at creation
      - At creation

- [ ] Balance runs out of the stream and the user tries to withdraw
      - Cannot run out since it is reserved at creation

- [ ] Can provied excess amount while funding
      - Yes

- [ ] Who can trigger withdraw
      - Receiver only

- [ ] Paused stream, can receiver still withdraw amount until the stream was paused
      - Yes

- [ ] Paused and resumed, can the user get the amount for the paused duration or not
      - No
