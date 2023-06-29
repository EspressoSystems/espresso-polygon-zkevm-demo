use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::channel::Sender;
use clap::Parser;
use ethers::types::Address;
use faucet::{Faucet, Options};
use regex::Regex;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::env;

// Suggestions for improvements:
// - After starting up, process messages sent since last online.

struct Handler {
    faucet_queue: Sender<Address>,
}

impl Handler {
    fn new(faucet_queue: Sender<Address>) -> Self {
        Self { faucet_queue }
    }
}

#[async_trait]
impl EventHandler for Handler {
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
        let mut reply = "".to_string();

        if let Some(matched) = re.captures(&msg.content) {
            if let Some(addr) = matched.get(0) {
                reply = format!("Sending funds to {}", addr.as_str());
                if let Ok(address) = addr.as_str().parse::<Address>() {
                    if let Err(err) = self.faucet_queue.send(address).await {
                        tracing::error!("Failed make faucet request for address: {}", err);
                    }
                } else {
                    // This shouldn't happen because the regex should only match
                    // valid addresses.
                    tracing::error!("Invalid address: {}", addr.as_str());
                }
                // TODO actually send funds
            }
        } else {
            reply = "No address found!".to_string();
        }
        if let Err(why) = msg.reply(&ctx.http, reply).await {
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
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    setup_logging();
    setup_backtrace();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let opts = Options::parse();
    let (sender, receiver) = async_std::channel::unbounded();
    let faucet = Faucet::create(opts, receiver)
        .await
        .expect("Failed to create faucet");

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler::new(sender))
        .await
        .expect("Err creating client");
    let _handle = faucet.start().await;

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        panic!("Client error: {:?}", why);
    }
}
