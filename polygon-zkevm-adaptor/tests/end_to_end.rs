// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::{sync::Arc, task::sleep};
use ethers::{prelude::*, providers::Middleware};
use futures::stream::{StreamExt, TryStream, TryStreamExt};
use hotshot_query_service::availability::BlockQueryData;
use polygon_zkevm_adaptor::{Layer1Backend, SequencerZkEvmDemo, SequencerZkEvmDemoOptions};
use sequencer::SeqTypes;
use sequencer_utils::{init_signer, wait_for_http, NonceManager};
use std::fmt::Debug;
use std::time::{Duration, Instant};
use zkevm::ZkEvm;
use zkevm_contract_bindings::PolygonZkEVM;

#[cfg(feature = "slow-tests")]
struct ReorgMe {
    ports: [u16; 3],
}

#[cfg(feature = "slow-tests")]
impl ReorgMe {
    fn start() -> Self {
        let reorgme = Self {
            ports: [0; 3].map(|_| portpicker::pick_unused_port().unwrap()),
        };
        let mut command = reorgme.cmd("start");
        command.args([
            "--chain-id",
            "1337",
            "--allocation",
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266=1000000000000000000000000000",
            "--allocation",
            "0x70997970C51812dc3A010C7d01b50e0d17dc79C8=1000000000000000000000000000",
        ]);
        if !command.spawn().unwrap().wait().unwrap().success() {
            panic!("failed to start reorgme testnet");
        }
        reorgme
    }

    fn rpc_port(&self) -> u16 {
        self.ports[0]
    }

    fn fork(&self) {
        if !self.cmd("fork").spawn().unwrap().wait().unwrap().success() {
            panic!("failed to fork L1 node from reorgme testnet");
        }
    }

    fn join(&self) {
        if !self.cmd("join").spawn().unwrap().wait().unwrap().success() {
            panic!("failed to rejoin L1 node to reorgme testnet");
        }
    }

    fn cmd(&self, command: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("npx");
        cmd.args(["reorgme", command]);
        for port in self.ports {
            cmd.args(["--rpc-port", &port.to_string()]);
        }
        cmd
    }
}

#[cfg(feature = "slow-tests")]
impl Drop for ReorgMe {
    fn drop(&mut self) {
        if !self.cmd("stop").spawn().unwrap().wait().unwrap().success() {
            tracing::error!("failed to stop reorgme");
        }
    }
}

#[async_std::test]
async fn test_end_to_end() {
    let node = setup_test("test-end-to-end", Duration::from_secs(1)).await;

    // Get test setup from environment.
    let env = node.env();
    let l1_provider = env.l1_provider();
    let l2_provider = env.l2_provider();
    let mnemonic = env.funded_mnemonic();
    let rollup_address = node.l1().rollup.address();

    let l1 = Arc::new(init_signer(&l1_provider, mnemonic, 0).await.unwrap());
    let l2_signer = init_signer(&l2_provider, mnemonic, 0).await.unwrap();
    let l2_addr = l2_signer.address();
    let l2 = Arc::new(NonceManager::new(l2_signer, l2_addr));
    let zkevm = ZkEvm {
        chain_id: l2.get_chainid().await.unwrap().as_u64(),
    };
    let rollup = PolygonZkEVM::new(rollup_address, l1.clone());
    let l1_initial_block = l1.get_block_number().await.unwrap();
    let l2_initial_balance = l2.get_balance(l2_addr, None).await.unwrap();

    // Subscribe to a block stream so we can find the blocks that end up including our transactions.
    tracing::info!("connecting to sequencer at {}", env.sequencer());
    let sequencer = surf_disco::Client::<hotshot_query_service::Error>::new(env.sequencer());
    sequencer.connect(None).await;
    let mut blocks = sequencer
        .socket("availability/stream/blocks/0")
        .subscribe::<BlockQueryData<SeqTypes>>()
        .await
        .unwrap();

    // Wait for the adaptor to start serving.
    tracing::info!("connecting to adaptor RPC at {}", env.l2_adaptor_rpc());
    // The adaptor is not a full RPC, therefore we can't use `wait_for_rpc`.`
    wait_for_http(&env.l2_adaptor_rpc(), Duration::from_secs(1), 100)
        .await
        .unwrap();
    tracing::info!(
        "connecting to adaptor query service at {}",
        env.l2_adaptor_query()
    );
    wait_for_http(&env.l2_adaptor_query(), Duration::from_secs(1), 100)
        .await
        .unwrap();

    // Send a malformed transaction to test that the system can handle it
    // gracefully and remains operational.
    let malformed_tx_payload = b"\xde\xad\xbe\xef";
    let malformed_tx_hash = l2
        .send_raw_transaction(malformed_tx_payload.into())
        .await
        .unwrap()
        .tx_hash();
    tracing::info!("malformed transaction hash: {:?}", malformed_tx_hash);

    // Wait for the malformed transaction to be included in a block.
    'block: loop {
        let block = blocks.next().await.unwrap().unwrap();
        tracing::info!("got block {:?}", block);
        for (_, txn) in block.enumerate() {
            if txn.payload() == malformed_tx_payload {
                tracing::info!("malformed transaction sequenced");
                break 'block;
            } else {
                tracing::warn!("unknown transaction sequenced: {txn:?}");
            }
        }
    }

    // Create a few test transactions.
    let transfer_amount = 1.into();
    let num_txns = 2u64;
    let mut txn_hashes = vec![];
    let mut block_nums = vec![];
    for i in 0..num_txns {
        let hash = l2
            .send_transaction(
                TransactionRequest {
                    from: Some(l2_addr),
                    to: Some(Address::zero().into()),
                    value: Some(transfer_amount),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap()
            .tx_hash();
        tracing::info!("Transaction {}: {:?}", i, hash);

        // Wait for the transaction to be included in a block. We must ensure this transaction is
        // sequenced before the next one, or both could be invalidated due to nonce misordering.
        let block_num = wait_for_block_containing_txn(&mut blocks, zkevm, hash).await;

        txn_hashes.push(hash);
        block_nums.push(block_num);
    }

    // Wait for the transactions to complete on L2.
    for hash in txn_hashes {
        await_transaction(&l2, hash).await;
    }

    // Check the effects of the transfers.
    assert_eq!(
        l2.get_balance(l2_addr, None).await.unwrap(),
        l2_initial_balance - U256::from(num_txns) * transfer_amount
    );

    // Check that blocks have been sequenced on L1 up to at least the block that included our most
    // recent transaction. The inequality is strict because batch numbers on L1 are 1-indexed but
    // HotShot block numbers are 0-indexed.
    let last_block = *block_nums.last().unwrap();

    // Wait for the batches to be verified.
    let verified_filter = rollup
        .verify_batches_trusted_aggregator_filter()
        .from_block(l1_initial_block);
    let mut events = verified_filter.stream().await.unwrap();
    loop {
        let event = events.next().await.unwrap().unwrap();
        tracing::info!("batches verified up to {}/{}", event.num_batch, last_block);
        if event.num_batch > last_block {
            break;
        }
    }

    // Check that the malformed transaction is not present by the zkevm-node.
    assert!(l2
        .get_transaction_receipt(malformed_tx_hash)
        .await
        .unwrap()
        .is_none());
}

#[cfg(feature = "slow-tests")]
#[async_std::test]
async fn test_preconfirmations() {
    setup_logging();
    setup_backtrace();

    let node = setup_test("test-preconfirmations", Duration::from_secs(10)).await;
    let env = node.env();
    let mnemonic = env.funded_mnemonic();
    let l2 = init_signer(&env.l2_provider(), mnemonic, 0).await.unwrap();
    let l2_preconf = init_signer(&env.l2_preconfirmations_provider(), mnemonic, 0)
        .await
        .unwrap();
    let zkevm = ZkEvm {
        chain_id: l2.get_chainid().await.unwrap().as_u64(),
    };
    let l2_initial_balance = l2.get_balance(l2.address(), None).await.unwrap();

    // Subscribe to a block stream so we can find the block that ends up including our transaction.
    tracing::info!("connecting to sequencer at {}", env.sequencer());
    let sequencer = surf_disco::Client::<hotshot_query_service::Error>::new(env.sequencer());
    sequencer.connect(None).await;
    let mut blocks = sequencer
        .socket("availability/stream/blocks/0")
        .subscribe::<BlockQueryData<SeqTypes>>()
        .await
        .unwrap();

    // Wait for the adaptor to start serving.
    tracing::info!("connecting to adaptor RPC at {}", env.l2_adaptor_rpc());
    // The adaptor is not a full RPC, therefore we can't use `wait_for_rpc`.`
    wait_for_http(&env.l2_adaptor_rpc(), Duration::from_secs(1), 100)
        .await
        .unwrap();
    tracing::info!(
        "connecting to adaptor query service at {}",
        env.l2_adaptor_query()
    );
    wait_for_http(&env.l2_adaptor_query(), Duration::from_secs(1), 100)
        .await
        .unwrap();

    // Create a test transaction.
    let transfer_amount = 1.into();
    let txn_hash = l2
        .send_transaction(
            TransactionRequest {
                from: Some(l2.address()),
                to: Some(Address::zero().into()),
                value: Some(transfer_amount),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
        .tx_hash();
    let submitted = Instant::now();
    tracing::info!("Sent transaction {txn_hash:?} at {submitted:?}");

    // Wait for the transaction to be included in a block.
    wait_for_block_containing_txn(&mut blocks, zkevm, txn_hash).await;
    tracing::info!(
        "Transaction sequenced after {:?}",
        Instant::now() - submitted
    );

    // Wait for the transaction to complete on L2, using both the regular RPC and the
    // preconfirmation RPC in parallel.
    let (pre_conf, slow_conf) = futures::join!(
        await_transaction(&l2_preconf, txn_hash),
        await_transaction(&l2, txn_hash),
    );

    // Check the effects of the transfer.
    assert_eq!(
        l2.get_balance(l2.address(), None).await.unwrap(),
        l2_initial_balance - transfer_amount
    );
    assert_eq!(
        l2_preconf
            .get_balance(l2_preconf.address(), None)
            .await
            .unwrap(),
        l2_initial_balance - transfer_amount
    );

    // Check that we got the preconfirmation first.
    tracing::info!(
        "preconfirmation received after {:?}, final confirmation received after {:?} ({:?} difference)",
        pre_conf - submitted, slow_conf - submitted, slow_conf - pre_conf
    );
    let ok = pre_conf < slow_conf;
    if std::env::var("ESPRESSO_DISABLE_TIMING_BASED_TESTS_FOR_CI").unwrap_or_default() == "true" {
        // This test passes consistently on a sufficiently powerful machine, but fails often in CI
        // due to the advantage of the preconfirmations node being drowned out by scheduling noise
        // on the smaller, heavily loaded CI runners. Don't fail the workflow for it, just print a
        // warning.
        if !ok {
            tracing::error!("preconfirmation was slower than final confirmation");
            tracing::warn!(
                "not failing test because ESPRESSO_DISABLE_TIMING_BASED_TESTS_FOR_CI was set"
            );
        }
    } else {
        assert!(ok);
    }

    // Check that both nodes have consistent state roots, for the latest block that both have in
    // common.
    let preconf_height = l2_preconf.get_block_number().await.unwrap().as_u64();
    let regular_height = l2.get_block_number().await.unwrap().as_u64();
    let height = std::cmp::min(preconf_height, regular_height);
    let preconf_state = l2_preconf
        .get_block(height)
        .await
        .unwrap()
        .unwrap()
        .state_root;
    let regular_state = l2.get_block(height).await.unwrap().unwrap().state_root;
    assert_eq!(preconf_state, regular_state);
}

#[cfg(feature = "slow-tests")]
#[async_std::test]
async fn test_reorg() {
    setup_logging();
    setup_backtrace();

    let reorgme = ReorgMe::start();
    let node = setup_test_with_host_l1("test-reorg", reorgme.rpc_port()).await;

    tracing::info!("Separating L1 node from the network");
    reorgme.fork();

    let env = node.env();
    let mnemonic = env.funded_mnemonic();
    let rollup_address = node.l1().rollup.address();

    let l1 = Arc::new(init_signer(&env.l1_provider(), mnemonic, 0).await.unwrap());
    let l2 = Arc::new(init_signer(&env.l2_provider(), mnemonic, 0).await.unwrap());
    let l2_preconf = init_signer(&env.l2_preconfirmations_provider(), mnemonic, 0)
        .await
        .unwrap();
    let l2_initial_balance = l2.get_balance(l2.address(), None).await.unwrap();
    let rollup = PolygonZkEVM::new(rollup_address, l1.clone());

    // Wait for the sequencer API to start before we try submitting transactions.
    tracing::info!("connecting to sequencer at {}", env.sequencer());
    let sequencer = surf_disco::Client::<hotshot_query_service::Error>::new(env.sequencer());
    sequencer.connect(None).await;

    // Wait for the adaptor to start serving.
    tracing::info!("connecting to adaptor RPC at {}", env.l2_adaptor_rpc());
    // The adaptor is not a full RPC, therefore we can't use `wait_for_rpc`.`
    wait_for_http(&env.l2_adaptor_rpc(), Duration::from_secs(1), 100)
        .await
        .unwrap();
    tracing::info!(
        "connecting to adaptor query service at {}",
        env.l2_adaptor_query()
    );
    wait_for_http(&env.l2_adaptor_query(), Duration::from_secs(1), 100)
        .await
        .unwrap();

    // Create a test transaction.
    let transfer_amount = 1.into();
    let txn_hash = l2
        .send_transaction(
            TransactionRequest {
                from: Some(l2.address()),
                to: Some(Address::zero().into()),
                value: Some(transfer_amount),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
        .tx_hash();
    let submitted = Instant::now();
    tracing::info!("Sent transaction {txn_hash:?} at {submitted:?}");

    // Wait for the transaction to complete on L2, using both the regular RPC and the
    // preconfirmation RPC in parallel.
    futures::join!(
        await_transaction(&l2_preconf, txn_hash),
        await_transaction(&l2, txn_hash),
    );

    // Check the effects of the transfer.
    assert_eq!(
        l2.get_balance(l2.address(), None).await.unwrap(),
        l2_initial_balance - transfer_amount
    );
    assert_eq!(
        l2_preconf
            .get_balance(l2_preconf.address(), None)
            .await
            .unwrap(),
        l2_initial_balance - transfer_amount
    );

    // Wait for some batches to be verified. The zkevm node does not updated its latest synced L1
    // block until it reaches a block with relevant events, and for the preconfirmations node, the
    // only relevant events are from verified batches (since it reads sequenced batches from
    // HotShot, not the L1). This means that if the preconfirmations node has not seen any verified
    // batches yet, it has not synced any L1 blocks at all, and the reorg will not affect it.
    let verified_filter = rollup.verify_batches_trusted_aggregator_filter();
    verified_filter.stream().await.unwrap().next().await;

    // Wait a few seconds for the preconfirmations node to handle that event.
    tracing::info!("waiting for batches to be verified");
    sleep(Duration::from_secs(5)).await;

    // Force an L1 reorg.
    tracing::info!("causing L1 reorg");
    reorgme.join();

    // Send another transaction to ensure the nodes are still syncing.
    let txn_hash = l2
        .send_transaction(
            TransactionRequest {
                from: Some(l2.address()),
                to: Some(Address::zero().into()),
                value: Some(transfer_amount),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
        .tx_hash();
    let submitted = Instant::now();
    tracing::info!("Sent transaction {txn_hash:?} at {submitted:?}");

    // Wait for the transaction to complete on L2, using both the regular RPC and the
    // preconfirmation RPC in parallel.
    futures::join!(
        await_transaction(&l2_preconf, txn_hash),
        await_transaction(&l2, txn_hash),
    );

    // Check the effects of the transfer.
    assert_eq!(
        l2.get_balance(l2.address(), None).await.unwrap(),
        l2_initial_balance - transfer_amount * 2
    );
    assert_eq!(
        l2_preconf
            .get_balance(l2_preconf.address(), None)
            .await
            .unwrap(),
        l2_initial_balance - transfer_amount * 2
    );

    // Wait for the verified batches to catch up, to be sure everything is still syncing properly.
    // This forces the test to run long enough, and for the nodes to sync enough verified batches,
    // that it will be clearly visible in the logs if the preconfirmations node is failing to sync
    // verified batches, which is the problem we saw when there was a reorg in production.
    //
    // Note that the test won't necessarily fail if the preconfirmations node is unable to sync
    // verified batches, because syncing these batches doesn't actually affect the observable state
    // of the preconfirmations node: it's state is only affected by blocks that it syncs directly
    // from HotShot, asynchronously with respect to the verified state. But at least we will be able
    // to check the logs for issues if we are actively working on reorg handling.
    let l2_height = sequencer
        .get::<u64>("status/block-height")
        .send()
        .await
        .unwrap();
    let verified_filter = rollup.verify_batches_trusted_aggregator_filter();
    loop {
        tracing::info!("waiting for batch {l2_height} to be verified");
        let event = verified_filter
            .stream()
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap();
        tracing::info!("current verified batch is {}", event.num_batch);
        if event.num_batch >= l2_height {
            break;
        }
    }
}

async fn wait_for_block_containing_txn<B>(mut blocks: B, zkevm: ZkEvm, hash: H256) -> u64
where
    B: TryStream<Ok = BlockQueryData<SeqTypes>> + Unpin,
    B::Error: Debug,
{
    loop {
        let block = blocks.try_next().await.unwrap().unwrap();
        tracing::info!("got block {:?}", block);
        for txn in zkevm.vm_transactions(block.payload()) {
            let sequenced_hash = txn.hash();
            if sequenced_hash == hash {
                tracing::info!("transaction {hash} sequenced");
                return block.height();
            } else {
                tracing::info!("unknown transaction {sequenced_hash} sequenced");
            }
        }
    }
}

async fn await_transaction(rpc: &impl Middleware, hash: H256) -> Instant {
    // Note that awaiting a [PendingTransaction] will not work here -- [PendingTransaction] returns
    // [None] if the transaction is thrown out of the mempool, but since we bypassed the sequencer,
    // our transactions were never in the mempool in the first place.
    loop {
        if let Some(receipt) = rpc.get_transaction_receipt(hash).await.unwrap() {
            tracing::info!("transfer {hash} completed: {receipt:?}");
            break;
        }
        tracing::info!("Waiting for transfer {hash} to complete");
        sleep(Duration::from_secs(1)).await;
    }
    Instant::now()
}

async fn setup_test(name: &str, l1_block_time: Duration) -> SequencerZkEvmDemo {
    setup_logging();
    setup_backtrace();

    SequencerZkEvmDemoOptions::default()
        .l1_backend(Layer1Backend::Anvil)
        .l1_block_period(l1_block_time)
        .start(name.to_string())
        .await
}

#[cfg(feature = "slow-tests")]
async fn setup_test_with_host_l1(name: &str, l1_port: u16) -> SequencerZkEvmDemo {
    setup_logging();
    setup_backtrace();

    SequencerZkEvmDemoOptions::default()
        .use_host_l1(l1_port)
        .l1_backend(Layer1Backend::Anvil)
        .start(name.to_string())
        .await
}
