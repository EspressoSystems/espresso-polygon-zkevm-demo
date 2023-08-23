// Copyright (c) 2023 Espresso Systems (espressosys.com)
// This file is part of the Espresso Sequencer-Polygon zkEVM integration demo.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(any(test, feature = "testing"))]
use crate::{Layer1Backend, ZkEvmEnv};
use sequencer_utils::wait_for_rpc;
use std::{path::Path, process::Command, time::Duration};
use zkevm_contract_bindings::TestPolygonContracts;

const L1_SERVICES: [&str; 1] = ["zkevm-mock-l1-network"];

const L2_SERVICES: [&str; 18] = [
    "zkevm-1-prover",
    "zkevm-1-aggregator",
    "zkevm-1-state-db",
    "zkevm-1-permissionless-node",
    "zkevm-1-preconfirmations-prover",
    "zkevm-1-preconfirmations-state-db",
    "zkevm-1-preconfirmations-node",
    "zkevm-1-eth-tx-manager",
    "zkevm-1-faucet",
    "polygon-zkevm-1-adaptor",
    "orchestrator",
    "consensus-server",
    "da-server",
    "sequencer0",
    "sequencer1",
    "sequencer2",
    "sequencer3",
    "sequencer4",
];

pub struct SequencerZkEvmDemoOptions {
    l1_backend: Layer1Backend,
    l1_block_period: Duration,
}

impl Default for SequencerZkEvmDemoOptions {
    fn default() -> Self {
        Self {
            l1_backend: Layer1Backend::Anvil,
            l1_block_period: Duration::from_secs(1),
        }
    }
}

impl SequencerZkEvmDemoOptions {
    pub fn l1_backend(mut self, backend: Layer1Backend) -> Self {
        self.l1_backend = backend;
        self
    }

    pub fn l1_block_period(mut self, period: Duration) -> Self {
        self.l1_block_period = period;
        self
    }

    pub async fn start(self, project_name: String) -> SequencerZkEvmDemo {
        SequencerZkEvmDemo::start_with_sequencer(project_name, self).await
    }
}

/// A zkevm-node inside docker compose with custom contracts
#[derive(Debug, Clone)]
pub struct SequencerZkEvmDemo {
    env: ZkEvmEnv,
    l1: TestPolygonContracts,
    project_name: String,
    layer1_backend: Layer1Backend,
}

impl SequencerZkEvmDemo {
    pub fn env(&self) -> &ZkEvmEnv {
        &self.env
    }

    pub fn l1(&self) -> &TestPolygonContracts {
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
        let work_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        cmd.current_dir(work_dir)
            .arg("compose")
            .arg("--project-name")
            .arg(project_name)
            .arg("-f")
            .arg("permissionless-docker-compose.yaml")
            .arg("-f")
            .arg(layer1_backend.compose_file())
            .args(["-f", "docker-compose.yaml"]);
        cmd
    }

    /// Start the L1, deploy contracts, start the L2
    pub async fn start_with_sequencer(
        project_name: String,
        opt: SequencerZkEvmDemoOptions,
    ) -> Self {
        let env = ZkEvmEnv::from_dotenv();

        tracing::info!("Starting ZkEvmNode with env: {:?}", env);
        tracing::info!(
            "Compose prefix: {:?}",
            Self::compose_cmd_prefix(&project_name, &opt.l1_backend)
        );

        // Remove all existing containers, so that if any configuration has changed since the last
        // time we ran this demo (with this `project_name`) the containers will be rebuilt. We have
        // to do this as a separate step, rather than with the `--force-recreate` option to
        // `docker-compose up`, because we will start the services in two steps: first the L1, then,
        // after deploying contracts, the remaining services. We don't want the second step to force
        // recreation of the L1 after we have deployed contracts to it.
        Self::compose_cmd_prefix(&project_name, &opt.l1_backend)
            .arg("rm")
            .arg("-f")
            .arg("-s")
            .arg("-v")
            .args(L1_SERVICES)
            .args(L2_SERVICES)
            .spawn()
            .expect("Failed to spawn docker-compose rm")
            .wait()
            .expect("Failed to remove old docker containers");

        // Start L1
        Self::compose_cmd_prefix(&project_name, &opt.l1_backend)
            .env(
                "ESPRESSO_ZKEVM_L1_BLOCK_PERIOD",
                opt.l1_block_period.as_secs().to_string(),
            )
            .arg("up")
            .args(L1_SERVICES)
            .arg("-V")
            .arg("--abort-on-container-exit")
            .spawn()
            .expect("Failed to start L1 docker container");

        tracing::info!("Waiting for L1 to start ...");

        wait_for_rpc(&env.l1_provider(), Duration::from_millis(200), 100)
            .await
            .unwrap();

        tracing::info!("L1 ready");

        // Use a dummy URL for the trusted sequencer since we're not running one anyways.
        let l1 = TestPolygonContracts::deploy(&env.l1_provider(), "http://dummy:1234").await;

        // Start zkevm-node
        Self::compose_cmd_prefix(&project_name, &opt.l1_backend)
            .env(
                "ESPRESSO_ZKEVM_1_ROLLUP_ADDRESS",
                format!("{:?}", l1.rollup.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_1_MATIC_ADDRESS",
                format!("{:?}", l1.matic.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_1_GER_ADDRESS",
                format!("{:?}", l1.global_exit_root.address()),
            )
            .env(
                "ESPRESSO_SEQUENCER_HOTSHOT_ADDRESS",
                format!("{:?}", l1.hotshot.address()),
            )
            .env(
                "ESPRESSO_ZKEVM_1_GENESIS_BLOCK_NUMBER",
                l1.gen_block_number.to_string(),
            )
            .arg("up")
            .args(L2_SERVICES)
            .arg("-V")
            .arg("--no-recreate")
            .arg("--abort-on-container-exit")
            .spawn()
            .expect("Failed to start compose environment");

        wait_for_rpc(&env.l2_provider(), Duration::from_secs(1), 200)
            .await
            .expect("Failed to start zkevm-node");
        wait_for_rpc(
            &env.l2_preconfirmations_provider(),
            Duration::from_secs(1),
            200,
        )
        .await
        .expect("Failed to start preconfirmations node");

        Self {
            env,
            project_name,
            l1,
            layer1_backend: opt.l1_backend,
        }
    }

    fn stop(&self) -> &Self {
        Self::compose_cmd_prefix(self.project_name(), self.layer1_backend())
            .arg("down")
            .arg("-v")
            .arg("--remove-orphans")
            .spawn()
            .expect("Failed to run docker compose down")
            .wait()
            .unwrap_or_else(|err| panic!("Failed to stop demo {}: {err}", self.project_name()));
        self
    }
}

impl Drop for SequencerZkEvmDemo {
    fn drop(&mut self) {
        self.stop();
    }
}
