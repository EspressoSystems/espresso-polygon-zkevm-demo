# Demo Faucet

This faucet sends funds to users that request it in a Discord channel.

## Usage
The easiest way to run the faucet is to run it against an anvil node:

```
anvil
cargo run --bin faucet -- \
  --mnemonic "test test test test test test test test test test test junk" \
  --faucet-grant-amount 1 --provider-url ws://localhost:8545

curl -X POST localhost:8111/faucet/request/0x1234567890123456789012345678901234567890
```

You can also run `just demo` to use the faucet with the zkevm-node. The the web
faucet will also be available listening at `http://localhost:8111`.

## Discord config

Note, for testing a throwaway private server is recommended. It can be created
by clicking the `+` sign in the left pane in the Discord web app. To add a
channel for the faucet click `+` next to `TEXT CHANNELS`.

1. Create a Discord app on https://discord.com/developers/applications.
1. On the application page, in the left pane, click `Bot`.
1. Click `Reset Token` and save the token. This will later be the value of the
   env var `ESPRESSO_ZKEVM_FAUCET_DISCORD_TOKEN`.
1. Disable `PUBLIC BOT`.
1. Enable `MESSAGE CONTENT INTENT`.
1. Click `Save Changes`.
1. In the left pane, click `OAuth2` -> `URL Generator`.
1. For `SCOPES` tick `bot`.
1. For `BOT PERMISSIONS` -> `TEXT PERMISSIONS` tick `Send Messages`.
1. At the bottom, copy the generated URL and open it in the browser.
1. Select the server to add the bot to, then hit `Continue`, then `Authorize`.
1. Navigate to the Discord server page. Go to `Server Settings` ->
   `Integrations` -> `Bots and Apps`.
1. Click the `Manage` button next to the entry of the bot.
1. Under `CHANNELS`, disable `# All Channels` and enable the channel(s) where
   the bot should accept faucet requests. Click `Save Changes`.

To test locally run `anvil` in one terminal. In another terminal run
 ```
env ESPRESSO_ZKEVM_FAUCET_DISCORD_TOKEN="..." cargo run --bin faucet -- \
  --mnemonic "test test test test test test test test test test test junk" \
  --faucet-grant-amount 1 --provider-url ws://localhost:8545
 ```
with your Discord bot token.

In Discord go to the faucet channel and write `/faucet 0x1234567890123456789012345678901234567890`.
