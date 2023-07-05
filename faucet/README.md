# Demo Faucet

This faucet sends funds to users that request it in a Discord channel.

[TODO] document how to set up Discord bot with Discord UI or API.

## Usage
The easiest way to run the faucet is to run it against an anvil node:

```
anvil
cargo run --bin faucet -- \
  --mnemonic "test test test test test test test test test test test junk" \
  --faucet-grant-amount 1 --provider-url ws://localhost:8545

curl -X POST localhost:8111/faucet/request/0x1234567890123456789012345678901234567890
```
