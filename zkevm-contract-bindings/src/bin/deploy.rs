// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::Result;
use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use clap::Parser;
use contract_bindings::hot_shot::HotShot;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{coins_bip39::English, MnemonicBuilder, Signer as _},
    types::Address,
    utils::{get_contract_address, parse_ether},
};
use hex::{FromHex, FromHexError};
use sequencer_utils::Signer;
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use std::{num::ParseIntError, path::PathBuf};
use std::{sync::Arc, time::Duration};
use url::Url;
use zkevm_contract_bindings::{
    erc20_permit_mock::ERC20PermitMock, polygon_zk_evm_bridge::PolygonZkEVMBridge,
    polygon_zk_evm_global_exit_root::PolygonZkEVMGlobalExitRoot,
    shared_types::InitializePackedParameters,
    verifier_rollup_helper_mock::VerifierRollupHelperMock, Deploy, PolygonZkEVM,
};

/// A script to deploy all contracts for the demo to an Ethereum RPC.
///
/// Note: The default config values are suitable for testing only.
#[derive(Parser, Debug, Clone)]
pub struct Options {
    /// The mnemonic of the deployer wallet.
    ///
    /// Account zero of this wallet will be used to deploy the contracts.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_MNEMONIC",
        default_value = "test test test test test test test test test test test junk"
    )]
    pub mnemonic: String,

    /// The account index of the deployer wallet.
    #[arg(long, env = "ESPRESSO_ZKEVM_DEPLOY_ACCOUNT_INDEX", default_value = "0")]
    pub account_index: u32,

    /// The URL of an Ethereum JsonRPC where the contracts will be deployed.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_WEB3_PROVIDER_URL",
        default_value = "http://localhost:8545"
    )]
    pub provider_url: Url,

    /// The address of the hotshot contract, if already deployed.
    #[arg(long, env = "ESPRESSO_SEQUENCER_HOTSHOT_ADDRESS")]
    pub hotshot_address: Option<Address>,

    /// Whether to deploy the second zkevm-node.
    ///
    /// If false, only the contracts for the first rollup are deployed.
    #[arg(long, env = "ESPRESSO_ZKEVM_DEPLOY_ROLLUP_2", default_value = "false")]
    pub deploy_rollup_2: bool,

    /// Wallet address of the trusted aggregator for the first zkevm.
    ///
    /// This needs to the address of the wallet that the zkevm aggregator
    /// services uses to sign Ethereum transactions.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_1_TRUSTED_AGGREGATOR_ADDRESS",
        default_value = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
    )]
    pub trusted_aggregator_1: Address,

    /// Wallet address of the trusted aggregator for the second zkevm.
    ///
    /// This needs to the address of the wallet that the zkevm aggregator
    /// services uses to sign Ethereum transactions.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_2_TRUSTED_AGGREGATOR_ADDRESS",
        default_value = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
    )]
    pub trusted_aggregator_2: Address,

    /// Genesis root for the first L2.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_1_GENESIS_ROOT",
        value_parser = |arg: &str| -> Result<[u8; 32], FromHexError> { Ok(<[u8; 32]>::from_hex(arg)?) },
        default_value = "5c8df6a4b7748c1308a60c5380a2ff77deb5cfee3bf4fba76eef189d651d4558",
      )]
    pub genesis_root_1: [u8; 32],

    /// Genesis root for the second L2.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_2_GENESIS_ROOT",
        value_parser = |arg: &str| -> Result<[u8; 32], FromHexError> { Ok(<[u8; 32]>::from_hex(arg)?) },
        default_value = "5c8df6a4b7748c1308a60c5380a2ff77deb5cfee3bf4fba76eef189d651d4558",
      )]
    pub genesis_root_2: [u8; 32],

    /// Chain ID for the first L2.
    #[arg(long, env = "ESPRESSO_ZKEVM_1_CHAIN_ID", default_value = "1001")]
    pub chain_id_1: u64,

    /// Chain ID for the second L2.
    #[arg(long, env = "ESPRESSO_ZKEVM_2_CHAIN_ID", default_value = "1002")]
    pub chain_id_2: u64,

    /// Output file path where deployment info will be stored.
    #[arg(
        short,
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_OUTPUT_PATH",
        default_value = "deployment.env"
    )]
    pub output_path: PathBuf,

    /// Output file path where deployment info will be stored.
    #[arg(long, env = "ESPRESSO_ZKEVM_DEPLOY_OUTPUT_JSON")]
    pub json: bool,

    /// Polling interval for the RPC provider
    ///
    /// By default the ether-rs default of 7 seconds will be used.
    #[arg(
        short,
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_POLLING_INTERVAL_MS",
        value_parser = |arg: &str| -> Result<Duration, ParseIntError> { Ok(Duration::from_millis(arg.parse()?)) }
    )]
    pub polling_interval: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct ZkEvmDeploymentInput {
    /// The address of the hotshot contract.
    hotshot_address: Address,
    /// The address of the trusted aggregator.
    trusted_aggregator: Address,
    /// The genesis root of the rollup contract.
    #[serde(with = "hex::serde")]
    genesis_root: [u8; 32],
    /// The chain ID of the L2.
    chain_id: u64,
    /// The fork ID of the L2.
    fork_id: u64,
    /// The network name.
    network_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct ZkEvmDeploymentOutput {
    /// The address of the rollup contract.
    rollup_address: Address,
    /// The address of the matic contract.
    matic_address: Address,
    /// The address of the bridge contract.
    bridge_address: Address,
    /// The address of the global exit root contract.
    ger_address: Address,
    /// The address of the verifier contract.
    verifier_address: Address,
    /// The L1 block number when the rollup contract was deployed.
    genesis_block_number: u64,
    /// The hotshot block number where the rollup contract was deployed.
    genesis_hotshot_block_number: u64,
}

with_prefix!(prefix_zkevm_1 "ESPRESSO_ZKEVM_1_");
with_prefix!(prefix_zkevm_2 "ESPRESSO_ZKEVM_2_");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct DeploymentOutput {
    /// The address of the hotshot contract.
    #[serde(rename = "ESPRESSO_SEQUENCER_HOTSHOT_ADDRESS")]
    // Note: serde_with produces wrong output here, so do it manually.
    hotshot_address: Address,

    /// The first polygon zkevm.
    #[serde(flatten, with = "prefix_zkevm_1")]
    zkevm_1_input: ZkEvmDeploymentInput,
    #[serde(flatten, with = "prefix_zkevm_1")]
    zkevm_1_output: ZkEvmDeploymentOutput,

    /// The second polygon zkevm.
    #[serde(flatten, with = "prefix_zkevm_2")]
    zkevm_2_input: Option<ZkEvmDeploymentInput>,
    #[serde(flatten, with = "prefix_zkevm_2")]
    zkevm_2_output: Option<ZkEvmDeploymentOutput>,
}

impl DeploymentOutput {
    fn to_dotenv(&self) -> String {
        let mut dotenv = "# Deployment configuration (generated by deploy.rs)\n".to_owned();
        let json = serde_json::to_value(self).unwrap();
        for (key, val) in json.as_object().unwrap() {
            dotenv = format!("{dotenv}{key}={val}\n")
        }
        dotenv
    }
}

/// Deploys the contracts for a demo Polygon ZkEVM.
///
/// For the demo they are considered independent systems so all contracts
/// including the verifier and the bridge contract are deployed once for each
/// node.
async fn deploy_zkevm(
    provider: &Provider<Http>,
    deployer: Arc<Signer>,
    input: &ZkEvmDeploymentInput,
) -> Result<ZkEvmDeploymentOutput> {
    let (_, verifier) = VerifierRollupHelperMock::deploy_contract(&deployer, ()).await;

    let deployer_address = deployer.address();
    let matic_token_initial_balance = parse_ether("20000000")?;
    let (_, matic) = ERC20PermitMock::deploy_contract(
        &deployer,
        (
            "Matic Token".to_string(),
            "MATIC".to_string(),
            deployer_address,
            matic_token_initial_balance,
        ),
    )
    .await;

    // We need to pass the addresses to the GER constructor.
    let nonce = provider
        .get_transaction_count(deployer.address(), None)
        .await?;
    let precalc_bridge_address = get_contract_address(deployer_address, nonce + 1);
    let precalc_rollup_address = get_contract_address(deployer_address, nonce + 2);
    let (_, global_exit_root) = PolygonZkEVMGlobalExitRoot::deploy_contract(
        &deployer,
        (precalc_rollup_address, precalc_bridge_address),
    )
    .await;

    let (_, bridge) = PolygonZkEVMBridge::deploy_contract(&deployer, ()).await;
    assert_eq!(bridge.address(), precalc_bridge_address);

    let ZkEvmDeploymentInput {
        hotshot_address,
        trusted_aggregator,
        genesis_root,
        chain_id,
        fork_id,
        network_name,
        ..
    } = input;

    let (receipt, rollup) = PolygonZkEVM::deploy_contract(
        &deployer,
        (
            global_exit_root.address(),
            matic.address(),
            verifier.address(),
            bridge.address(),
            *hotshot_address,
            *chain_id,
            *fork_id,
        ),
    )
    .await;
    assert_eq!(rollup.address(), precalc_rollup_address);

    let hotshot = HotShot::new(input.hotshot_address, deployer.clone());
    let genesis_hotshot_block_number = hotshot
        .block_height()
        .block(receipt.block_number.unwrap())
        .await?
        .as_u64();
    tracing::info!(
        "HotShot genesis block number: {}",
        genesis_hotshot_block_number
    );

    // Remember the genesis block number where the rollup contract was deployed.
    let genesis_block_number = receipt.block_number.unwrap().as_u64();

    let network_id_mainnet = 0;
    bridge
        .initialize(
            network_id_mainnet,
            global_exit_root.address(),
            rollup.address(),
        )
        .send()
        .await?
        .await?;

    let version = "0.0.1".to_string();
    rollup
        .initialize(
            InitializePackedParameters {
                admin: deployer_address,
                trusted_sequencer: Address::zero(), // Not used.
                pending_state_timeout: 10,
                trusted_aggregator: *trusted_aggregator,
                trusted_aggregator_timeout: 10,
            },
            *genesis_root,
            "http://not-used:1234".to_string(), // Trusted sequencer URL, not used.
            network_name.clone(),
            version,
        )
        .send()
        .await?
        .await?;

    Ok(ZkEvmDeploymentOutput {
        rollup_address: rollup.address(),
        bridge_address: bridge.address(),
        matic_address: matic.address(),
        ger_address: global_exit_root.address(),
        verifier_address: verifier.address(),
        genesis_block_number,
        genesis_hotshot_block_number,
    })
}

pub async fn connect_rpc(
    provider: &Url,
    mnemonic: &str,
    index: u32,
    chain_id: Option<u64>,
    polling_interval: Option<Duration>,
) -> Option<Arc<Signer>> {
    let mut provider = match Provider::try_from(provider.to_string()) {
        Ok(provider) => provider,
        Err(err) => {
            tracing::error!("error connecting to RPC {}: {}", provider, err);
            return None;
        }
    };
    tracing::info!("Connected to RPC {}", provider.url());

    if let Some(interval) = polling_interval {
        provider.set_interval(interval);
    }
    tracing::info!("RPC Polling interval is {:?}", provider.get_interval());

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
    tracing::info!("Chain ID is {}", chain_id);

    let mnemonic = match MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .index(index)
    {
        Ok(mnemonic) => mnemonic,
        Err(err) => {
            tracing::error!("error building walletE: {}", err);
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
    Some(Arc::new(SignerMiddleware::new(provider, wallet)))
}

async fn deploy(opts: Options) -> Result<()> {
    let mut provider = Provider::try_from(opts.provider_url.to_string())?;
    if let Some(interval) = opts.polling_interval {
        provider.set_interval(interval);
    }

    let deployer = connect_rpc(
        &opts.provider_url,
        &opts.mnemonic,
        opts.account_index,
        None,
        opts.polling_interval,
    )
    .await
    .unwrap();
    tracing::info!("Using deployer account {:?}", deployer.address());

    // Deploy the hotshot contract.
    let hotshot_address = if let Some(hotshot_address) = opts.hotshot_address {
        tracing::info!("Using existing HotShot contract at {:?}", hotshot_address);
        hotshot_address
    } else {
        tracing::info!("Deploying HotShot contract");
        let hotshot = HotShot::deploy(deployer.clone(), ())?.send().await?;
        tracing::info!("Deployed HotShot at {:?}", hotshot.address());
        hotshot.address()
    };

    // Deploy the contracts for the first zkevm-node.
    let zkevm_1_input = ZkEvmDeploymentInput {
        hotshot_address,
        trusted_aggregator: opts.trusted_aggregator_1,
        genesis_root: opts.genesis_root_1,
        chain_id: opts.chain_id_1,
        fork_id: 1u64,
        network_name: "zkevm-one".to_string(),
    };
    let zkevm_1_output = deploy_zkevm(&provider, deployer.clone(), &zkevm_1_input).await?;

    let (zkevm_2_input, zkevm_2_output) = if opts.deploy_rollup_2 {
        tracing::info!("Deploying second zkevm-node");
        let zkevm_2_input = ZkEvmDeploymentInput {
            hotshot_address,
            trusted_aggregator: opts.trusted_aggregator_2,
            genesis_root: opts.genesis_root_2,
            chain_id: opts.chain_id_2,
            fork_id: 1u64,
            network_name: "zkevm-two".to_string(),
        };
        let zkevm_2_output = deploy_zkevm(&provider, deployer.clone(), &zkevm_2_input).await?;
        (Some(zkevm_2_input), Some(zkevm_2_output))
    } else {
        tracing::info!("Not deploying second zkevm-node");
        (None, None)
    };
    // Deploy the contracts for the second zkevm-node.

    // Save the output to a file.
    let output = DeploymentOutput {
        hotshot_address,
        zkevm_1_input,
        zkevm_1_output,
        zkevm_2_input,
        zkevm_2_output,
    };

    let content = if opts.json {
        serde_json::to_string_pretty(&output)?
    } else {
        output.to_dotenv()
    };
    tracing::info!("Wrote deployment output to {}", opts.output_path.display());
    std::fs::write(&opts.output_path, content)?;

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    setup_logging();
    setup_backtrace();

    let opts = Options::parse();
    deploy(opts).await
}

#[cfg(test)]
mod test {
    use super::*;
    use async_std::task::sleep;
    use ethers::types::{BlockNumber, TransactionRequest};
    use sequencer_utils::AnvilOptions;
    use tempfile::NamedTempFile;

    async fn wait_for_anvil(opts: &Options) {
        let provider = connect_rpc(
            &opts.provider_url,
            &opts.mnemonic,
            opts.account_index,
            None,
            opts.polling_interval,
        )
        .await
        .unwrap();

        // When we are running a local Anvil node, as in tests, some endpoints (e.g. eth_feeHistory)
        // do not work until at least one block has been mined. Send a transaction to force the
        // mining of a block.
        provider
            .send_transaction(
                TransactionRequest {
                    to: Some(provider.address().into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap()
            .await
            .unwrap();
        // Wait until the fee history endpoint works.
        while let Err(err) = provider.fee_history(1, BlockNumber::Latest, &[]).await {
            tracing::warn!("RPC is not ready: {err}");
            sleep(opts.polling_interval.unwrap_or(Duration::from_millis(10))).await;
        }
    }

    #[async_std::test]
    async fn test_run_deploy_script_two_rollups() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default()
            .block_time(Duration::from_secs(1))
            .spawn()
            .await;
        let mut opts = Options::parse_from([""]);
        opts.polling_interval = Some(Duration::from_millis(10));
        opts.deploy_rollup_2 = true;
        opts.provider_url = anvil.url();
        opts.output_path = NamedTempFile::new()?.path().to_path_buf();

        wait_for_anvil(&opts).await;
        deploy(opts).await?;

        Ok(())
    }

    #[async_std::test]
    async fn test_run_deploy_script_one_rollup() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;
        let mut opts = Options::parse_from([""]);
        opts.polling_interval = Some(Duration::from_millis(10));
        opts.provider_url = anvil.url();
        opts.output_path = NamedTempFile::new()?.path().to_path_buf();

        wait_for_anvil(&opts).await;
        deploy(opts).await?;

        Ok(())
    }
}
