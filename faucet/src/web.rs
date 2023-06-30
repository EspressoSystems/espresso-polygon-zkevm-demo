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

pub(crate) async fn serve(port: u16, state: State) -> io::Result<()> {
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
pub(crate) struct State {
    faucet_queue: Sender<Address>,
}

impl State {
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
