use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use clap::Parser;
use futures::join;
use hermez_adaptor::{Layer1Backend, Run, SequencerZkEvmDemo};
use sequencer_utils::connect_rpc;
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
}

#[async_std::main]
async fn main() {
    setup_logging();
    setup_backtrace();

    let opt = Options::parse();

    let run = if let Some(path) = opt.load_plan {
        tracing::info!("Loading plan from {}", path.display());
        Run::load(&path)
    } else {
        let run = Run::generate(opt.mins);
        let path = opt.save_plan.unwrap();
        tracing::info!("Saved plan to {}", path.display());
        run.save(&path);
        run
    };

    let project_name = "demo".to_string();

    // Start L1 and zkevm-node
    let demo =
        SequencerZkEvmDemo::start_with_sequencer(project_name.clone(), Layer1Backend::Anvil).await;

    // Get test setup from environment.
    let env = demo.env();
    let l2_provider = env.l2_provider();
    let mnemonic = env.funded_mnemonic();
    let l2_client = connect_rpc(&l2_provider, mnemonic, 0, None).await.unwrap();

    let submit_handle = run.submit_operations(l2_client.clone());
    let wait_handle = run.wait_for_effects(l2_client);
    join!(submit_handle, wait_handle);

    tracing::info!("Run complete!");
}
