
```bash

cargo build --all --target wasm32-unknown-unknown --release
near deploy stream.remora.testnet --wasmFile '/Users/user/development/blockchain/near-protocol/zebec-near/contract/target/wasm32-unknown-unknown/release/zebec.wasm'

near state stream.remora.testnet


near call stream.remora.testnet new ----accountId remora.testnet

near call stream.remora.testnet create_stream '{"receiver": "stream.remora.testnet", "stream_rate":"1", "start":"1762707445051569700", "end": "1862707445051569700"}' ----accountId remora.testnet

near call stream.remora.testnet get_stream '{"stream_id": "1"}' ----accountId remora.testnet


near call stream.remora.testnet  create_stream '{"receiver": "sub1.twojoy.testnet", "stream_rate": "10000000", "start": "1662709833946590000", "end": "1672707999000000000"}' --amount 0.000004 --account-id remora.testnet ----depositYocto 10000000010000000000

near call stream.remora.testnet  create_stream '{"receiver": "sub1.twojoy.testnet", "stream_rate": "1000", "start": "1663056149517379000", "end": "1663063349517379000"}' --depositYocto 600000000000000 --account-id remora.testnet
```

### script to calculate the stream_amount

```python
def calculate_stream_amount():
    #the caculations are done in nanoseconds
    near = 10 ** 24
    # 10 * 60 * 60 secs into nano seconds 
    start_after = 10 * 60 * 10 ** 9  # 10 minutes 
    stream_rate = 1000  # in yocto (10 ** 24)
    start_time = time.time_ns() + start_after
    two_hours = 2 * 60 * 60 * 10 ** 9 
    end_time = start_time + two_hours ## ends after 2 hours 
    amount = (end_time - start_time)  * stream_rate
    print(f"start: {start_time}, end: {end_time}, stream_rate={stream_rate} depositYocto:{amount}")
```

```bash
near call stream.remora.testnet  create_stream '{"receiver": "sub1.twojoy.testnet", "stream_rate": "1000", "start": "1663050047749232000", "end": "1663057247749232000"}' --depositYocto 7200000000000000 --account-id remora.testnet

# output
Scheduling a call: stream.remora.testnet.create_stream({"receiver": "sub1.twojoy.testnet", "stream_rate": "1000", "start": "1662711286687034000", "end": "1662711886687034000"}) with attached 0.0000000006 NEAR
Doing account.functionCall()
Receipt: EvnrhmDvuE8YfaoLFhF39Ya6BULFVVKBdv25tbJDVMtN
        Log [stream.remora.testnet]: Saving streams 2
Transaction Id 8qP8hGXxkyRsvJNqypv8v1wAPanUcUTKjFZvPpDdnJRh
To see the transaction in the transaction explorer, please open this url in your browser
https://explorer.testnet.near.org/transactions/8qP8hGXxkyRsvJNqypv8v1wAPanUcUTKjFZvPpDdnJRh
```

#### Pause the stream

```bash
near call stream.remora.testnet pause '{"stream_id": "1"}' ----accountId remora.testnet
# check status 
near call stream.remora.testnet get_stream '{"stream_id": "1"}' ----accountId remora.testnet


# output
Scheduling a call: stream.remora.testnet.get_stream({"stream_id": "1"})
Doing account.functionCall()
Transaction Id Hkbb8fiBcSxnT1jgZwxNER5NUobd5oQNEzcsh8oQ68km
To see the transaction in the transaction explorer, please open this url in your browser
https://explorer.testnet.near.org/transactions/Hkbb8fiBcSxnT1jgZwxNER5NUobd5oQNEzcsh8oQ68km
{
  id: 1,
  sender: 'remora.testnet',
  receiver: 'sub1.twojoy.testnet',
  balance: 600000000000000,
  rate: 1000,
  created: 1662709960905007000,
  start_time: 1662710403284144000,
  end_time: 1662711003284144000,
  withdraw_time: 1662710403284144000,
  is_paused: true,
  paused_time: 1662710878421928000,
  contract_id: 'near.testnet'
}
```

### Resume the stream

```bash
near call stream.remora.testnet resume '{"stream_id": "1"}' ----accountId remora.testnet
# check status 
near view stream.remora.testnet get_stream '{"stream_id": "1"}' ----accountId remora.testnet


# output
View call: stream.remora.testnet.get_stream({"stream_id": "1"})
{
  id: 1,
  sender: 'remora.testnet',
  receiver: 'sub1.twojoy.testnet',
  balance: 600000000000000,
  rate: 1000,
  created: 1662709960905007000,
  start_time: 1662710403284144000,
  end_time: 1662711003284144000,
  withdraw_time: 1662710528146360000,
  is_paused: false,
  paused_time: 0,
  contract_id: 'near.testnet'
}
```

### withdraw the funds

```
near call stream.remora.testnet withdraw '{"stream_id": "1"}' ----accountId remora.testnet  
```

## Test for NEP-141 token

### 1. Create stream

1. Acquire wrapped Near to transfer from `wrap.testnet`

```bash
near call wrap.testnet near_deposit --deposit 1 --accountId remora.testnet
```

2. register both sender and receiver in the FT contract

- create receiver subaccount

```
near create-account receiver.remora.testnet --masterAccount remora.testnet
```

- deposit min to register

```
// register 
near call wrap.testnet storage_deposit --accountId remora.testnet --amount 0.00125
```

- get the storage bound
- `near view wrap.testnet storage_balance_bounds`
- 1250000000000000000000 = 0.0125
- we can register other too

```
near call wrap.testnet storage_deposit '{"account_id": "stream.remora.testnet"}' --accountId remora.testnet --amount 0.00125
```

```


near call wrap.testnet ft_transfer_call '{"amount": "7200000000000000","receiver_id": "stream.remora.testnet", "memo": "test", "msg":"{\"method_name\": \"create_stream\", \"receiver\":\"sub1.twojoy.testnet\",\"stream_rate\":\"1000\",\"start\":\"1663064453618110000\",\"end\":\"1663071653618110000\"}"}' --depositYocto 1 --gas 200000000000000 --accountId remora.testnet


{
  amount: 100,
  receiver: "twojoy",
"msg" : {
  method_name: "create_stream",
  "receiver": "sub1.twojoy.testnet",
  "stream_rate": "1000",
  "start":"1663050047749232000",
  "end": "1663057247749232000",
  }
}



near call stream.remora.testnet  create_stream '{"receiver": "sub1.twojoy.testnet", "stream_rate": "1000", "start": "1663050047749232000", "end": "1663057247749232000"}' --depositYocto 7200000000000000 --account-id remora.testnet
```

Tests:

1. fails when the sender contract is not whitelisted:

- In the call below the sender contract (wrap.testnet) is not valid_ft_sender

```
 near call wrap.testnet ft_transfer_call '{"amount": "7200000000000000","receiver_id": "stream.remora.testnet", "memo": "test", "msg":"{\"create_stream\":{\"receiver\":\"sub1.twojoy.testnet\",\"stream_rate\":\"1000\",\"start\":\"1663052207473710000\",\"end\":\"1663059407473710000\"}}"}' --depositYocto 1 --gas 200000000000000 --accountId remora.testnet

Scheduling a call: wrap.testnet.ft_transfer_call({"amount": "7200000000000000","receiver_id": "stream.remora.testnet", "memo": "test", "msg": "{\"create_stream\\\":{\\\"receiver\\\":\"sub1.twojoy.testnet\",\"stream_rate\":\"1000\",\"start\":\"1663052207473710000\",,\"end\":\"1663059407473710000\"}}"}) with attached 0.000000000000000000000001 NEAR
Doing account.functionCall()
Receipts: 3PGLvkCz2E8XwAx57bnXqFgxHLPC6VpNdR2v22Qg3sCq, HEVADECnhKhe6aKWxQUJrFgExMt3CqRb5hmhqEycYPDP, 93TAdbsCM2NUWH7hrPnNJ76N3ns22BDqRGvLBs9Ns7zz
 Log [wrap.testnet]: Transfer 7200000000000000 from remora.testnet to stream.remora.testnet
 Log [wrap.testnet]: Memo: test
Receipt: 69BYe42thSu19kboRCLna44PB5NU6BDUGmswuvgA9vJy
 Failure [wrap.testnet]: Error: {"index":0,"kind":{"ExecutionError":"Smart contract panicked: panicked at 'assertion failed: Self::valid_ft_sender(env::predecessor_account_id())', src/calls.rs:88:9"}}
Receipt: HySNDbG1GKMtpQa5HJiWMEx2hzvyfGJV5P1VPfVMJDnj
 Log [wrap.testnet]: Refund 7200000000000000 from stream.remora.testnet to remora.testnet
Transaction Id Cm9J5E83z3K1hA8rHUQZfD9jsoVhboA86GR2yqxsBMRB
To see the transaction in the transaction explorer, please open this url in your browser
https://explorer.testnet.near.org/transactions/Cm9J5E83z3K1hA8rHUQZfD9jsoVhboA86GR2yqxsBMRB
'0'
```
