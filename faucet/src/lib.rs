use anyhow::Result;
use async_std::sync::RwLock;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Middleware as _, Provider, ProviderError, StreamExt, Ws},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::{Address, TransactionRequest, H256, U256},
};
use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

// TODO
// - [X] Funding on startup.
// - Separate web server for faucet for easier testing
// - Webserver
// - Healthcheck
// - [X] Move receipt processing into separate method.
// - [X] Only process receipts of tracked transfers
// - Keep track of processed blocks.
// - Add method of fetching missed blocks (or just query all pending transactions for receipts on startup?) and process them.
// - Maybe create an enum for receipts (funding, faucet) so it's easier to process incoming receipts
// - Error handling

pub type Middleware = SignerMiddleware<Provider<Ws>, LocalWallet>;

#[derive(Error, Debug)]
enum FaucetError {
    #[error("Error querying JsonRPC: {0}")]
    Rpc(ProviderError),
}

#[derive(Debug, Clone)]
struct FaucetRequest;

#[derive(Debug, Clone)]
struct Client;

#[derive(Debug, Clone, Copy)]
enum TransferKind {
    Funding,
    Faucet,
}

#[derive(Debug, Clone, Copy)]
enum Transfer {
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
struct State {
    available_clients: Vec<Arc<Middleware>>,
    faucet_request: Vec<FaucetRequest>,
    inflight_clients: HashMap<Address, Arc<Middleware>>,
    inflight_transfers: HashMap<H256, Transfer>,
    // Funding wallets has priority, these transfer requests must be pushed to
    // the front.
    transfer_queue: VecDeque<Transfer>,
    monitoring_started: bool,
    // num_pending_client_funding_transfers: usize, // not needed?
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "State: available={} faucet_requests={} inflight_clients={} inflight_transfers={} transfer_queue={}",
            self.available_clients.len(),
            self.faucet_request.len(),
            self.inflight_clients.len(),
            self.inflight_transfers.len(),
            self.transfer_queue.len(),
        )
    }
}

#[derive(Debug, Clone)]
struct Options {
    num_clients: usize,
    mnemonic: String,
    faucet_grant_amount: U256,
}

#[derive(Debug, Clone)]
struct Faucet {
    config: Options,
    state: Arc<RwLock<State>>,
    /// Used to monitor Ethereum transactions.
    provider: Provider<Ws>,
}

impl Faucet {
    pub fn new(options: Options, chain_id: u64, provider: Provider<Ws>) -> Self {
        // Create the clients
        let mut state = State::default();
        for index in 0..options.num_clients {
            let wallet = MnemonicBuilder::<English>::default()
                .phrase(&options.mnemonic[..])
                .index(index as u32)
                .unwrap()
                .build()
                .unwrap()
                .with_chain_id(chain_id);
            let client = Arc::new(Middleware::new(provider.clone(), wallet));
            tracing::info!("Created client {} {}", index, client.address());
            state.available_clients.push(client);
        }
        Self {
            config: options,
            state: Arc::new(RwLock::new(state)),
            provider,
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut balances = vec![];
        let mut total_balance = U256::zero();
        let mut state = self.state.write().await;
        for client in state.available_clients.iter() {
            let balance = self.balance(client.address()).await?;
            tracing::info!("Initial balance {:?}: {}", client.address(), balance);
            balances.push(balance);
            total_balance += balance;
        }
        let mut clients = vec![];
        clients.append(&mut state.available_clients);

        // Fund all clients who have less that (1 / num_clients) of the total.
        for (client, balance) in clients.iter().zip(balances) {
            if balance < total_balance / clients.len() {
                tracing::info!("Queuing funding transfer for {:?}", client.address());
                let transfer = Transfer::funding(client.address());
                state.transfer_queue.push_back(transfer);
                state
                    .inflight_clients
                    .insert(client.address(), client.clone());
            } else {
                state.available_clients.push(client.clone());
            }
        }
        Ok(())
    }

    async fn request_transfer(&self, transfer: Transfer) {
        tracing::info!("Adding transfer to queue: {:?}", transfer);
        self.state.write().await.transfer_queue.push_back(transfer);
    }

    async fn execute_transfers(&self) {
        // Create transfer requests for available clients.
        loop {
            if self.state.read().await.monitoring_started {
                break;
            } else {
                tracing::info!("Waiting for monitoring to start...");
                async_std::task::sleep(Duration::from_secs(1)).await;
            }
        }
        loop {
            let mut clients = vec![];
            let mut transfers = vec![];
            {
                let mut state = self.state.write().await;
                while !state.available_clients.is_empty() && !state.transfer_queue.is_empty() {
                    let sender = state.available_clients.pop().unwrap();
                    let transfer = state.transfer_queue.pop_front().unwrap();
                    clients.push(sender);
                    transfers.push(transfer);
                }
            }

            tracing::info!(
                "Executing {} transfers, inflight {}, in queue {}",
                transfers.len(),
                self.state.read().await.inflight_transfers.len(),
                self.state.read().await.transfer_queue.len()
            );

            for (sender, transfer) in clients.into_iter().zip(transfers.into_iter()) {
                let amount = match transfer {
                    Transfer::Faucet { amount, .. } => amount,
                    Transfer::Funding { .. } => {
                        // Send half the balance to the new receiving client.
                        sender.get_balance(sender.address(), None).await.unwrap() / 2
                    }
                };
                let mut tx = TransactionRequest::pay(transfer.to(), amount).into();
                sender
                    .fill_transaction(&mut tx, None)
                    .await
                    .expect("failed to fill transaction"); // TODO handle this error
                let signature = sender
                    .sign_transaction(&tx, sender.address())
                    .await
                    .unwrap();
                let tx_hash = tx.hash(&signature);
                tracing::info!("Sending transfer: {:?} hash={:?}", transfer, tx_hash);
                {
                    let mut state = self.state.write().await;
                    state
                        .inflight_clients
                        .insert(sender.address(), sender.clone());
                    state.inflight_transfers.insert(tx_hash, transfer);
                }
                sender
                    .send_raw_transaction(tx.rlp_signed(&signature))
                    .await
                    .unwrap();
            }
            // Wait a bit to avoid a busy loop.
            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn balance(&self, address: Address) -> Result<U256> {
        Ok(self.provider.get_balance(address, None).await?)
    }

    async fn handle_receipt(&self, tx_hash: H256) -> Result<()> {
        tracing::info!("Got tx hash {:?}", tx_hash);
        tracing::info!("State: {}", self.state.read().await);

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
            state.available_clients.push(client);
        } else {
            tracing::warn!(
                "Sender of transfer not found in inflight clients: {:?}",
                receipt.from
            );
        }

        // If the transaction succeeded, update the state of for funding
        // transfers.
        if receipt.status == Some(1.into()) {
            // For funding transfer mark recipient as available.
            if let Transfer::Funding { to, .. } = transfer {
                if let Some(client) = state.inflight_clients.remove(&to) {
                    tracing::info!("Client funded: {:?}", client.address());
                    state.available_clients.push(client);
                } else {
                    tracing::warn!(
                        "Recipient of funding transfer not found in inflight clients: {:?}",
                        to
                    );
                }
            };

        // If the transaction failed resend it.
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

    async fn monitor_transactions(&self) {
        let mut stream = self
            .provider
            .subscribe_blocks()
            .await
            .unwrap()
            .flat_map(|block| futures::stream::iter(block.transactions));
        self.state.write().await.monitoring_started = true;
        while let Some(tx_hash) = stream.next().await {
            self.handle_receipt(tx_hash).await;
        }
        unreachable!();
    }
}

// Tests
#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::task::spawn;
    use hermez_adaptor::{Layer1Backend, SequencerZkEvmDemo};
    use sequencer_utils::AnvilOptions;

    const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

    #[async_std::test]
    async fn test_faucet_anvil() {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;

        let mut ws_url = anvil.url();
        ws_url.set_scheme("ws").unwrap();
        let provider = Provider::<Ws>::connect(ws_url)
            .await
            .expect("Unable to make websocket connection to L1");

        let options = Options {
            num_clients: 12, // With anvil 10 clients are pre-funded
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_grant_amount: 100.into(),
        };
        let chain_id = 31337;
        let faucet = Faucet::new(options, chain_id, provider.clone());
        faucet.start().await.unwrap();

        let recipient = Address::random();
        let transfer_amount = 100.into();
        let mut total_transfer_amount = U256::zero();
        for _ in 0..3 {
            let transfer = Transfer::faucet(recipient, transfer_amount);
            faucet.request_transfer(transfer).await;
            total_transfer_amount += transfer_amount;
        }

        let futures = async move {
            futures::join!(faucet.execute_transfers(), faucet.monitor_transactions())
        };
        let _handle = spawn(futures);

        loop {
            let balance = provider.get_balance(recipient, None).await.unwrap();
            tracing::info!("Balance is {balance}");
            if balance == total_transfer_amount {
                break;
            }
            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }

    #[async_std::test]
    async fn test_faucet_zkevm_node() {
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
        let provider = Provider::<Ws>::connect(ws_url)
            .await
            .expect("Unable to make websocket connection to JsonRPC");

        let options = Options {
            num_clients: 2,
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_grant_amount: 100.into(),
        };
        let chain_id = 1001;
        let faucet = Faucet::new(options, chain_id, provider.clone());
        faucet.start().await.unwrap();

        let recipient = Address::random();
        let transfer_amount = 100.into();
        let mut total_transfer_amount = U256::zero();
        for _ in 0..2 {
            let transfer = Transfer::faucet(recipient, transfer_amount);
            faucet.request_transfer(transfer).await;
            total_transfer_amount += transfer_amount;
        }

        let futures = async move {
            futures::join!(faucet.execute_transfers(), faucet.monitor_transactions())
        };
        let _handle = spawn(futures);

        loop {
            let balance = provider.get_balance(recipient, None).await.unwrap();
            tracing::info!("Balance is {balance}");
            if balance == total_transfer_amount {
                break;
            }
            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }
}
