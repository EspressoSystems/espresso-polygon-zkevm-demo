// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo. 
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
