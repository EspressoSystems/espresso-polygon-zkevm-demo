use anyhow::{Error, Result};
use async_std::{channel::Receiver, sync::RwLock, task::JoinHandle};
use clap::Parser;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Middleware as _, Provider, StreamExt, Ws},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::{Address, TransactionRequest, H256, U256},
    utils::{parse_ether, ConversionError},
};

use std::{
    collections::{BinaryHeap, HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};
use url::Url;

// TODO
// - [X] Funding on startup.
// - Webserver
// - Separate web servers for faucet for easier testing?
// - Healthcheck
// - [X] Move receipt processing into separate method.
// - [X] Only process receipts of tracked transfers
// - Keep track of processed blocks.
// - Add method of fetching missed blocks (or just query all pending transactions for receipts on startup?) and process them.
// - Error handling

pub type Middleware = SignerMiddleware<Provider<Ws>, LocalWallet>;

#[derive(Parser, Debug, Clone)]
pub struct Options {
    /// Number of Ethereum accounts to use for the faucet.
    #[arg(long, env = "ESPRESSO_ZKEVM_FAUCET_NUM_CLIENTS", default_value = "10")]
    num_clients: usize,

    /// The mnemonic of the faucet wallet.
    #[arg(long, env = "ESPRESSO_ZKEVM_FAUCET_MNEMONIC")]
    mnemonic: String,

    /// Port on which to serve the API. (TODO: use for healthchecks)
    #[arg(
        short,
        long,
        env = "ESPRESSO_ZKEVM_ADAPTOR_RPC_PORT",
        default_value = "8545"
    )]
    port: u16,

    /// The amount of funds to grant to each account on startup in Ethers.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_FAUCET_GRANT_AMOUNT_ETHERS",
        value_parser = |arg: &str| -> Result<U256, ConversionError> { Ok(parse_ether(arg)?) }
    )]
    faucet_grant_amount: U256,

    /// The URL of the JsonRPC the faucet connects to.
    #[arg(long, env = "ESPRESSO_ZKEVM_FAUCET_PROVIDER_URL")]
    provider_url: Url,
}

// #[derive(Error, Debug)]
// enum FaucetError {
//     #[error("Error querying JsonRPC: {0}")]
//     Rpc(ProviderError),
// }

#[derive(Debug, Clone, Copy)]
pub enum Transfer {
    Faucet { to: Address, amount: U256 },
    Funding { to: Address },
}

impl Transfer {
    pub fn faucet(to: Address, amount: U256) -> Self {
        Self::Faucet { to, amount }
    }

    pub fn funding(to: Address) -> Self {
        Self::Funding { to }
    }

    pub fn to(&self) -> Address {
        match self {
            Self::Faucet { to, .. } => *to,
            Self::Funding { to } => *to,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ClientPool {
    clients: HashMap<Address, Arc<Middleware>>,
    priority: BinaryHeap<(U256, Address)>,
}

impl ClientPool {
    pub fn pop(&mut self) -> Option<Arc<Middleware>> {
        let (_, address) = self.priority.pop()?;
        let client = self.clients.remove(&address)?;
        Some(client)
    }

    pub fn push(&mut self, balance: U256, client: Arc<Middleware>) {
        self.clients.insert(client.address(), client.clone());
        self.priority.push((balance, client.address()));
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
struct State {
    available_clients: ClientPool,
    inflight_transfers: HashMap<H256, Transfer>,
    inflight_clients: HashMap<Address, Arc<Middleware>>,
    // Funding wallets has priority, these transfer requests must be pushed to
    // the front.
    transfer_queue: VecDeque<Transfer>,
    monitoring_started: bool,
}

#[derive(Debug, Clone)]
pub struct Faucet {
    config: Options,
    state: Arc<RwLock<State>>,
    /// Used to monitor Ethereum transactions.
    provider: Provider<Ws>,
    /// Channel to receive faucet requests.
    faucet_receiver: Arc<RwLock<Receiver<Address>>>,
}

impl Faucet {
    pub async fn create(options: Options, faucet_receiver: Receiver<Address>) -> Result<Self> {
        let provider = Provider::<Ws>::connect(options.provider_url.clone()).await?;
        let chain_id = provider.get_chainid().await?.as_u64();

        let mut state = State::default();
        let mut balances = vec![];
        let mut total_balance = U256::zero();

        // Create clients
        for index in 0..options.num_clients {
            let wallet = MnemonicBuilder::<English>::default()
                .phrase(&options.mnemonic[..])
                .index(index as u32)?
                .build()?
                .with_chain_id(chain_id);
            let client = Arc::new(Middleware::new(provider.clone(), wallet));

            let balance = provider.get_balance(client.address(), None).await?;
            total_balance += balance;
            balances.push(balance);

            tracing::info!(
                "Created client {index} {} with balance {balance}",
                client.address(),
            );

            // Fund all clients who have less than average balance.
            if balance < total_balance / options.num_clients {
                tracing::info!("Queuing funding transfer for {:?}", client.address());
                let transfer = Transfer::funding(client.address());
                state.transfer_queue.push_back(transfer);
                state.inflight_clients.insert(client.address(), client);
            } else {
                state.available_clients.push(balance, client);
            }
        }

        Ok(Self {
            config: options,
            state: Arc::new(RwLock::new(state)),
            provider,
            faucet_receiver: Arc::new(RwLock::new(faucet_receiver)),
        })
    }

    pub async fn start(self) -> JoinHandle<(Result<(), Error>, (), Result<(), Error>)> {
        let futures = async move {
            futures::join!(
                self.monitor_transactions(),
                self.monitor_faucet_requests(),
                self.execute_transfers()
            )
        };
        async_std::task::spawn(futures)
    }

    async fn request_transfer(&self, transfer: Transfer) {
        tracing::info!("Adding transfer to queue: {:?}", transfer);
        self.state.write().await.transfer_queue.push_back(transfer);
    }

    pub async fn request_faucet_transfer(&self, to: Address) {
        let transfer = Transfer::faucet(to, self.config.faucet_grant_amount);
        self.request_transfer(transfer).await;
    }

    async fn execute_transfers(&self) -> Result<()> {
        // Create transfer requests for available clients.
        loop {
            if self.state.read().await.monitoring_started {
                break;
            } else {
                tracing::info!("Waiting for transaction monitoring to start...");
                async_std::task::sleep(Duration::from_secs(1)).await;
            }
        }
        loop {
            let mut clients = vec![];
            let mut transfers = vec![];
            // Execute as many of the transfers as clients available.
            {
                let mut state = self.state.write().await;
                while !state.available_clients.is_empty() && !state.transfer_queue.is_empty() {
                    let sender = state.available_clients.pop().unwrap();
                    let transfer = state.transfer_queue.pop_front().unwrap();
                    clients.push(sender);
                    transfers.push(transfer);
                }
            }
            let mut failed = vec![];
            for (sender, transfer) in clients.into_iter().zip(transfers.into_iter()) {
                if let Err(err) = self.execute_transfer(sender.clone(), transfer).await {
                    tracing::error!("Failed to execute {transfer:?} {sender:?} {err:?}");
                    failed.push((sender, transfer));
                }
            }
            for (sender, transfer) in failed.iter() {
                // TODO: Need to figure out a way to handle it gracefully if
                // we're not able to talk to the JsonRPC at all.
                let balance = self.balance(sender.address()).await.unwrap_or_else(|err| {
                    panic!("Failed to get balance for {:?} {err}", sender.address())
                });
                // Put the client and transfer back in the queue.
                self.state
                    .write()
                    .await
                    .available_clients
                    .push(balance, sender.clone());
                self.request_transfer(*transfer).await;
            }
            // Wait a bit to avoid a busy loop.
            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn execute_transfer(&self, sender: Arc<Middleware>, transfer: Transfer) -> Result<()> {
        let amount = match transfer {
            Transfer::Faucet { amount, .. } => amount,
            Transfer::Funding { .. } => {
                // Send half the balance to the new receiving client.
                self.balance(sender.address()).await? / 2
            }
        };
        let tx = sender
            .send_transaction(TransactionRequest::pay(transfer.to(), amount), None)
            .await?;
        tracing::info!("Sending transfer: {:?} hash={:?}", transfer, tx.tx_hash());

        // Note: if running against an *extremely* fast chain , it is possible
        // that the transaction is mined before we have a chance to add it to
        // the inflight transfers. In that case, the receipt handler may not yet
        // find the transaction and fail to process it correctly. I think the
        // risk of this happening outside of local testing is neglible. We could
        // sign the tx locally first and then insert it but this also means we
        // would have to remove it again if the submission fails.
        {
            let mut state = self.state.write().await;
            state
                .inflight_clients
                .insert(sender.address(), sender.clone());
            state.inflight_transfers.insert(tx.tx_hash(), transfer);
        }
        Ok(())
    }

    async fn balance(&self, address: Address) -> Result<U256> {
        Ok(self.provider.get_balance(address, None).await?)
    }

    async fn handle_receipt(&self, tx_hash: H256) -> Result<()> {
        tracing::debug!("Got tx hash {:?}", tx_hash);

        let transfer = {
            if let Some(transfer) = self.state.write().await.inflight_transfers.remove(&tx_hash) {
                transfer
            } else {
                // Not a transaction we are monitoring.
                return Ok(());
            }
        };

        // In case there is a race condition and the receipt is not yet available, wait for it.
        let receipt = loop {
            if let Ok(Some(tx)) = self.provider.get_transaction_receipt(tx_hash).await {
                break tx;
            }
            async_std::task::sleep(Duration::from_secs(1)).await;
        };

        tracing::info!("Received receipt for {:?}", transfer);

        // Mark the sender as available
        let mut state = self.state.write().await;
        if let Some(client) = state.inflight_clients.remove(&receipt.from) {
            let balance = self.balance(client.address()).await?;
            state.available_clients.push(balance, client);
        } else {
            tracing::warn!(
                "Sender of transfer not found in inflight clients: {:?}",
                receipt.from
            );
        }

        // If the transaction succeeded, make the funded client available.
        if receipt.status == Some(1.into()) {
            // For funding transfer mark recipient as available.
            if let Transfer::Funding { to, .. } = transfer {
                if let Some(client) = state.inflight_clients.remove(&to) {
                    // TODO should not hold write lock over this external call.
                    let balance = self.balance(client.address()).await?;
                    tracing::info!("Client {to:?} funded @ {balance}");
                    state.available_clients.push(balance, client);
                } else {
                    tracing::warn!(
                        "Recipient of funding transfer not found in inflight clients: {:?}",
                        to
                    );
                }
            };

        // If the transaction failed send it again.
        // TODO: this code is currently untested.
        } else {
            tracing::warn!(
                "Transfer failed tx_hash={:?}, will resend: {:?}",
                tx_hash,
                transfer
            );
            state.transfer_queue.push_back(transfer);
        }

        // TODO: I think for transactions with bad nonces we would not even get
        // a transactions receipt. As a result the sending client would remain
        // stuck. As a workaround we could add a timeout to the inflight clients
        // and unlock them after a while. It may be difficult to set a good
        // fixed value for the timeout because the zkevm-node currently waits
        // for hotshot blocks being sequenced in the contract.

        Ok(())
    }

    async fn monitor_transactions(&self) -> Result<()> {
        let mut stream = self
            .provider
            .subscribe_blocks()
            .await
            .unwrap()
            .flat_map(|block| futures::stream::iter(block.transactions));
        self.state.write().await.monitoring_started = true;
        tracing::info!("Transaction monitoring started ...");
        while let Some(tx_hash) = stream.next().await {
            self.handle_receipt(tx_hash).await?;
        }
        unreachable!();
    }

    async fn monitor_faucet_requests(&self) {
        loop {
            if let Ok(address) = self.faucet_receiver.write().await.recv().await {
                self.request_transfer(Transfer::Faucet {
                    to: address,
                    amount: self.config.faucet_grant_amount,
                })
                .await;
            }
        }
    }
}

// Tests
#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};

    use hermez_adaptor::{Layer1Backend, SequencerZkEvmDemo};
    use sequencer_utils::AnvilOptions;

    const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

    #[async_std::test]
    async fn test_faucet_anvil() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;

        let mut ws_url = anvil.url();
        ws_url.set_scheme("ws").unwrap();
        let provider = Provider::<Ws>::connect(ws_url.clone())
            .await
            .expect("Unable to make websocket connection to L1");

        let options = Options {
            num_clients: 12, // With anvil 10 clients are pre-funded
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_grant_amount: 100.into(),
            provider_url: ws_url,
            port: 11223,
        };
        let (sender, receiver) = async_std::channel::unbounded();

        let faucet = Faucet::create(options.clone(), receiver).await?;

        let recipient = Address::random();
        let mut total_transfer_amount = U256::zero();
        for _ in 0..3 {
            sender.send(recipient).await.unwrap();
            total_transfer_amount += options.faucet_grant_amount;
        }

        let _handle = faucet.start().await;

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
    async fn test_faucet_zkevm_node() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let demo = SequencerZkEvmDemo::start_with_sequencer(
            "faucet-test".to_string(),
            Layer1Backend::Anvil,
        )
        .await;
        let env = demo.env();

        let mut ws_url = env.l2_provider();
        ws_url.set_scheme("ws").unwrap();
        ws_url.set_port(Some(8133)).unwrap(); // zkevm-node uses 8133 for websockets
        let provider = Provider::<Ws>::connect(ws_url.clone())
            .await
            .expect("Unable to make websocket connection to JsonRPC");

        let options = Options {
            num_clients: 2,
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_grant_amount: 100.into(),
            provider_url: ws_url,
            port: 11223,
        };
        let (sender, receiver) = async_std::channel::unbounded();

        let faucet = Faucet::create(options.clone(), receiver).await?;

        let recipient = Address::random();
        let mut total_transfer_amount = U256::zero();
        for _ in 0..2 {
            sender.send(recipient).await.unwrap();
            total_transfer_amount += options.faucet_grant_amount;
        }

        let _handle = faucet.start().await;

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
}
