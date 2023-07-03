//! A discord event handler for the faucet.
//!
//! Suggestions for improvements:
//!   - After starting up, process messages sent since last online.
use crate::serve;
use crate::WebState;
use crate::{Faucet, Options};
use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::task::spawn;
use clap::Parser;
use ethers::types::Address;
use regex::Regex;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::io;

#[async_trait]
impl EventHandler for WebState {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        // Don't respond to messages by bots, (which includes this bot).
        if msg.author.bot {
            return;
        }

        // Try to find an ethereum address in the message body.
        let re = Regex::new("0x[a-fA-F0-9]{40}").unwrap();
        let mut chat_response = Default::default();

        if let Some(matched) = re.captures(&msg.content) {
            if let Some(addr) = matched.get(0) {
                if let Ok(address) = addr.as_str().parse::<Address>() {
                    if let Err(err) = self.request(address).await {
                        tracing::error!("Failed make faucet request for {address:?}: {}", err);
                        chat_response =
                            format!("Internal Error: Failed to send funds to {address:?}");
                    } else {
                        chat_response = format!("Sending funds to {address:?}");
                    }
                } else {
                    // This shouldn't happen because the regex should only match
                    // valid addresses.
                    tracing::error!("Invalid address: {}", addr.as_str());
                }
            }
        } else {
            chat_response = "No address found!".to_string();
        }
        if let Err(why) = msg.reply(&ctx.http, chat_response).await {
            tracing::error!("Error sending message: {:?}", why);
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    async fn ready(&self, _: Context, ready: Ready) {
        tracing::info!("{} is connected!", ready.user.name);
    }
}

#[async_std::main]
pub async fn main() -> io::Result<()> {
    // Configure the client with your Discord bot token in the environment.
    setup_logging();
    setup_backtrace();

    let opts = Options::parse();

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let (sender, receiver) = async_std::channel::unbounded();
    let state = WebState::new(sender);
    let faucet = Faucet::create(opts.clone(), receiver)
        .await
        .expect("Failed to create faucet");

    let discord_client = if opts.discord_enable {
        // Set gateway intents, which decides what events the bot will be notified about
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        let client = Client::builder(&opts.discord_token.unwrap(), intents)
            .event_handler(state.clone())
            .await
            .expect("Err creating discord client");
        Some(client)
    } else {
        tracing::warn!("Discord bot disabled. For local testing this is fine.");
        None
    };

    let faucet_handle = spawn(faucet.start());
    let api_handle = spawn(serve(opts.port, state));

    if let Some(mut discord) = discord_client {
        let _result = futures::join!(faucet_handle, api_handle, discord.start());
    } else {
        let _result = futures::join!(faucet_handle, api_handle);
    };
    Ok(())
}
