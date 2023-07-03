//! Web server for the discord faucet.
//!
//! Serves these purposes:
//! 1. Provide a healthcheck endpoint for the discord bot, so it can be automatically
//!    restarted if it fails.
//! 2. Test and use the faucet locally without connecting to Discord.
use async_std::channel::Sender;
use async_std::sync::RwLock;
use ethers::types::Address;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::env;
use std::io;
use thiserror::Error;
use tide_disco::RequestError;
use tide_disco::{http::StatusCode, Api, App, Error};

#[derive(Clone, Debug, Deserialize, Serialize, Error)]
pub enum FaucetError {
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

pub(crate) async fn serve(port: u16, state: WebState) -> io::Result<()> {
    let mut app = App::<_, FaucetError>::with_state(RwLock::new(state));
    app.with_version(env!("CARGO_PKG_VERSION").parse().unwrap());

    // Include API specification in binary
    let toml = toml::from_str::<toml::value::Value>(include_str!("api.toml"))
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    let mut api = Api::<RwLock<WebState>, FaucetError>::new(toml).unwrap();
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
            state.request(address).await?;
            Ok(())
        }
        .boxed()
    })
    .unwrap();

    app.register_module("faucet", api).unwrap();
    app.serve(format!("0.0.0.0:{}", port)).await
}

#[derive(Clone, Debug)]
pub(crate) struct WebState {
    faucet_queue: Sender<Address>,
}

impl WebState {
    pub fn new(faucet_queue: Sender<Address>) -> Self {
        Self { faucet_queue }
    }

    pub async fn request(&self, address: Address) -> Result<(), FaucetError> {
        self.faucet_queue
            .send(address)
            .await
            .map_err(|err| FaucetError::FaucetError {
                status: StatusCode::InternalServerError,
                msg: err.to_string(),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::faucet::{Faucet, Options};
    use anyhow::Result;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::task::spawn;
    use ethers::{
        providers::{Middleware, Provider, Ws},
        types::U256,
        utils::parse_ether,
    };
    use hermez_adaptor::{Layer1Backend, SequencerZkEvmDemo};
    use sequencer_utils::AnvilOptions;
    use std::time::Duration;
    use surf_disco::Client;

    async fn run_faucet_test(options: Options) -> Result<()> {
        let client =
            Client::<FaucetError>::new(format!("http://localhost:{}", options.port).parse()?);
        // Avoids waiting 10 seconds for the retry in `connect`.
        async_std::task::sleep(Duration::from_millis(100)).await;
        client.connect(None).await;

        let recipient = Address::random();
        let mut total_transfer_amount = U256::zero();

        for _ in 0..3 {
            client
                .post(&format!("faucet/request/{recipient:?}"))
                .send()
                .await?;

            total_transfer_amount += options.faucet_grant_amount;
        }

        let provider = Provider::<Ws>::connect(options.provider_url).await?;
        loop {
            let balance = provider.get_balance(recipient, None).await.unwrap();
            tracing::info!("Balance is {balance}");
            if balance == total_transfer_amount {
                break;
            }
            async_std::task::sleep(Duration::from_secs(1)).await;
        }

        Ok(())
    }

    #[async_std::test]
    async fn test_faucet_anvil() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;

        let mut ws_url = anvil.url();
        ws_url.set_scheme("ws").unwrap();

        // With anvil 10 clients are pre-funded. We use more than that to make
        // sure the funding logic runs.
        let options = Options {
            num_clients: 12,
            faucet_grant_amount: parse_ether(1).unwrap(),
            provider_url: ws_url,
            ..Default::default()
        };

        let (sender, receiver) = async_std::channel::unbounded();

        // Start the faucet
        let faucet = Faucet::create(options.clone(), receiver).await?;
        let _handle = faucet.start().await;

        // Start the web server
        spawn(async move { serve(options.port, WebState::new(sender)).await });

        run_faucet_test(options).await?;
        Ok(())
    }

    // TODO: un-comment this when faucet docker image is in main
    #[ignore]
    #[async_std::test]
    async fn test_faucet_zkevm_node() -> Result<()> {
        setup_logging();
        setup_backtrace();

        // Use fewer clients to shorten test time.
        let num_clients = 2;
        std::env::set_var("ESPRESSO_ZKEVM_FAUCET_NUM_CLIENTS", num_clients.to_string());

        let demo = SequencerZkEvmDemo::start_with_sequencer(
            "faucet-test".to_string(),
            Layer1Backend::Anvil,
        )
        .await;
        let env = demo.env();

        // Connect to the faucet running inside the docker compose environment.
        let mut ws_url = env.l2_provider();
        ws_url.set_scheme("ws").unwrap();
        ws_url.set_port(Some(8133)).unwrap(); // zkevm-node uses 8133 for websockets

        let options = Options {
            num_clients,
            faucet_grant_amount: parse_ether(1000).unwrap(), // Needs to match the faucet grant amount the .env file
            provider_url: ws_url,
            ..Default::default()
        };
        run_faucet_test(options).await?;
        Ok(())
    }
}
