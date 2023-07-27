// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use clap::Parser;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Middleware as _, Provider},
    signers::{coins_bip39::English, MnemonicBuilder, Signer},
};
use futures::join;
use http_types::Url;
use polygon_zkevm_adaptor::{InnerMiddleware, Operations, Run};
use sequencer_utils::wait_for_rpc;
use std::{num::ParseIntError, path::PathBuf, time::Duration};

/// Run a load test against an existing ZkEVM node.
///
/// It is the responsibility of the user to save the log output for inspection
/// later.
#[derive(Parser)]
pub struct Options {
    /// Where to save the test plan JSON file.
    ///
    /// If specified, a new test plan will be generated, saved to this file and
    /// executed.
    #[arg(
        long,
        required_unless_present = "load_plan",
        conflicts_with = "load_plan"
    )]
    pub save_plan: Option<PathBuf>,

    /// Where to load the test plan JSON file from for replay.
    ///
    /// If specified, the test plan will be loaded from this file and executed.
    #[arg(
        long,
        required_unless_present = "save_plan",
        conflicts_with = "save_plan"
    )]
    pub load_plan: Option<PathBuf>,

    /// Sum of sleep time between transactions.
    ///
    /// The runtime of the test will be lower bounded by this value.
    #[arg(
        long,
        default_value = "1",
        conflicts_with = "load_plan",
        value_parser = |arg: &str| -> Result<Duration, ParseIntError> { Ok(60 * Duration::from_secs(arg.parse()?)) }
    )]
    pub mins: Duration,

    /// URL for the L2 JSON-RPC service.
    #[arg(long)]
    pub l2_provider: Url,

    /// Mnemonic for a funded L2 account, which the load test will drain.
    #[arg(long)]
    pub mnemonic: String,
}

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

#[async_std::main]
async fn main() {
    setup_logging();
    setup_backtrace();

    let opt = Options::parse();

    let operations = if let Some(path) = opt.load_plan {
        tracing::info!("Loading plan from {}", path.display());
        Operations::load(&path)
    } else {
        let operations = Operations::generate(opt.mins);
        let path = opt.save_plan.unwrap();
        tracing::info!("Saved plan to {}", path.display());
        operations.save(&path);
        operations
    };

    // Get test setup from environment.
    let signer = connect_rpc_simple(&opt.l2_provider, &opt.mnemonic, 0, None)
        .await
        .unwrap();

    wait_for_rpc(&opt.l2_provider, Duration::from_secs(1), 10)
        .await
        .unwrap();

    // At this point we may still get errors when talking to the RPC,
    // so wait a bit more.
    async_std::task::sleep(Duration::from_secs(10)).await;

    let run = Run::new(operations, signer);

    let submit_handle = run.submit_operations();
    let wait_handle = run.wait_for_effects();
    join!(submit_handle, wait_handle);

    tracing::info!("Run complete!");
}
