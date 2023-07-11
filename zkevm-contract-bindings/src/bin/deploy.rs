use anyhow::Result;
use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
use clap::Parser;
use contract_bindings::HotShot;
use ethers::{
    prelude::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::Address,
    utils::{get_contract_address, parse_ether},
};
use hex::{FromHex, FromHexError};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;
use std::path::PathBuf;
use std::{sync::Arc, time::Duration};
use url::Url;
use zkevm_contract_bindings::{
    erc20_permit_mock::ERC20PermitMock, polygon_zk_evm_bridge::PolygonZkEVMBridge,
    polygon_zk_evm_global_exit_root::PolygonZkEVMGlobalExitRoot,
    shared_types::InitializePackedParameters,
    verifier_rollup_helper_mock::VerifierRollupHelperMock, Deploy, PolygonZkEVM,
};

pub type EthMiddleware = SignerMiddleware<Provider<Http>, LocalWallet>;

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

    /// The URL of an Ethereum JsonRPC where the contracts will be deployed.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_WEB3_PROVIDER_URL",
        default_value = "http://localhost:8545"
    )]
    pub provider_url: Url,

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

    /// Output file path where deployment info will be stored.
    #[arg(
        long,
        env = "ESPRESSO_ZKEVM_DEPLOY_OUTPUT",
        default_value = "deployment.json"
    )]
    pub output_path: PathBuf,
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
    global_exit_root_address: Address,
    /// The address of the verifier contract.
    verifier_address: Address,
    /// The block number when the rollup contract was deployed.
    genesis_block_number: u64,
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
    zkevm_2_input: ZkEvmDeploymentInput,
    #[serde(flatten, with = "prefix_zkevm_2")]
    zkevm_2_output: ZkEvmDeploymentOutput,
}

/// Deploys the contracts for a demo Polygon ZkEVM.
///
/// For the demo they are considered independent systems so all contracts
/// including the verifier and the bridge contract are deployed once for each
/// node.
async fn deploy_zkevm(
    provider: &Provider<Http>,
    deployer: Arc<EthMiddleware>,
    input: &ZkEvmDeploymentInput,
) -> Result<ZkEvmDeploymentOutput> {
    let (_, verifier) = VerifierRollupHelperMock::deploy_contract(&deployer, ()).await;

    let matic_token_initial_balance = parse_ether("20000000")?;
    let (_, matic) = ERC20PermitMock::deploy_contract(
        &deployer,
        (
            "Matic Token".to_string(),
            "MATIC".to_string(),
            deployer.address(),
            matic_token_initial_balance,
        ),
    )
    .await;

    // We need to pass the addresses to the GER constructor.
    let nonce = provider
        .get_transaction_count(deployer.address(), None)
        .await?;
    let precalc_bridge_address = get_contract_address(deployer.address(), nonce + 1);
    let precalc_rollup_address = get_contract_address(deployer.address(), nonce + 2);
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
                admin: deployer.address(),
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
        global_exit_root_address: global_exit_root.address(),
        verifier_address: verifier.address(),
        genesis_block_number,
    })
}

async fn deploy(opts: Options) -> Result<()> {
    let mut provider = Provider::try_from(opts.provider_url.to_string())?;
    provider.set_interval(Duration::from_millis(100));
    let chain_id = provider.get_chainid().await?.as_u64();
    let deployer = Arc::new(SignerMiddleware::new(
        provider.clone(),
        MnemonicBuilder::<English>::default()
            .phrase(opts.mnemonic.as_str())
            .index(0u32)?
            .build()?
            .with_chain_id(chain_id),
    ));

    // Deploy the hotshot contract.
    let hotshot = HotShot::deploy(deployer.clone(), ())?.send().await?;
    let hotshot_address = hotshot.address();
    tracing::info!("Deployed HotShot at {:?}", hotshot.address());

    // Deploy the contracts for the first zkevm-node.
    let zkevm_1_input = ZkEvmDeploymentInput {
        hotshot_address,
        trusted_aggregator: opts.trusted_aggregator_1,
        genesis_root: opts.genesis_root_1,
        chain_id: 1001u64,
        fork_id: 1u64,
        network_name: "zkevm-one".to_string(),
    };
    let zkevm_1_output = deploy_zkevm(&provider, deployer.clone(), &zkevm_1_input).await?;

    // Deploy the contracts for the second zkevm-node.
    let zkevm_2_input = ZkEvmDeploymentInput {
        hotshot_address,
        trusted_aggregator: opts.trusted_aggregator_2,
        genesis_root: opts.genesis_root_2,
        chain_id: 1002u64,
        fork_id: 1u64,
        network_name: "zkevm-two".to_string(),
    };
    let zkevm_2_output = deploy_zkevm(&provider, deployer.clone(), &zkevm_2_input).await?;

    // Save the output to a file.
    let output = DeploymentOutput {
        hotshot_address,
        zkevm_1_input,
        zkevm_1_output,
        zkevm_2_input,
        zkevm_2_output,
    };

    let data = serde_json::to_string_pretty(&output)?;
    std::fs::write(&opts.output_path, data)?;
    tracing::info!("Wrote deployment output to {}", opts.output_path.display());

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
    use sequencer_utils::AnvilOptions;
    use tempfile::NamedTempFile;

    #[async_std::test]
    async fn test_run_deploy_scripts() -> Result<()> {
        setup_logging();
        setup_backtrace();

        let anvil = AnvilOptions::default().spawn().await;
        let mut opts = Options::parse_from([""]);
        opts.provider_url = anvil.url();
        opts.output_path = NamedTempFile::new()?.path().to_path_buf();

        deploy(opts).await?;

        Ok(())
    }
}
