use clap::Parser;
use polygon_zkevm_adaptor::{Layer1Backend, ZkEvmNode};

#[derive(Parser)]
struct Options {
    /// Whether to run in background
    #[clap(short, long, action)]
    detach: bool,
}

#[async_std::main]
async fn main() {
    let opt = Options::parse();
    let node = ZkEvmNode::start("demo".to_string(), Layer1Backend::Geth).await;

    if opt.detach {
        std::mem::forget(node);
    } else {
        loop {
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}
