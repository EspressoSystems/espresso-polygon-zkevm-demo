use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use ethers::{
    abi::Address,
    providers::Middleware as _,
    types::{TransactionRequest, U256},
};
use rand::{distributions::Standard, prelude::Distribution, Rng};
use sequencer_utils::Middleware;
use serde::{Deserialize, Serialize};

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
pub enum Action {
    Transfers(Vec<Transfer>),
}

impl Distribution<Action> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Action {
        match rng.gen_range(0..1) {
            0 => {
                let num_transfers = rng.gen_range(0..10);
                Action::Transfers((0..num_transfers).map(|_| rng.gen()).collect())
            }
            _ => unreachable!(),
        }
    }
}

impl Action {
    /// Executes the actions.
    ///
    /// Currently this function waits for transaction receipts (up to a timeout)
    /// before it returns. For more realistic concurrent load testing it may be
    /// beneficial to instead store a handle, or the transaction hash for each
    /// submitted transaction and await them somewhere else.
    async fn execute(&self, client: Arc<Middleware>) {
        match self {
            Action::Transfers(transfers) => {
                // Create and submit transactions.
                let mut txs_pending = HashSet::new();
                for Transfer { to, amount } in transfers {
                    let tx = TransactionRequest {
                        from: Some(client.inner().address()),
                        to: Some((*to).into()),
                        value: Some(*amount),
                        ..Default::default()
                    };
                    tracing::info!("Transfer {} to {:?}", amount, to);
                    let tx_hash = client.send_transaction(tx, None).await.unwrap().tx_hash();
                    txs_pending.insert(tx_hash);
                }

                // Poll for transaction receipts.
                let mut txs_received = HashSet::new();
                let now = std::time::Instant::now();
                loop {
                    let mut new_hashes = vec![]; // Work around txs_received being borrowed.
                    for tx_hash in txs_pending.difference(&txs_received) {
                        if let Some(receipt) =
                            client.get_transaction_receipt(*tx_hash).await.unwrap()
                        {
                            tracing::info!("Transaction receipt: {:?}", receipt);
                            new_hashes.push(*tx_hash);
                        }
                    }
                    for tx_hash in new_hashes {
                        txs_received.insert(tx_hash);
                    }
                    // If we have all receipts we are done.
                    if txs_received == txs_pending {
                        tracing::info!("Received all transaction receipts");
                        break;
                    // If we have not received all receipts after the timeout we panic.
                    } else if now.elapsed() > Duration::from_secs(300) {
                        panic!(
                            "Did not receive all transaction receipts, missing {:?}",
                            txs_pending.difference(&txs_received)
                        );
                    }
                    // Wait a second, to then poll again
                    async_std::task::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    pub actions: Vec<Action>,
}

impl Run {
    pub async fn run(&self, client: Arc<Middleware>) {
        for action in &self.actions {
            action.execute(client.clone()).await;
        }
    }

    pub fn generate(num_transfer_blocks: usize) -> Self {
        let mut rng = rand::thread_rng();
        let actions = (0..num_transfer_blocks)
            .map(|_| rng.gen())
            .collect::<Vec<_>>();
        Self { actions }
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_run() {
        let num_transfer_blocks = 10;
        let run = Run::generate(num_transfer_blocks);
        assert_eq!(run.actions.len(), num_transfer_blocks);

        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("run.json");
        run.save(&path);
        assert_eq!(Run::load(&path), run);
    }
}
