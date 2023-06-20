use async_std::sync::RwLock;
use ethers::{
    abi::Address,
    prelude::{NonceManagerMiddleware, SignerMiddleware},
    providers::{Http, Middleware as _, Provider},
    signers::LocalWallet,
    types::{TransactionRequest, H256, U256},
};
use rand::{distributions::Standard, prelude::Distribution, Rng};

use sequencer_utils::Middleware;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

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

#[derive(Debug, Clone)]
pub struct Run {
    operations: Operations,
    // New effects to await *must* be pushed to the back of this queueimplim    pub pending: Arc<RwLock<VecDeque<Effect>>>,
    pending: Arc<RwLock<VecDeque<Effect>>>,
    submit_operations_done: Arc<RwLock<bool>>,
    signer: SignerMiddleware<Provider<Http>, LocalWallet>,
    client: Arc<RwLock<Arc<Middleware>>>,
}

impl Run {
    pub fn new(
        operations: Operations,
        signer: SignerMiddleware<Provider<Http>, LocalWallet>,
    ) -> Self {
        Self {
            operations,
            pending: Default::default(),
            submit_operations_done: Default::default(),
            signer: signer.clone(),
            client: Arc::new(RwLock::new(Arc::new(NonceManagerMiddleware::new(
                signer.clone(),
                signer.address(),
            )))),
        }
    }

    pub async fn submit_operations(&self) {
        for (index, operation) in self.operations.0.iter().enumerate() {
            tracing::info!(
                "Submitting operation {index: >6} / {}: {operation:?}",
                self.operations.0.len()
            );
            // Get a lock before executing the operation to avoid adding pending
            // receipts in case we are resetting the nonce manager.
            if let Operation::Transfer(_) = operation {
                let mut pending = self.pending.write().await;
                let effect = operation.execute(self.client.read().await.clone()).await;
                if let Some(effect) = effect {
                    pending.push_back(effect);
                }
            } else {
                operation.execute(self.client.read().await.clone()).await;
            }
        }
        *self.submit_operations_done.write().await = true;
        tracing::info!("Submitted all {} operations", self.operations.0.len());
    }

    async fn reinit_nonce_manager(&self) {
        tracing::info!("Reinitializing nonce manager");
        *self.client.write().await = Arc::new(NonceManagerMiddleware::new(
            self.signer.clone(),
            self.signer.address(),
        ));
    }

    pub async fn wait_for_effects(&self) {
        loop {
            tracing::info!("num_pending_effects={}", self.pending.read().await.len());
            let effect = { self.pending.write().await.pop_front() };
            if let Some(effect) = effect {
                match effect {
                    Effect::PendingReceipt { hash, start, .. } => {
                        if self
                            .client
                            .read()
                            .await
                            .get_transaction_receipt(hash)
                            .await
                            .unwrap()
                            .is_some()
                        {
                            tracing::info!("hash={hash:?} receive_receipt={:?}", start.elapsed());
                        } else {
                            tracing::info!("hash={hash:?} wait_receipt={:?}", start.elapsed());
                            if start.elapsed() > Duration::from_secs(90) {
                                tracing::info!("hash={hash:?} receipt_timeout");
                                tracing::info!("Removing all pending effects");
                                // Keep a write lock to avoid adding more pending receipts.
                                let mut pending = self.pending.write().await;
                                while let Some(effect) = pending.pop_front() {
                                    tracing::info!("effect_clear: {effect:?}");
                                }
                                self.reinit_nonce_manager().await;
                            } else {
                                self.pending.write().await.push_back(effect);
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
            if *self.submit_operations_done.read().await && self.pending.read().await.is_empty() {
                tracing::info!("All effects completed!");
                break;
            }
        }
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
