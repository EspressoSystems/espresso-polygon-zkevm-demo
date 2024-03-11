// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Query service adaptor for Polygon zkEVM.
//!
//! This service is an adaptor between the generic HotShot query service and the Polygon zkEVM L2
//! node. It converts blocks in responses from generic HotShot blocks to Polygon zkEVM-encoded EVM
//! blocks. This involves extracting the Polygon zkEVM transactions out of a HotShot block (which
//! may contain transactions for several different L2s), encoding them according to the format
//! expected by the Polygon L2, and adding extra metadata, such as a timestamp and L1 block number.
//!
//! This functionality is encapsulated into its own service for purposes of demoing the
//! Espresso-Polygon integration, as it allows us to run the demo with minimal changes to the
//! Polygon L2 node. However, in a production system, the node itself would be responsible for
//! extracting the relevant transactions and decoding them directly from HotShot.

use crate::Options;
use async_std::sync::RwLock;
use futures::{try_join, FutureExt, StreamExt, TryFutureExt};
use hotshot_query_service::availability::{BlockQueryData, VidCommonQueryData};
use sequencer::SeqTypes;
use serde::{Deserialize, Serialize};
use tide_disco::{error::ServerError, App};
use zkevm::{polygon_zkevm::encode_transactions, ZkEvm};

type HotShotClient = surf_disco::Client<ServerError>;

struct State {
    hotshot: HotShotClient,
    zkevm: ZkEvm,
}

pub async fn serve(opt: &Options) {
    let hotshot = HotShotClient::new(opt.sequencer_url.clone());
    let state = State {
        hotshot,
        zkevm: opt.zkevm(),
    };
    state.hotshot.connect(None).await;

    let api: toml::Value = toml::from_str(include_str!("query_api.toml")).unwrap();
    let mut app = App::<_, ServerError>::with_state(RwLock::new(state));
    app.module::<ServerError>("availability", api)
        .unwrap()
        .get("getblock", |req, state| {
            async move {
                let height: u64 = req.integer_param("height")?;
                let (block, vid_common) = try_join!(
                    state
                        .hotshot
                        .get(&format!("availability/block/{height}"))
                        .send(),
                    state
                        .hotshot
                        .get(&format!("availability/vid/common/{height}"))
                        .send(),
                )?;
                Ok(PolygonZkevmBlock::new(state.zkevm, &block, &vid_common))
            }
            .boxed()
        })
        .unwrap()
        .stream("streamblocks", |req, state| {
            async move {
                let state = state.read().await;
                let height: u64 = req.integer_param("height")?;
                let blocks = state
                    .hotshot
                    .socket(&format!("availability/stream/blocks/{height}"))
                    .subscribe::<BlockQueryData<SeqTypes>>()
                    .await?;
                let vid_common = state
                    .hotshot
                    .socket(&format!("availability/stream/vid/common/{height}"))
                    .subscribe::<VidCommonQueryData<SeqTypes>>()
                    .await?;
                let zkevm = state.zkevm;
                Ok(blocks.zip(vid_common).map(move |(block, common)| {
                    Ok(PolygonZkevmBlock::new(zkevm, &block?, &common?))
                }))
            }
            .try_flatten_stream()
            .boxed()
        })
        .unwrap()
        .get("blockheight", |_, state| {
            async move {
                let height: usize = state.hotshot.get("status/block-height").send().await?;
                Ok(height)
            }
            .boxed()
        })
        .unwrap();

    if let Err(err) = app.serve(format!("0.0.0.0:{}", opt.query_port)).await {
        tracing::error!("query service adaptor exited with error: {}", err);
    }
}

/// Block of Polygon zkEVM transactions produced by the HotShot sequencer.
///
/// This type, derived from a sequencer block, contains the Polygon zkEVM transactions extracted
/// from the sequencer block and hex encoded according to the format expected by the zkEVM node. It
/// also contains metadata fields used by the node to associate this L2 block with an L1 block.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PolygonZkevmBlock {
    timestamp: u64,
    height: u64,
    l1_block: u64,
    transactions: String,
}

impl PolygonZkevmBlock {
    fn new(
        zkevm: ZkEvm,
        l2_block: &BlockQueryData<SeqTypes>,
        vid_common: &VidCommonQueryData<SeqTypes>,
    ) -> Self {
        Self {
            timestamp: l2_block.header().timestamp,
            height: l2_block.height(),
            l1_block: l2_block.header().l1_head,
            transactions: encode_transactions(zkevm.vm_transactions(l2_block, vid_common))
                .to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::task::spawn;
    use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};
    use futures::future::{ready, FutureExt};
    use portpicker::pick_unused_port;
    use rand_chacha::{rand_core::SeedableRng, ChaChaRng};
    use sequencer::{
        api::{self, options},
        persistence::fs,
        testing::TestConfig,
    };
    use sequencer_utils::AnvilOptions;
    use std::str::FromStr;
    use std::time::Duration;
    use tempfile::TempDir;
    use zkevm::EvmTransaction;

    #[async_std::test]
    async fn test_query_service_adaptor() {
        setup_logging();
        setup_backtrace();

        let l1_rpc_port = pick_unused_port().unwrap();
        let sequencer_port = pick_unused_port().unwrap();
        let adaptor_port = pick_unused_port().unwrap();

        // Start an L1 node.
        let l1 = AnvilOptions::default()
            .port(l1_rpc_port)
            .block_time(Duration::from_secs(1))
            .spawn()
            .await;

        // Start a sequencer network.
        let cfg = TestConfig::default();
        let mut nodes = cfg.init_nodes().await;
        for node in &nodes {
            node.start_consensus().await;
        }
        let handle = nodes[0].consensus().clone();

        // Start query service.
        let sequencer_store = TempDir::new().unwrap();
        api::Options::from(options::Http {
            port: sequencer_port,
        })
        .query_fs(
            Default::default(),
            fs::Options {
                path: sequencer_store.path().into(),
            },
        )
        .serve(move |_| ready(nodes.remove(0)).boxed())
        .await
        .unwrap();

        // Start the query service adaptor.
        let opt = Options {
            l1_provider: l1.url(),
            sequencer_url: format!("http://localhost:{sequencer_port}")
                .parse()
                .unwrap(),
            l2_chain_id: 1001,
            rpc_port: 0,
            query_port: adaptor_port,
        };
        let zkevm = opt.zkevm();
        spawn(async move { serve(&opt).await });

        // Subscribe to future blocks.
        let adaptor = surf_disco::Client::<ServerError>::new(
            format!("http://localhost:{adaptor_port}/availability")
                .parse()
                .unwrap(),
        );
        adaptor.connect(None).await;
        let mut blocks = adaptor
            .socket("stream/blocks/0")
            .subscribe::<PolygonZkevmBlock>()
            .await
            .unwrap()
            .enumerate();

        // Create a ZkEVM transaction.
        let signer = LocalWallet::new(&mut ChaChaRng::seed_from_u64(0));
        let txn = TypedTransaction::Eip1559(Default::default());
        let sig = signer.sign_transaction(&txn).await.unwrap();
        let txn = EvmTransaction::new(txn, sig);

        // Sequence the transaction.
        handle.submit_transaction(zkevm.wrap(&txn)).await.unwrap();

        // Wait for it to be sequenced.
        let expected = encode_transactions(vec![&txn]);
        let block_num = loop {
            let (i, block) = blocks.next().await.unwrap();
            let block = block.unwrap();
            assert_eq!(block.height, i as u64);
            let block = Bytes::from_str(&block.transactions).unwrap();
            if block.is_empty() {
                continue;
            }

            assert_eq!(block, expected);
            break i;
        };

        let block = adaptor
            .get::<PolygonZkevmBlock>(&format!("block/{block_num}"))
            .send()
            .await
            .unwrap();
        assert_eq!(block.height, block_num as u64);
        assert_eq!(expected, Bytes::from_str(&block.transactions).unwrap());
    }
}
