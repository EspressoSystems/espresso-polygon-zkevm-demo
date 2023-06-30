use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::channel::Sender;
use async_std::sync::RwLock;
use async_std::task::spawn;
use clap::Parser;
use ethers::types::Address;
use faucet::{Faucet, Options};
use futures::FutureExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::env;
use std::io;
use thiserror::Error;
use tide_disco::RequestError;
use tide_disco::{http::StatusCode, Api, App, Error};

#[derive(Clone, Debug, Deserialize, Serialize, Error)]
enum FaucetError {
    #[error("faucet error {status}: {msg}")]
    FaucetError { status: StatusCode, msg: String },
    #[error("unable to parse Ethereum address: {input}")]
    BadAddress { status: StatusCode, input: String },
}

impl tide_disco::Error for FaucetError {
    fn catch_all(status: StatusCode, msg: String) -> Self {
        Self::FaucetError { status, msg }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::FaucetError { status, .. } => *status,
            Self::BadAddress { status, .. } => *status,
        }
    }
}

impl From<RequestError> for FaucetError {
    fn from(err: RequestError) -> Self {
        Self::catch_all(StatusCode::BadRequest, err.to_string())
    }
}

async fn serve(port: u16, state: State) -> io::Result<()> {
    let mut app = App::<_, FaucetError>::with_state(RwLock::new(state));
    app.with_version(env!("CARGO_PKG_VERSION").parse().unwrap());

    // Include API specification in binary
    let toml = toml::from_str::<toml::value::Value>(include_str!("api.toml"))
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    let mut api = Api::<RwLock<State>, FaucetError>::new(toml).unwrap();
    api.with_version(env!("CARGO_PKG_VERSION").parse().unwrap());

    // Can invoke with
    //    `curl -i -X POST http://0.0.0.0:8111/faucet/request/0x1234567890123456789012345678901234567890`
    api.post("request", |req, state| {
        async move {
            let address = req.string_param("address")?;
            let address = address.parse().map_err(|_| FaucetError::BadAddress {
                status: StatusCode::BadRequest,
                input: address.to_string(),
            })?;
            tracing::info!("Received faucet request for {:?}", address);
            state
                .faucet_queue
                .send(address)
                .await
                .map_err(|err| FaucetError::FaucetError {
                    status: StatusCode::InternalServerError,
                    msg: err.to_string(),
                })?;
            Ok(())
        }
        .boxed()
    })
    .unwrap();

    app.register_module("faucet", api).unwrap();
    app.serve(format!("0.0.0.0:{}", port)).await
}

#[derive(Clone, Debug)]
struct State {
    faucet_queue: Sender<Address>,
}

impl State {
    fn new(faucet_queue: Sender<Address>) -> Self {
        Self { faucet_queue }
    }
}

// Suggestions for improvements:
// - After starting up, process messages sent since last online.

#[async_trait]
impl EventHandler for State {
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
                chat_response = format!("Sending funds to {}", addr.as_str());
                if let Ok(address) = addr.as_str().parse::<Address>() {
                    if let Err(err) = self.faucet_queue.send(address).await {
                        tracing::error!(
                            "Failed make faucet request for address {:?}: {}",
                            address,
                            err
                        );
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
async fn main() -> io::Result<()> {
    // Configure the client with your Discord bot token in the environment.
    setup_logging();
    setup_backtrace();

    let opts = Options::parse();

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let (sender, receiver) = async_std::channel::unbounded();
    let state = State::new(sender);
    let faucet = Faucet::create(opts.clone(), receiver)
        .await
        .expect("Failed to create faucet");

    let discord_client = if let Ok(token) = env::var("ESPRESSO_ZKEVM_FAUCET_DISCORD_TOKEN") {
        // Set gateway intents, which decides what events the bot will be notified about
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        let client = Client::builder(&token, intents)
            .event_handler(state.clone())
            .await
            .expect("Err creating client");
        Some(client)
    } else {
        tracing::warn!("Discord token not set in ESPRESSO_ZKEVM_FAUCET_DISCORD_TOKEN");
        tracing::error!("Running without Discord bot. For local testing this is expected.");
        None
    };

    let faucet_handle = spawn(faucet.start());
    let api_handle = spawn(serve(opts.port(), state));

    if let Some(mut discord) = discord_client {
        let _result = futures::join!(faucet_handle, api_handle, discord.start());
    } else {
        let _result = futures::join!(faucet_handle, api_handle);
    };
    Ok(())
}
