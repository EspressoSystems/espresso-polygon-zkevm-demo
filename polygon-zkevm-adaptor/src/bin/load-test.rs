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
use polygon_zkevm_adaptor::{
    connect_rpc_simple, CombinedOperations, Layer1Backend, Run, SequencerZkEvmDemo,
};
use std::{num::ParseIntError, path::PathBuf, time::Duration};

/// Run a load test on the ZkEVM node.
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

    /// Layer 1 backend to use.
    #[arg(long, default_value = "geth")]
    pub l1_backend: Layer1Backend,
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

    let project_name = "demo".to_string();

    // Start L1 and zkevm-node
    let demo = SequencerZkEvmDemo::start_with_sequencer(project_name.clone(), opt.l1_backend).await;

    // Get test setup from environment.
    let env = demo.env();
    let mnemonic = env.funded_mnemonic();
    // Connect clients to stress test both the regular L2 node and the preconfirmations node.
    let signer = connect_rpc_simple(&env.l2_provider(), mnemonic, 0, None)
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
    let preconf_signer = connect_rpc_simple(&env.l2_preconfirmations_provider(), mnemonic, 1, None)
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

    let run = Run::new("regular", operations.regular_node, signer);
    let preconf_run = Run::new("preconf", operations.preconf_node, preconf_signer);
    join!(run.wait(), preconf_run.wait());

    tracing::info!("Run complete!");
}
