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
use serenity::{
    async_trait,
    model::{
        gateway::Ready,
        prelude::{
            command::{Command, CommandOptionType},
            interaction::{
                application_command::{CommandDataOption, CommandDataOptionValue},
                Interaction, InteractionResponseType,
            },
        },
    },
    prelude::{Context, EventHandler, GatewayIntents},
    Client,
};
use std::io;

impl WebState {
    async fn handle_faucet_request(&self, options: &[CommandDataOption]) -> String {
        let option = options
            .get(0)
            .expect("Expected address option")
            .resolved
            .as_ref()
            .expect("Expected user object");
        match option {
            CommandDataOptionValue::String(input) => {
                // Try to find an ethereum address in the message body.
                let re = Regex::new("0x[a-fA-F0-9]{40}").unwrap();

                if let Some(matched) = re.captures(input) {
                    let address = matched
                        .get(0)
                        .expect("At least one match")
                        .as_str()
                        .parse::<Address>()
                        .expect("Address can be parsed after matching regex");
                    if let Err(err) = self.request(address).await {
                        tracing::error!("Failed make faucet request for {address:?}: {}", err);
                        format!("Internal Error: Failed to send funds to {address:?}")
                    } else {
                        format!("Sending funds to {address:?}")
                    }
                } else {
                    "No address found!".to_string()
                }
            }
            _ => unreachable!(),
        }
    }
}

#[async_trait]
impl EventHandler for WebState {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            tracing::info!("Received command interaction: {:#?}", command);

            let content = match command.data.name.as_str() {
                "faucet" => self.handle_faucet_request(&command.data.options).await,
                _ => "not implemented".to_string(),
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(content))
                })
                .await
            {
                tracing::error!("Cannot respond to slash command: {}", why);
            }
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    async fn ready(&self, ctx: Context, ready: Ready) {
        tracing::info!("{} is connected!", ready.user.name);

        Command::create_global_application_command(&ctx.http, |command| {
            command
                .name("faucet")
                .description("Request funds from the faucet")
                .create_option(|option| {
                    option
                        .name("address")
                        .description("Your ethereum address")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .await
        .expect("Command creation succeeds");
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

    // Do not attempt to start the discord bot if the token is missing or empty.
    let discord_client = if let Some(token) = opts.discord_token.filter(|token| !token.is_empty()) {
        // Set gateway intents, which decides what events the bot will be notified about
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        let client = Client::builder(token, intents)
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
