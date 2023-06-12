#![cfg(any(test, feature = "testing"))]
use crate::{Layer1Backend, ZkEvmEnv};
use sequencer_utils::wait_for_rpc;
use std::{process::Command, time::Duration};
use zkevm_contract_bindings::TestHermezContracts;

/// A zkevm-node inside docker compose with custom contracts
#[derive(Debug, Clone)]
pub struct SequencerZkEvmDemo {
    env: ZkEvmEnv,
    l1: TestHermezContracts,
    project_name: String,
    layer1_backend: Layer1Backend,
}

impl SequencerZkEvmDemo {
    pub fn env(&self) -> &ZkEvmEnv {
        &self.env
    }

    pub fn l1(&self) -> &TestHermezContracts {
        &self.l1
    }

    pub fn project_name(&self) -> &String {
        &self.project_name
    }

    pub fn layer1_backend(&self) -> &Layer1Backend {
        &self.layer1_backend
    }

    pub(crate) fn compose_cmd_prefix(
        project_name: &str,
        layer1_backend: &Layer1Backend,
    ) -> Command {
        let mut cmd = Command::new("docker");
        cmd.arg("compose")
            .arg("--project-name")
            .arg(project_name)
            .arg("-f")
            .arg("permissionless-docker-compose.yaml")
            .arg("-f")
            .arg(layer1_backend.compose_file());
        cmd
    }

    /// Start the L1, deploy contracts, start the L2
    pub async fn start_with_sequencer(project_name: String, layer1_backend: Layer1Backend) -> Self {
        // Add a unique number to `project_name` to ensure that all instances use a unique name.

        let env = ZkEvmEnv::from_dotenv();

        tracing::info!("Starting ZkEvmNode with env: {:?}", env);
        tracing::info!(
            "Compose prefix: {:?}",
            Self::compose_cmd_prefix(&project_name, &layer1_backend)
        );

        // Start L1
        Self::compose_cmd_prefix(&project_name, &layer1_backend)
            .arg("up")
            .arg("zkevm-mock-l1-network")
            .arg("-V")
            .arg("--force-recreate")
            .arg("--abort-on-container-exit")
            .spawn()
            .expect("Failed to start L1 docker container");

        tracing::info!("Waiting for L1 to start ...");

        wait_for_rpc(&env.l1_provider(), Duration::from_millis(200), 100)
            .await
            .unwrap();

        tracing::info!("L1 ready");

        // Use a dummy URL for the trusted sequencer since we're not running one anyways.
        let l1 = TestHermezContracts::deploy(&env.l1_provider(), "http://dummy:1234").await;

        // Start zkevm-node
        Self::compose_cmd_prefix(&project_name, &layer1_backend)
            .env(
                "ESPRESSO_ZKEVM_ROLLUP_ADDRESS",
                format!("{:?}", l1.rollup.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_MATIC_ADDRESS",
                format!("{:?}", l1.matic.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_GER_ADDRESS",
                format!("{:?}", l1.global_exit_root.address()),
            )
            .env(
                "ESPRESSO_SEQUENCER_HOTSHOT_ADDRESS",
                format!("{:?}", l1.hotshot.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_GENBLOCKNUMBER",
                l1.gen_block_number.to_string(),
            )
            .args(["-f", "docker-compose.yaml"])
            .arg("up")
            .args([
                "zkevm-prover",
                "zkevm-aggregator",
                "zkevm-state-db",
                "zkevm-permissionless-node",
                "zkevm-eth-tx-manager",
                "hermez-adaptor",
                "cdn-server",
                "sequencer0",
                "sequencer1",
                "sequencer2",
                "sequencer3",
                "sequencer4",
            ])
            .arg("-V")
            .arg("--force-recreate")
            .arg("--abort-on-container-exit")
            .spawn()
            .expect("Failed to start compose environment");

        wait_for_rpc(&env.l2_provider(), Duration::from_secs(1), 100)
            .await
            .expect("Failed to start zkevm-node");

        Self {
            env,
            project_name,
            l1,
            layer1_backend,
        }
    }

    fn stop(&self) -> &Self {
        Self::compose_cmd_prefix(self.project_name(), self.layer1_backend())
            .arg("down")
            .arg("-v")
            .arg("--remove-orphans")
            .spawn()
            .expect("Failed to run docker compose down");
        self
    }
}

impl Drop for SequencerZkEvmDemo {
    fn drop(&mut self) {
        self.stop();
    }
}
