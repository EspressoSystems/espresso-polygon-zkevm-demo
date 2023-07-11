//! Query service adaptor for Polygon zkEVM.
//!
//! This service is an adaptor between the generic HotShot query service and the Polygon zkEVM L2
//! node. It converts blocks in responses from generic HotShot blocks to Polygon zkEVM-encoded EVM
//! blocks.
//!
//! This adaptor performs the straightforward function of extracting the Polygon zkEVM transactions
//! out of a HotShot block (which may contain transactions for several different L2s) and encoding
//! them according to the format expected by the Polygon L2. This functionality is encapsulated into
//! its own service for purposes of demoing the Espresso-Polygon integration, as it allows us to
//! run the demo with minimal changes to the Polygon L2 node. However, in a production system, the
//! node itself would be responsible for extracting the relevant transactions and decoding them
//! directly from HotShot.
//!
//! This adaptor also performs a secondary function, to work around a limitation of the current
//! version of HotShot consensus. In order for the bridge between L1 and Polygon zkEVM to work, each
//! L2 block must be associated with a unique L1 block, so that we know which global exit root
//! (maintained on L1) to use when executing bridge withdrawals on L2. In the original Polygon
//! zkEVM, this association is determined whenever an L2 batch is sequenced on L1. This design makes
//! it impossible to use the HotShot sequencer for fast preconfirmations, because even though a
//! canonical ordering of L2 blocks is determined quickly, we cannot execute those blocks until they
//! have been persisted on L1, which can be slow.
//!
//! To enable fast preconfirmations, we redefine the way in which L2 blocks get associated with L1
//! blocks. Each time an L2 block is _sequenced_, the HotShot consensus protocol assigns it an L1
//! block number, which is guaranteed to be a recent L1 block number by a quorum of the stake. This
//! means each L2 block is *immediately* associated with an L1 block in a determinstic and
//! unequivocal way. We use this association when executing the block and later when proving it, so
//! there is no need to wait for the block to be sent to L1 in order to compute the resulting state.
//!
//! As a limitation of the current demo, HotShot has not yet been updated to reach consensus on an
//! L1 block number for each L2 block, so we mock this feature by using the _timestamp_ attached to
//! each L2 block to match it with an L1 block. Specifically, the L1 block associated with an L2
//! block is defined as the unique L1 block whose timestamp is less than or equal to the L2 block's
//! timestamp, and whose successor's timestamp is greater than the L2 block's timestamp.
//!
//! This would be a valid and deterministic way of matching L1 and L2 blocks, since Ethereum
//! timestamps are monotonically increasing. It might even be a nicer way than using block numbers,
//! since the global exit root contract keeps track of the L1 block timestamp associated with each
//! historical global exit root. However, it is unfortunately unsound because HotShot block
//! timestamps are non-normative: they are proposed by the leader and unconditionally accepted by
//! replicas. Thus, while this method is suitable for demo purposes, where the leader is always
//! honest, consensus changes will be needed before moving this to production.
//!
//! Unfortunately, looking up the L1 block nearest to a given timestamp is not so easy, as the
//! standard Ethereum JSON-RPC interface does not expose an index by timestamp. Thus, we build this
//! index ourselves. Part of the state maintained by the web server is a mapping between L2 block
//! numbers and the corresponding L1 block numbers. This mapping is kept up-to-date by a background
//! task which streams L2 and L1 blocks, finding for each L2 block the last L1 block which is older
//! than the L2 block. This mapping can then be queried or subscribed in response to requests from
//! clients.

use crate::Options;
use async_compatibility_layer::async_primitives::broadcast::{channel, BroadcastSender};
use async_std::{
    sync::{Arc, RwLock},
    task::{sleep, spawn},
};
use ethers::prelude::*;
use futures::{
    stream::{self, Peekable, Stream},
    FutureExt, StreamExt, TryFutureExt,
};
use hotshot_query_service::availability::BlockQueryData;
use sequencer::SeqTypes;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;
use tide_disco::{error::ServerError, App, Error, StatusCode};
use zkevm::{polygon_zkevm::encode_transactions, ZkEvm};

type HotShotClient = surf_disco::Client<ServerError>;

struct State {
    blocks: Arc<RwLock<BlockMapping>>,
    hotshot: HotShotClient,
    zkevm: ZkEvm,
}

pub async fn serve(opt: &Options) {
    let mut l1_provider = opt.l1_provider.clone();
    l1_provider.set_scheme("ws").unwrap();
    let l1 = loop {
        match Provider::connect(l1_provider.clone()).await {
            Ok(l1) => break l1,
            Err(err) => {
                tracing::warn!("error connecting to L1, retrying: {err}");
                sleep(Duration::from_secs(1)).await;
            }
        }
    };

    let hotshot = HotShotClient::new(opt.sequencer_url.join("availability").unwrap());
    let state = State {
        blocks: BlockMapping::new(l1, hotshot.clone()).await.unwrap(),
        hotshot,
        zkevm: opt.zkevm(),
    };
    state.hotshot.connect(None).await;

    let api = toml::from_str(include_str!("query_api.toml")).unwrap();
    let mut app = App::<_, ServerError>::with_state(RwLock::new(state));
    app.module::<ServerError>("availability", api)
        .unwrap()
        .at("getblock", |req, state| {
            async move {
                let height: u64 = req.integer_param("height")?;
                let state = state.read().await;
                let block: BlockQueryData<SeqTypes> =
                    state.hotshot.get(&format!("block/{height}")).send().await?;
                // Find the L1 block number corresponding to this L2 block, based on its timestamp.
                let l1_block = state
                    .blocks
                    .read()
                    .await
                    .l1_block_from_l2_block(block.height())
                    .ok_or_else(|| {
                        ServerError::catch_all(
                            StatusCode::NotFound,
                            format!("invalid block height {height}"),
                        )
                    })?;
                Ok(PolygonZkevmBlock::new(state.zkevm, block, l1_block))
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
                    .await?
                    // Map each L2 block to an L1 block number based on its timestamp.
                    .zip(state.blocks.read().await.subscribe(height as usize).await);
                let zkevm = state.zkevm;
                Ok(blocks.map(move |(block, (l2_block, l1_block))| {
                    let block = block?;
                    // Sanity check that we have mapped the correct L2 block to an L1 block.
                    assert_eq!(block.height(), l2_block);
                    Ok(PolygonZkevmBlock::new(zkevm, block, l1_block))
                }))
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

// Mapping from L2 block numbers to L1 block numbers.
struct BlockMapping {
    // L1 block numbers indexed by L2 block number.
    l1_blocks: Vec<u64>,
    // Stream of L1 blocks.
    l1_block_stream: Peekable<Pin<Box<dyn Stream<Item = Block<H256>> + Send + Sync>>>,
    // Output stream of L2->L1 mappings.
    output_stream: BroadcastSender<(u64, u64)>,
}

impl BlockMapping {
    async fn new<M>(l1: M, hotshot: HotShotClient) -> Result<Arc<RwLock<Self>>, M::Error>
    where
        M: Middleware + 'static,
        M::Provider: PubsubClient + 'static,
        <M::Provider as PubsubClient>::NotificationStream: Sync,
    {
        // Get a stream of L1 blocks.
        let l1 = Box::leak(Box::new(l1));
        let l1_block_stream: Pin<Box<dyn Stream<Item = Block<H256>> + Send + Sync>> =
            Box::pin(l1.subscribe_blocks().await?);

        // Create the mapping. This object will be shared between the background task responsible
        // for updating it and the web server, which uses it to respond to requests.
        let mapping = Arc::new(RwLock::new(Self {
            l1_blocks: vec![],
            l1_block_stream: l1_block_stream.peekable(),
            output_stream: channel().0,
        }));
        let ret = mapping.clone();

        // Spawn a task to update the mapping with new L2 blocks.
        spawn(async move {
            // Subscribe to a block stream from HotShot, retrying until we succeed (this request can
            // fail during initialization, until the HotShot query service is up and running).
            let mut l2_blocks = loop {
                match hotshot
                    .socket("stream/blocks/0")
                    .subscribe::<BlockQueryData<SeqTypes>>()
                    .await
                {
                    Ok(stream) => break stream,
                    Err(err) => {
                        tracing::warn!(
                            "unable to subscribe to HotShot block stream, retrying: {err}"
                        );
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            };
            // Append each L2 block to the mapping.
            while let Some(block) = l2_blocks.next().await {
                let block = match block {
                    Ok(block) => block,
                    Err(err) => {
                        tracing::warn!("Error in HotShot block stream: {err}");
                        continue;
                    }
                };
                if let Err(err) = mapping
                    .write()
                    .await
                    .append(block.timestamp().unix_timestamp() as u64)
                    .await
                {
                    tracing::error!("Unexpected error appending L2 block: {err}");
                    return;
                }
            }
            tracing::warn!("Unexpected end of L2 block stream");
        });

        Ok(ret)
    }

    async fn append(&mut self, timestamp: u64) -> Result<(), String> {
        tracing::debug!("Matching L2 block with L1 block, timestamp={timestamp}");

        // Skip L1 blocks until we find one which is newer than the new L2 block. The most recent L1
        // block before this terminal block is the one corresponding to the L2 block.
        let mut l1_block_num = self.l1_blocks.last().cloned().unwrap_or(0);
        loop {
            let next_l1_block = match Pin::new(&mut self.l1_block_stream).peek().now_or_never() {
                Some(Some(block)) => block,
                Some(None) => Err("unexpected end of L1 block stream")?,
                None => {
                    // If the next block in the L1 stream is not ready immediately (ie awaiting it
                    // would have blocked) we can safely assume that the most recent L1 block we've
                    // seen is the one corresponding to this L2 block, since the next L1 block
                    // (which has not been produced yet) will necessarily be newer than the L2 block
                    // (which has been produced).
                    //
                    // This is an important optimization; by never blocking on production of the
                    // next L1 block, we ensure preconfirmations can proceed faster than the L1
                    // block rate.
                    break;
                }
            };
            tracing::debug!(
                "L1 block {:?} has timestamp {}",
                next_l1_block.number,
                next_l1_block.timestamp
            );
            if next_l1_block.timestamp <= timestamp.into() {
                // This L1 block is older than the L2 block; it could be the corresponding block.
                // Remember and continue looking at more L1 blocks.
                l1_block_num = next_l1_block
                    .number
                    .ok_or("finalized L1 block has no number")?
                    .as_u64();
                self.l1_block_stream.next().await;
            } else {
                // We've found a block newer than the L2 block; the last L1 block we saw must be
                // the corresponding one.
                break;
            }
        }

        // Notify subscribers of the new block.
        self.output_stream
            .send_async((self.l1_blocks.len() as u64, l1_block_num))
            .await
            .ok();

        // Remember the new block's L1 block number.
        self.l1_blocks.push(l1_block_num);

        Ok(())
    }

    fn l1_block_from_l2_block(&self, l2_block_num: u64) -> Option<u64> {
        self.l1_blocks.get(l2_block_num as usize).cloned()
    }

    /// Subscribe to a stream of (L2, L1) block number mappings.
    async fn subscribe(&self, from: usize) -> impl Stream<Item = (u64, u64)> {
        // Stream the blocks starting from `from` that we already have directly form memory.
        let existing_blocks = stream::iter(self.l1_blocks[from..].to_vec())
            .enumerate()
            .map(move |(i, l1_block)| ((from + i) as u64, l1_block));

        // Stream future blocks from the broadcast channel, skipping any before `from`.
        let future_blocks = stream::unfold(
            self.output_stream.handle_async().await,
            |mut handle| async move {
                match handle.recv_async().await {
                    Ok(mapping) => Some((mapping, handle)),
                    Err(_) => {
                        // An error in receive means the send end of the channel has been
                        // disconnected, which means the stream is over.
                        None
                    }
                }
            },
        )
        .skip(from.saturating_sub(self.l1_blocks.len()));

        existing_blocks.chain(future_blocks)
    }
}

/// Block of Polygon zkEVM transactions produced by the HotShot sequencer.
///
/// This type, derived from a sequencer block, contains the Polygon zkEVM transactions extracted
/// from the sequencer block and hex encoded according to the format expected by the zkEVM node. It
/// also contains metadata fields used by the node to associated this L2 block with an L1 block.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PolygonZkevmBlock {
    timestamp: u64,
    height: u64,
    l1_block: u64,
    transactions: String,
}

impl PolygonZkevmBlock {
    fn new(zkevm: ZkEvm, l2_block: BlockQueryData<SeqTypes>, l1_block: u64) -> Self {
        Self {
            timestamp: l2_block.timestamp().unix_timestamp() as u64,
            height: l2_block.height(),
            l1_block,
            transactions: encode_transactions(zkevm.vm_transactions(l2_block.block())).to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::task::spawn;
    use ethers::types::transaction::eip2718::TypedTransaction;
    use futures::future::ready;
    use portpicker::pick_unused_port;
    use rand_chacha::{rand_core::SeedableRng, ChaChaRng};
    use sequencer::Vm;
    use sequencer_utils::AnvilOptions;
    use std::str::FromStr;
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
        nodes[0].submit_transaction(zkevm.wrap(&txn)).await.unwrap();

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
