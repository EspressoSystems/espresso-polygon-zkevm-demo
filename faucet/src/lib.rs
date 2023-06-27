use async_std::{
    channel::{Receiver, Sender},
    sync::RwLock,
};
use derive_more::From;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Http, Middleware as _, Provider, StreamExt, Ws},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::{Address, TransactionRequest, H256, U256},
};
use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
    sync::Arc,
    time::Duration,
};

pub type Middleware = SignerMiddleware<Provider<Ws>, LocalWallet>;

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
    unfunded_clients: Vec<Arc<Middleware>>,
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
            "State: available={} unfunded={} faucet_requests={} pending_clients={} pending_transfers={} transfer_queue={}",
            self.available_clients.len(),
            self.unfunded_clients.len(),
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
    client_funding_amount: U256,
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
            // TODO: we can check balance to determine what is funded
            tracing::info!("Created client {} {}", index, client.address());
            if index == 0 {
                state.available_clients.push(client);
            } else {
                state.unfunded_clients.push(client);
            }
        }
        Self {
            config: options,
            state: Arc::new(RwLock::new(state)),
            provider,
        }
    }

    pub async fn start(&self) {
        // Fund all unfunded clients
        let mut state = self.state.write().await;
        while let Some(receiver) = state.unfunded_clients.pop() {
            let transfer = Transfer::funding(receiver.address());
            tracing::info!("Adding transfer to queue: {:?}", transfer);
            state.transfer_queue.push_back(transfer);
            state.inflight_clients.insert(receiver.address(), receiver);
        }
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
            // TODO: This may keep the lock for too long!
            let mut clients = vec![];
            let mut transfers = vec![];
            {
                let mut state = self.state.write().await;
                let num_executable =
                    std::cmp::min(state.available_clients.len(), state.transfer_queue.len());
                for _ in 0..num_executable {
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
                tracing::info!("Sending transfer: {:?}", transfer);
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

    // async fn clients(&self) -> Vec<Clients> {
    // }
    // async fn funding_loop(&self) {
    //     loop {
    //         self.state.read().await.available_clients.pop() {
    //             let balance = self
    //                 .provider
    //                 .get_balance(client.address(), None)
    //                 .await
    //                 .unwrap();
    //             if balance < self.config.client_funding_amount {
    //                 tracing::info!("Adding transfer request for {:?}", client.address());
    //                 self.request_transfer(TransferInfo {
    //                     amount: ,
    //                     to: client.address(),
    //                 })
    //                 .await;
    //             }
    //         }

    async fn monitor_transactions(&self) {
        let mut stream = self
            .provider
            .subscribe_blocks()
            .await
            .unwrap()
            .flat_map(|block| futures::stream::iter(block.transactions));

        let mut counter = 0usize;
        self.state.write().await.monitoring_started = true;

        while let Some(tx_hash) = stream.next().await {
            counter += 1;
            tracing::info!("Got tx hash: {} {:?}", counter, tx_hash);
            tracing::info!("State: {}", self.state.read().await);
            // TODO it's inefficient to query every transaction receipt.
            let tx = loop {
                if let Ok(Some(tx)) = self.provider.get_transaction_receipt(tx_hash).await {
                    break tx;
                }
                async_std::task::sleep(Duration::from_secs(1)).await;
            };
            // This transaction is no longer pending
            let pending_transfer = self.state.write().await.inflight_transfers.remove(&tx_hash);
            if let Some(transfer) = pending_transfer {
                tracing::info!("Transfer completed: {:?}", transfer);
            }

            // If the transaction succeeded, update the state. Note that
            // it's possible that the transaction is an external
            // transfer that we are not tracking but may still involve
            // our accounts.
            if tx.status == Some(1.into()) && pending_transfer.is_some() {
                if let Some(recipient) = tx.to {
                    // Update the state for the parties involved in the transfer.
                    for address in [tx.from, recipient] {
                        // if self
                        //     .state
                        //     .read()
                        //     .await
                        //     .inflight_clients
                        //     .contains_key(&address)
                        // {
                        let balance = self.provider.get_balance(address, None).await.unwrap(); // TODO: error handling
                                                                                               // if balance >= self.config.client_funding_amount {
                        let mut state = self.state.write().await;
                        if let Some(client) = state.inflight_clients.remove(&address) {
                            tracing::info!(
                                "Client funded: {:?} balance={:?}",
                                client.address(),
                                balance,
                            );
                            state.available_clients.push(client);
                        }
                        // } else {
                        //     tracing::info!(
                        //         "Client not funded: {:?} balance={:?}",
                        //         address,
                        //         balance,
                        //     );
                        // // }
                        // }
                    }
                }
            // If the transaction failed and is a faucet transaction. We
            // need to send it again.
            } else if let Some(transfer) = pending_transfer {
                tracing::warn!(
                    "Transfer failed tx_hash={:?}, will resend: {:?}",
                    tx_hash,
                    transfer
                );

                self.state.write().await.transfer_queue.push_back(transfer);
            } else {
                tracing::warn!("Ignoring transaction: {:?}", tx);
            }
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
    use sequencer_utils::AnvilOptions;

    const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

    #[async_std::test]
    async fn test_faucet() {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;

        let mut ws_url = anvil.url();
        ws_url.set_scheme("ws").unwrap();
        let provider = Provider::<Ws>::connect(ws_url)
            .await
            .expect("Unable to make websocket connection to L1");

        let options = Options {
            num_clients: 10,
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_grant_amount: 100.into(),
            client_funding_amount: 1000.into(),
        };
        let chain_id = 31337;
        let faucet = Faucet::new(options, chain_id, provider.clone());
        faucet.start().await;

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
}
