// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use async_std::task::sleep;
use clap::Parser;
use ethers::prelude::*;
use futures::join;
use http_types::Url;
use polygon_zkevm_adaptor::{connect_rpc_simple, CombinedOperations, Run};
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

    /// URL for an optional L2 JSON-RPC service using preconfirmations.
    #[arg(long)]
    pub preconfirmations_l2_provider: Option<Url>,

    /// Mnemonic for a funded L2 account, which the load test will drain.
    #[arg(long)]
    pub mnemonic: String,
}

#[async_std::main]
async fn main() {
    setup_logging();
    setup_backtrace();

    let opt = Options::parse();

    let operations = if let Some(path) = opt.load_plan {
        tracing::info!("Loading plan from {}", path.display());
        CombinedOperations::load(&path)
    } else {
        let operations = CombinedOperations::generate(opt.mins);
        let path = opt.save_plan.unwrap();
        tracing::info!("Saved plan to {}", path.display());
        operations.save(&path);
        operations
    };

    // Get test setup from environment.
    let signer = connect_rpc_simple(&opt.l2_provider, &opt.mnemonic, 0, None)
        .await
        .unwrap();
    // Use the second account for the second connection. Even though the two signers will be
    // _submitting_ transactions to different RPCs, both RPCs will see the transactions from both
    // signers come out of the sequencer, which means using the same account for both could cause
    // the two random clients to interfere with each other.
    //
    // Using two different signers connected to the two RPCs ensures we are continuously testing
    // that both RPCs are still working, and stresses the scenario where an RPC sees a transaction
    // that it didn't submit.
    let preconf_signer = match &opt.preconfirmations_l2_provider {
        Some(provider) => {
            let preconf_signer = connect_rpc_simple(provider, &opt.mnemonic, 1, None)
                .await
                .unwrap();
            // Find the balance of the funded account.
            let balance = signer.get_balance(signer.address(), None).await.unwrap();

            // Transfer some funds from the first (funded) account to the second one.
            let transfer_amount = balance / 2;
            tracing::info!("Transferring {transfer_amount}/{balance} to unfunded account");
            let tx = TransactionRequest::default()
                .to(preconf_signer.address())
                .value(transfer_amount);
            let hash = signer.send_transaction(tx, None).await.unwrap().tx_hash();

            // Wait for the transfer to complete.
            loop {
                if let Some(receipt) = signer.get_transaction_receipt(hash).await.unwrap() {
                    tracing::info!("transfer {hash} completed: {receipt:?}");
                    break;
                }
                tracing::info!("Waiting for transfer {hash} to complete");
                sleep(Duration::from_secs(1)).await;
            }

            Some(preconf_signer)
        }
        None => None,
    };

    let run = Run::new("regular", operations.regular_node, signer);
    let preconf_run =
        preconf_signer.map(|signer| Run::new("preconf", operations.preconf_node, signer));
    let ((regular_submitted, regular_successful), preconf_results) =
        join!(run.wait(), async move {
            if let Some(run) = preconf_run {
                Some(run.wait().await)
            } else {
                None
            }
        });

    tracing::info!("Run complete!");
    tracing::info!(
        "{regular_successful}/{regular_submitted} transactions successful via regular node"
    );
    if let Some((preconf_submitted, preconf_successful)) = preconf_results {
        tracing::info!(
            "{preconf_successful}/{preconf_submitted} transactions successful via preconf node"
        );
    }
}
