use async_std::sync::RwLock;
use ethers::{
    abi::Address,
    providers::Middleware as _,
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

#[derive(Clone, Debug, Default)]
pub struct Run {
    pub operations: Vec<Operation>,
    // New effects to await *must* be pushed to the back of this queue.
    pub pending: Arc<RwLock<VecDeque<Effect>>>,
    submit_operations_done: Arc<RwLock<bool>>,
}

impl Run {
    pub async fn submit_operations(&self, client: Arc<Middleware>) {
        for (index, operation) in self.operations.iter().enumerate() {
            tracing::info!(
                "Submitting operation {index: >6} / {}: {operation:?}",
                self.operations.len()
            );
            let effect = operation.execute(client.clone()).await;
            if let Some(effect) = effect {
                self.pending.write().await.push_back(effect);
            }
        }
        *self.submit_operations_done.write().await = true;
        tracing::info!("Submitted all {} operations", self.operations.len());
    }

    pub async fn wait_for_effects(&self, client: Arc<Middleware>) {
        // Currently this loop assumes the effects will resolve in order to
        // reduce the number of requests it needs to make. If effects are
        // expected to resolve out of order, we should change this.
        loop {
            tracing::info!(
                "Number of pending effects: {}",
                self.pending.read().await.len()
            );
            let effect = { self.pending.write().await.pop_front() };
            if let Some(effect) = effect {
                match effect {
                    Effect::PendingReceipt { hash, start, .. } => {
                        if client
                            .get_transaction_receipt(hash)
                            .await
                            .unwrap()
                            .is_some()
                        {
                            tracing::info!(
                                "hash={hash:?} got receipt after: {:?}",
                                start.elapsed()
                            );
                        } else {
                            tracing::info!("hash={hash:?} no receipt after {:?}", start.elapsed());
                            self.pending.write().await.push_front(effect);
                            // No receipt for this transaction yet, wait a bit.
                            async_std::task::sleep(Duration::from_secs(5)).await;
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
        Self {
            operations,
            ..Default::default()
        }
    }

    pub fn save(&self, path: &PathBuf) {
        let data = serde_json::to_string_pretty(&self.operations).unwrap();
        std::fs::write(path, data).unwrap();
    }

    pub fn load(path: &PathBuf) -> Self {
        let data = std::fs::read_to_string(path).unwrap();
        let operations = serde_json::from_str(&data).unwrap();
        Self {
            operations,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_serialization() {
        let run = Run::generate(Duration::from_secs(100));
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("run.json");
        run.save(&path);
        assert_eq!(Run::load(&path).operations, run.operations);
    }
}
