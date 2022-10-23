#!/bin/sh

./build.sh

if [ $? -ne 0 ]; then
  echo ">> Error building contract"
  exit 1
fi

echo ">> Deploying contract"

# https://docs.near.org/tools/near-cli#near-dev-deploy
near dev-deploy --wasmFile ./target/wasm32-unknown-unknown/release/zebec.wasm

source ./neardev/dev-account.env

# near call $CONTRACT_NAME new '{"owner_id":"remora.testnet", "manager_id":"remora.testnet", "fee_receiver":"twojoy.testnet", "fee_rate":"25", "max_fee_rate": "200"}' --accountId remora.testnet


# add whitelist tokens first 


# register 
# near call $CONTRACT_NAME storage_deposit '{"account_id": "0xtestuser.testnet"}' --accountId '0xtestuser.testnet' --amount 0.00141