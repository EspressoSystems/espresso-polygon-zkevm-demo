use clap::Parser;
use polygon_zkevm_adaptor::{Layer1Backend, SequencerZkEvmDemo};

#[derive(Parser)]
struct Options {
    /// Whether to run in background
    #[clap(short, long, action)]
    detach: bool,

    /// Layer 1 backend to use.
    #[arg(long, default_value = "geth")]
    pub l1_backend: Layer1Backend,
}

#[async_std::main]
async fn main() {
    let opt = Options::parse();
    let project_name = "demo".to_string();
    let demo = SequencerZkEvmDemo::start_with_sequencer(project_name, opt.l1_backend).await;

    if opt.detach {
        std::mem::forget(demo);
    } else {
        loop {
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}
