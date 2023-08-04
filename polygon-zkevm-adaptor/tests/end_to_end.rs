// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::task::{sleep, spawn};
use ethers::{prelude::*, providers::Middleware};
use futures::{
    future::{ready, FutureExt},
    join,
    stream::{StreamExt, TryStream, TryStreamExt},
};
use hotshot_query_service::availability::BlockQueryData;
use polygon_zkevm_adaptor::{Layer1Backend, ZkEvmNode};
use sequencer::{
    api::{self, HttpOptions, QueryOptions},
    hotshot_commitment::{run_hotshot_commitment_task, CommitmentTaskOptions},
    SeqTypes,
};
use sequencer_utils::{connect_rpc, wait_for_http};
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use zkevm::ZkEvm;
use zkevm_contract_bindings::PolygonZkEVM;

#[async_std::test]
async fn test_end_to_end() {
    let (node, _stores) = setup_test("test-end-to-end", Duration::from_secs(1)).await;

    // Get test setup from environment.
    let env = node.env();
    let l1_provider = env.l1_provider();
    let l2_provider = env.l2_provider();
    let mnemonic = env.funded_mnemonic();
    let rollup_address = node.l1().rollup.address();

    let l1 = connect_rpc(&l1_provider, mnemonic, 0, None).await.unwrap();
    let l2 = connect_rpc(&l2_provider, mnemonic, 0, None).await.unwrap();
    let zkevm = ZkEvm {
        chain_id: l2.get_chainid().await.unwrap().as_u64(),
    };
    let rollup = PolygonZkEVM::new(rollup_address, l1.clone());
    let l1_initial_block = l1.get_block_number().await.unwrap();
    let l2_initial_balance = l2.get_balance(l2.inner().address(), None).await.unwrap();

    // Subscribe to a block stream so we can find the blocks that end up including our transactions.
    tracing::info!("connecting to sequencer at {}", env.sequencer());
    let sequencer = surf_disco::Client::<hotshot_query_service::Error>::new(env.sequencer());
    sequencer.connect(None).await;
    let mut blocks = sequencer
        .socket("availability/stream/blocks/0")
        .subscribe()
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
        let block: BlockQueryData<SeqTypes> = blocks.next().await.unwrap().unwrap();
        tracing::info!("got block {:?}", block);
        if let Some(txn) = block.block().clone().transactions().next() {
            assert_eq!(txn.payload(), malformed_tx_payload);
            tracing::info!("malformed transaction sequenced");
            break 'block;
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
                    from: Some(l2.inner().address()),
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
        l2.get_balance(l2.inner().address(), None).await.unwrap(),
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

#[async_std::test]
async fn test_preconfirmations() {
    setup_logging();
    setup_backtrace();

    let (node, _stores) = setup_test("test-preconfirmations", Duration::from_secs(5)).await;
    let env = node.env();
    let mnemonic = env.funded_mnemonic();
    let l2 = connect_rpc(&env.l2_provider(), mnemonic, 0, None)
        .await
        .unwrap();
    let l2_preconf = connect_rpc(&env.l2_preconfirmations_provider(), mnemonic, 0, None)
        .await
        .unwrap();
    let zkevm = ZkEvm {
        chain_id: l2.get_chainid().await.unwrap().as_u64(),
    };
    let l2_initial_balance = l2.get_balance(l2.inner().address(), None).await.unwrap();

    // Subscribe to a block stream so we can find the block that ends up including our transaction.
    tracing::info!("connecting to sequencer at {}", env.sequencer());
    let sequencer = surf_disco::Client::<hotshot_query_service::Error>::new(env.sequencer());
    sequencer.connect(None).await;
    let mut blocks = sequencer
        .socket("availability/stream/blocks/0")
        .subscribe()
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
                from: Some(l2.inner().address()),
                to: Some(Address::zero().into()),
                value: Some(transfer_amount),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap()
        .tx_hash();
    tracing::info!("Sent transaction {:?}", txn_hash);

    // Wait for the transaction to be included in a block.
    wait_for_block_containing_txn(&mut blocks, zkevm, txn_hash).await;
    tracing::info!("Transaction sequenced at {:?}", Instant::now());

    // Wait for the transaction to complete on L2, using both the regular RPC and the
    // preconfirmation RPC in parallel.
    let (pre_conf, slow_conf) = join!(
        await_transaction(&l2_preconf, txn_hash),
        await_transaction(&l2, txn_hash),
    );

    // Check the effects of the transfer.
    assert_eq!(
        l2.get_balance(l2.inner().address(), None).await.unwrap(),
        l2_initial_balance - transfer_amount
    );
    assert_eq!(
        l2_preconf
            .get_balance(l2_preconf.inner().address(), None)
            .await
            .unwrap(),
        l2_initial_balance - transfer_amount
    );

    // Check that we got the preconfirmation first.
    tracing::info!(
        "preconfirmation received at {pre_conf:?}, final confirmation received at {slow_conf:?} ({:?} difference)",
        slow_conf - pre_conf
    );
    assert!(pre_conf < slow_conf);
}

async fn wait_for_block_containing_txn<B>(mut blocks: B, zkevm: ZkEvm, hash: H256) -> u64
where
    B: TryStream<Ok = BlockQueryData<SeqTypes>> + Unpin,
    B::Error: Debug,
{
    loop {
        let block = blocks.try_next().await.unwrap().unwrap();
        tracing::info!("got block {:?}", block);
        for txn in zkevm.vm_transactions(block.block()) {
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

async fn setup_test(name: &str, l1_block_time: Duration) -> (ZkEvmNode, TempDir) {
    setup_logging();
    setup_backtrace();

    let node = ZkEvmNode::start(name.to_string(), Layer1Backend::Anvil).await;

    // Create blocks periodically. This seems to be required, but we should
    // investigate how exactly the Ethereum block number drivers the
    // zkevm-node.
    node.l1().mine_blocks_periodic(l1_block_time).await;

    // Get test setup from environment.
    let env = node.env();
    let l2_provider = env.l2_provider();
    let mnemonic = env.funded_mnemonic();
    let hotshot_address = node.l1().hotshot.address();

    let l2 = connect_rpc(&l2_provider, mnemonic, 0, None).await.unwrap();
    let zkevm = ZkEvm {
        chain_id: l2.get_chainid().await.unwrap().as_u64(),
    };

    // Start a sequencer network.
    let nodes = sequencer::testing::init_hotshot_handles().await;
    let api_node = nodes[0].clone();
    let sequencer_store = TempDir::new().unwrap();
    api::Options::from(HttpOptions {
        port: env.sequencer_port(),
    })
    .query(QueryOptions {
        storage_path: sequencer_store.path().into(),
        reset_store: true,
    })
    .submit(Default::default())
    .serve(Box::new(move |_| ready((api_node, 0)).boxed()))
    .await
    .unwrap();
    for node in nodes {
        node.hotshot.start_consensus().await;
    }

    // Start a Polygon zkEVM adaptor.
    let adaptor_opt = polygon_zkevm_adaptor::Options {
        l1_provider: env.l1_provider(),
        sequencer_url: env.sequencer(),
        rpc_port: env.l2_adaptor_rpc_port(),
        l2_chain_id: zkevm.chain_id,
        query_port: env.l2_adaptor_query_port(),
    };
    let hotshot_contract_opt = CommitmentTaskOptions {
        l1_provider: env.l1_provider(),
        sequencer_mnemonic: mnemonic.to_string(),
        sequencer_account_index: node.l1().clients.funded[0].index,
        hotshot_address,
        l1_chain_id: None,
        query_service_url: Some(env.sequencer()),
    };
    spawn(async move {
        join!(
            polygon_zkevm_adaptor::json_rpc::serve(&adaptor_opt),
            polygon_zkevm_adaptor::query_service::serve(&adaptor_opt),
            run_hotshot_commitment_task(&hotshot_contract_opt)
        );
    });

    (node, sequencer_store)
}
