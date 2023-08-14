// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use clap::Parser;
use surf_disco::Url;
use zkevm::ZkEvm;

pub mod json_rpc;
pub mod query_service;

#[derive(Parser)]
pub struct Options {
    /// URL of a HotShot sequencer node.
    #[clap(long, env = "ESPRESSO_SEQUENCER_URL")]
    pub sequencer_url: Url,

    /// URL of layer 1 Ethereum JSON-RPC provider.
    #[clap(long, env = "ESPRESSO_ZKEVM_L1_PROVIDER")]
    pub l1_provider: Url,

    /// Chain ID for layer 2 EVM.
    ///
    /// This will be used as the VM ID for layer 2 EVM transactions within the HotShot sequencer.
    #[clap(long, env = "ESPRESSO_ZKEVM_L2_CHAIN_ID", default_value = "1001")]
    pub l2_chain_id: u64,

    /// Port on which to serve the JSON-RPC API.
    #[clap(
        short,
        long,
        env = "ESPRESSO_ZKEVM_ADAPTOR_RPC_PORT",
        default_value = "8545"
    )]
    pub rpc_port: u16,

    /// Port on which to serve the Polygon zkEVM query API adaptor.
    #[clap(
        short,
        long,
        env = "ESPRESSO_ZKEVM_ADAPTOR_QUERY_PORT",
        default_value = "50100"
    )]
    pub query_port: u16,
}

impl Options {
    pub fn zkevm(&self) -> ZkEvm {
        ZkEvm {
            chain_id: self.l2_chain_id,
        }
    }
}

mod polygon_zkevm;
#[cfg(any(test, feature = "testing"))]
pub use polygon_zkevm::*;

mod random_client;
#[cfg(any(test, feature = "testing"))]
pub use random_client::*;

mod demo_with_sequencer;
#[cfg(any(test, feature = "testing"))]
pub use demo_with_sequencer::*;
