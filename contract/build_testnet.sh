#!/bin/sh

echo ">> Building contract for testing"
echo ">> This build is for deploying in testnet only\n\n"

rustup target add wasm32-unknown-unknown
cargo build --all --target wasm32-unknown-unknown --release --features testnet
