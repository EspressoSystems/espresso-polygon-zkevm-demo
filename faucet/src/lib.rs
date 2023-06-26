use async_std::{channel::Receiver, sync::RwLock};
use ethers::{
    prelude::SignerMiddleware,
    providers::{Http, Middleware as _, Provider, StreamExt, Ws},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::{Address, TransactionRequest, H256, U256},
};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

pub type Middleware = SignerMiddleware<Provider<Ws>, LocalWallet>;

#[derive(Debug, Clone)]
struct FaucetRequest;

#[derive(Debug, Clone)]
struct Client;

#[derive(Debug, Clone, Copy)]
struct RequestedTransfer {
    amount: U256,
    to: Address,
}

#[derive(Debug, Clone)]
struct PendingTransfer(RequestedTransfer);

#[derive(Debug, Clone, Default)]
struct State {
    available_clients: Vec<Arc<Middleware>>,
    unfunded_clients: Vec<Arc<Middleware>>,
    faucet_request: Vec<FaucetRequest>,
    pending_clients: HashMap<Address, Arc<Middleware>>,
    pending_transfers: HashMap<H256, PendingTransfer>,
    // Funding wallets has priority, these transfer requests must be pushed to
    // the front.
    requested_transfers: VecDeque<RequestedTransfer>,
    // num_pending_client_funding_transfers: usize, // not needed?
}

// Client state
// 1. available, funded
// 2. inflight, funded
// 3. available, unfunded
// 4. inflight, unfunded

// Loop that funds clients
// Loop that handles faucet requests
//
// State transisions
// 1. Faucet request -> new transfer
// 2. Transaction receipt -> transfer done
// 3. Transfer done -> client funded
// 4. Transfer done -> faucet request completed
// 5. Client funded -> client available

#[derive(Debug, Clone)]
struct Options {
    num_clients: usize,
    mnemonic: String,
    faucet_amount: U256,
    funding_amount: U256,
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
            state.requested_transfers.push_front(RequestedTransfer {
                amount: self.config.funding_amount,
                to: receiver.address(),
            });
            state.pending_clients.insert(receiver.address(), receiver);
        }
    }

    async fn request_transfer(&self, transfer: RequestedTransfer) {
        self.state
            .write()
            .await
            .requested_transfers
            .push_back(transfer);
    }

    async fn execute_transfers(&self) {
        // Create transfer requests for available clients.
        loop {
            // TODO: This may keep the lock for too long!
            let mut state = self.state.write().await;
            while let (Some(sender), Some(transfer)) = (
                state.available_clients.pop(),
                state.requested_transfers.pop_front(),
            ) {
                tracing::info!("Sending transfer: {:?}", transfer);
                let tx = sender
                    .send_transaction(
                        TransactionRequest::pay(transfer.to, self.config.faucet_amount),
                        None,
                    )
                    .await
                    .expect("failed to send transaction"); // TODO handle this error
                state
                    .pending_clients
                    .insert(sender.address(), sender.clone());
                state
                    .pending_transfers
                    .insert(tx.tx_hash(), PendingTransfer(transfer));
            }
            // Wait a bit to avoid a busy loop.
            async_std::task::sleep(Duration::from_secs(1)).await;
        }
    }

    // async fn clients(&self) -> Vec<Clients> {
    // }

    async fn monitor_transactions(&self) {
        let mut stream = self
            .provider
            .subscribe_blocks()
            .await
            .unwrap()
            .flat_map(|block| futures::stream::iter(block.transactions));

        while let Some(tx_hash) = stream.next().await {
            tracing::info!("Got tx hash: {:?}", tx_hash);
            tracing::info!("Self {:?}", self);
            // TODO it's inefficient to query every transaction receipt.
            let tx = self.provider.get_transaction_receipt(tx_hash).await;
            if let Ok(Some(tx)) = tx {
                // This transaction is no longer pending
                let pending_transfer = self.state.write().await.pending_transfers.remove(&tx_hash);

                // If the transaction succeeded, update the state. Note that
                // it's possible that the transaction is an external
                // transfer that we are not tracking but may still involve
                // our accounts.
                if tx.status == Some(1.into()) {
                    if let Some(recipient) = tx.to {
                        // Update the state for the parties involved in the transfer.
                        for address in [tx.from, recipient] {
                            if self
                                .state
                                .read()
                                .await
                                .pending_clients
                                .contains_key(&address)
                            {
                                let balance =
                                    self.provider.get_balance(address, None).await.unwrap(); // TODO: error handling
                                if balance >= self.config.faucet_amount {
                                    let mut state = self.state.write().await;
                                    if let Some(client) = state.pending_clients.remove(&address) {
                                        tracing::info!(
                                            "Client funded: {:?} balance={:?}",
                                            client.address(),
                                            balance,
                                        );
                                        state.available_clients.push(client);
                                    }
                                }
                            }
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

                    // TODO: consider using an enum for the two types of
                    // transfers we are making.
                    if transfer.0.amount == self.config.funding_amount {
                        self.state
                            .write()
                            .await
                            .requested_transfers
                            .push_front(transfer.0);
                    } else {
                        self.state
                            .write()
                            .await
                            .requested_transfers
                            .push_back(transfer.0);
                    }
                }
            }
        }
    }
}

// Tests
#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
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
            num_clients: 2,
            mnemonic: TEST_MNEMONIC.to_string(),
            faucet_amount: 100.into(),
            funding_amount: 1000.into(),
        };
        let chain_id = 31337;
        let faucet = Faucet::new(options, chain_id, provider);
        faucet.start().await;

        for _ in 0..10 {
            let transfer = RequestedTransfer {
                amount: 100.into(),
                to: Address::zero(),
            };
            faucet.request_transfer(transfer).await;
        }

        futures::join!(faucet.execute_transfers(), faucet.monitor_transactions());
    }
}
