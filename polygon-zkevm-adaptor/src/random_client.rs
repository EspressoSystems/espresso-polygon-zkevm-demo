// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(any(test, feature = "testing"))]
use async_std::sync::RwLock;
use ethers::{
    abi::Address,
    prelude::{MnemonicBuilder, NonceManagerMiddleware, Signer, SignerMiddleware},
    providers::{Http, Middleware as _, Provider},
    signers::{coins_bip39::English, LocalWallet},
    types::{TransactionRequest, H256, U256},
};
use futures::future::join;
use http_types::Url;
use rand::{distributions::Standard, prelude::Distribution, Rng};

use sequencer_utils::Middleware;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

pub async fn connect_rpc_simple(
    provider: &Url,
    mnemonic: &str,
    index: u32,
    chain_id: Option<u64>,
) -> Option<InnerMiddleware> {
    let provider = match Provider::try_from(provider.to_string()) {
        Ok(provider) => provider,
        Err(err) => {
            tracing::error!("error connecting to RPC {}: {}", provider, err);
            return None;
        }
    };
    let chain_id = match chain_id {
        Some(id) => id,
        None => match provider.get_chainid().await {
            Ok(id) => id.as_u64(),
            Err(err) => {
                tracing::error!("error getting chain ID: {}", err);
                return None;
            }
        },
    };
    let mnemonic = match MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .index(index)
    {
        Ok(mnemonic) => mnemonic,
        Err(err) => {
            tracing::error!("error building wallet: {}", err);
            return None;
        }
    };
    let wallet = match mnemonic.build() {
        Ok(wallet) => wallet,
        Err(err) => {
            tracing::error!("error opening wallet: {}", err);
            return None;
        }
    };
    let wallet = wallet.with_chain_id(chain_id);
    Some(SignerMiddleware::new(provider, wallet))
}

pub type InnerMiddleware = SignerMiddleware<Provider<Http>, LocalWallet>;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub to: Address,
    pub amount: U256,
}

impl Distribution<Transfer> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Transfer {
        Transfer {
            to: rng.gen(),
            amount: rng.gen_range(0..1000).into(),
        }
    }
}

/// Currently only batches of transfers are supported. This is currently enough
/// to cause the zkvem-node to sometimes run into problems.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    Transfer(Transfer),
    Wait(Duration),
}

impl Distribution<Operation> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Operation {
        match rng.gen_range(0..2) {
            0 => Operation::Transfer(rng.gen()),
            1 => Operation::Wait(Duration::from_millis(rng.gen_range(0..10000))),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    PendingReceipt {
        transfer: Transfer,
        hash: H256,
        start: Instant,
    },
}

impl Operation {
    async fn execute(&self, client: Arc<Middleware>) -> Option<Effect> {
        match self {
            Operation::Transfer(transfer) => {
                let Transfer { to, amount } = transfer;
                let tx = TransactionRequest {
                    from: Some(client.inner().address()),
                    to: Some((*to).into()),
                    value: Some(*amount),
                    ..Default::default()
                };
                let hash = client.send_transaction(tx, None).await.unwrap().tx_hash();
                tracing::info!("Submitted transaction: {:?}", hash);
                Some(Effect::PendingReceipt {
                    transfer: transfer.clone(),
                    hash,
                    start: Instant::now(),
                })
            }
            Operation::Wait(duration) => {
                async_std::task::sleep(*duration).await;
                tracing::info!("Finished sleep of {:?}", duration);
                None
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operations(pub(crate) Vec<Operation>);

impl Operations {
    pub fn generate(total_duration: Duration) -> Self {
        let mut rng = rand::thread_rng();
        let mut wait_time = Duration::from_secs(0);
        let mut operations = vec![];
        loop {
            let operation: Operation = rng.gen();
            if let Operation::Wait(duration) = operation {
                wait_time += duration;
            }
            operations.push(operation);
            if wait_time > total_duration {
                break;
            }
        }
        Self(operations)
    }

    pub fn save(&self, path: &PathBuf) {
        let data = serde_json::to_string_pretty(&self.0).unwrap();
        std::fs::write(path, data).unwrap();
    }

    pub fn load(path: &PathBuf) -> Self {
        let data = std::fs::read_to_string(path).unwrap();
        let operations = serde_json::from_str(&data).unwrap();
        Self(operations)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedOperations {
    pub regular_node: Operations,
    pub preconf_node: Operations,
}

impl CombinedOperations {
    pub fn generate(total_duration: Duration) -> Self {
        Self {
            regular_node: Operations::generate(total_duration),
            preconf_node: Operations::generate(total_duration),
        }
    }

    pub fn save(&self, path: &PathBuf) {
        let data = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(path, data).unwrap();
    }

    pub fn load(path: &PathBuf) -> Self {
        let data = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&data).unwrap()
    }
}

#[derive(Debug, Clone)]
struct State {
    pending: VecDeque<Effect>,
    submit_operations_done: bool,
    client: Arc<Middleware>,
}

#[derive(Debug, Clone)]
pub struct Run {
    name: String,
    operations: Operations,
    // The signer is used to re-initialize the nonce manager when necessary.
    signer: SignerMiddleware<Provider<Http>, LocalWallet>,
    state: Arc<RwLock<State>>,
}

impl Run {
    pub fn new(
        name: impl Into<String>,
        operations: Operations,
        signer: SignerMiddleware<Provider<Http>, LocalWallet>,
    ) -> Self {
        Self {
            name: name.into(),
            operations,
            signer: signer.clone(),
            state: Arc::new(RwLock::new(State {
                pending: Default::default(),
                submit_operations_done: Default::default(),
                client: Arc::new(NonceManagerMiddleware::new(
                    signer.clone(),
                    signer.address(),
                )),
            })),
        }
    }

    /// Run the test and wait for completion.
    ///
    /// Returns
    /// * Number of transactions submitted
    /// * Number of transactions successful
    pub async fn wait(&self) -> (usize, usize) {
        join(self.submit_operations(), self.wait_for_effects()).await
    }

    pub async fn submit_operations(&self) -> usize {
        let mut submitted = 0;
        for (index, operation) in self.operations.0.iter().enumerate() {
            tracing::info!(
                "[{}] Submitting operation {index: >6} / {}: {operation:?}",
                self.name,
                self.operations.0.len()
            );
            if let Operation::Transfer(_) = operation {
                submitted += 1;
                let effect = operation
                    .execute(self.state.read().await.client.clone())
                    .await;
                if let Some(effect) = effect {
                    self.state.write().await.pending.push_back(effect);
                }
            } else {
                operation
                    .execute(self.state.read().await.client.clone())
                    .await;
            }
        }
        self.state.write().await.submit_operations_done = true;
        tracing::info!(
            "[{}] Submitted all {} operations ({submitted} transactions)",
            self.name,
            self.operations.0.len()
        );
        submitted
    }

    pub async fn wait_for_effects(&self) -> usize {
        let mut received = 0;
        loop {
            tracing::info!(
                "[{}] num_pending_effects={}",
                self.name,
                self.state.read().await.pending.len()
            );
            let effect = { self.state.write().await.pending.pop_front() };
            if let Some(effect) = effect {
                match effect {
                    Effect::PendingReceipt { hash, start, .. } => {
                        if self
                            .state
                            .read()
                            .await
                            .client
                            .get_transaction_receipt(hash)
                            .await
                            .unwrap()
                            .is_some()
                        {
                            tracing::info!(
                                "[{}] hash={hash:?} receive_receipt={:?}",
                                self.name,
                                start.elapsed()
                            );
                            received += 1;
                        } else {
                            tracing::info!(
                                "[{}] hash={hash:?} wait_receipt={:?}",
                                self.name,
                                start.elapsed()
                            );
                            if start.elapsed() > Duration::from_secs(90) {
                                tracing::info!("[{}] hash={hash:?} receipt_timeout", self.name);
                                tracing::info!("[{}] Removing all pending effects", self.name);
                                // Keep a write lock to avoid adding more pending receipts.
                                let mut state = self.state.write().await;
                                while let Some(effect) = state.pending.pop_front() {
                                    tracing::info!("[{}] effect_clear: {effect:?}", self.name);
                                }
                                tracing::info!("[{}] Reinitializing nonce manager", self.name);
                                state.client = Arc::new(NonceManagerMiddleware::new(
                                    self.signer.clone(),
                                    self.signer.address(),
                                ));
                            } else {
                                self.state.write().await.pending.push_back(effect);
                                // No receipt for this transaction yet, wait a bit.
                                async_std::task::sleep(Duration::from_millis(1000)).await;
                            }
                        }
                    }
                }
            } else {
                // There are no pending effects, wait a bit.
                async_std::task::sleep(Duration::from_secs(5)).await;
            }
            let state = self.state.read().await;
            if state.submit_operations_done && state.pending.is_empty() {
                tracing::info!(
                    "[{}] All effects completed ({received} successful)!",
                    self.name
                );
                break;
            }
        }

        received
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_ops_serialization() {
        let ops = Operations::generate(Duration::from_secs(100));
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("run.json");
        ops.save(&path);
        assert_eq!(Operations::load(&path), ops);
    }
}
