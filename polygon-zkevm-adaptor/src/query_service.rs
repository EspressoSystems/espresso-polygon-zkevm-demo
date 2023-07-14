// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

/// Query service adaptor for Polygon zkEVM.
///
/// This service is an adaptor between the generic HotShot query service and the Polygon zkEVM L2 node. It
/// converts blocks in responses from generic HotShot blocks to Polygon zkEVM-encoded EVM blocks.
use crate::Options;
use async_std::sync::RwLock;
use futures::{FutureExt, StreamExt, TryFutureExt};
use hotshot_query_service::availability::BlockQueryData;
use sequencer::{Block, SeqTypes};
use tide_disco::{error::ServerError, App};
use zkevm::{polygon_zkevm::encode_transactions, ZkEvm};

type HotShotClient = surf_disco::Client<ServerError>;

struct State {
    hotshot: HotShotClient,
    zkevm: ZkEvm,
}

pub async fn serve(opt: &Options) {
    let state = State {
        hotshot: HotShotClient::new(opt.sequencer_url.join("availability").unwrap()),
        zkevm: opt.zkevm(),
    };
    state.hotshot.connect(None).await;

    let api = toml::from_str(include_str!("query_api.toml")).unwrap();
    let mut app = App::<_, ServerError>::with_state(RwLock::new(state));
    app.module::<ServerError>("availability", api)
        .unwrap()
        .get("getblock", |req, state| {
            async move {
                let height: u64 = req.integer_param("height")?;
                let block: BlockQueryData<SeqTypes> =
                    state.hotshot.get(&format!("block/{height}")).send().await?;
                Ok(encode_block(state.zkevm, block.block()))
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
                    .socket(&format!("stream/blocks/{height}"))
                    .subscribe::<BlockQueryData<SeqTypes>>()
                    .await?;
                let zkevm = state.zkevm;
                Ok(blocks.map(move |block| Ok(encode_block(zkevm, block?.block()))))
            }
            .try_flatten_stream()
            .boxed()
        })
        .unwrap();

    if let Err(err) = app
        .serve(format!("http://0.0.0.0:{}", opt.query_port))
        .await
    {
        tracing::error!("query service adaptor exited with error: {}", err);
    }
}

fn encode_block(zkevm: ZkEvm, block: &Block) -> String {
    encode_transactions(zkevm.vm_transactions(block)).to_string()
}

#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::task::spawn;
    use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};
    use futures::future::ready;
    use portpicker::pick_unused_port;
    use rand_chacha::{rand_core::SeedableRng, ChaChaRng};
    use sequencer::Vm;
    use std::str::FromStr;
    use tempfile::TempDir;
    use zkevm::EvmTransaction;

    #[async_std::test]
    async fn test_query_service_adaptor() {
        setup_logging();
        setup_backtrace();

        let sequencer_port = pick_unused_port().unwrap();
        let adaptor_port = pick_unused_port().unwrap();

        // Start a sequencer network.
        let nodes = sequencer::testing::init_hotshot_handles().await;
        let api_node = nodes[0].clone();
        let sequencer_store = TempDir::new().unwrap();
        let options = sequencer::api::Options {
            port: sequencer_port,
            storage_path: sequencer_store.path().into(),
            reset_store: true,
        };
        sequencer::api::serve(options, Box::new(move |_| ready((api_node, 0)).boxed()))
            .await
            .unwrap();
        for node in &nodes {
            node.start().await;
        }

        // Start the query service adaptor.
        let opt = Options {
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
            .subscribe::<String>()
            .await
            .unwrap()
            .enumerate();

        // Create a ZkEVM transaction.
        let signer = LocalWallet::new(&mut ChaChaRng::seed_from_u64(0));
        let txn = TypedTransaction::Eip1559(Default::default());
        let sig = signer.sign_transaction(&txn).await.unwrap();
        let txn = EvmTransaction::new(txn, sig);

        // Sequence the transaction.
        nodes[0].submit_transaction(zkevm.wrap(&txn)).await.unwrap();

        // Wait for it to be sequenced.
        let expected = encode_transactions(vec![&txn]);
        let block_num = loop {
            let (i, block) = blocks.next().await.unwrap();
            let block = Bytes::from_str(&block.unwrap()).unwrap();
            if block.is_empty() {
                continue;
            }

            assert_eq!(block, expected);
            break i;
        };

        let block = adaptor
            .get::<String>(&format!("block/{block_num}"))
            .send()
            .await
            .unwrap();
        assert_eq!(expected, Bytes::from_str(&block).unwrap());
    }
}
