# Espresso Sequencer - Polygon zkEVM - Integration Demo

This repo contains a demo where two rollups based on the Polygon zkEVM stack use the Espresso Sequencer
and Data Availability (DA) instead of the Polygon zkEVM Sequencer and Ethereum
L1 as DA.

The repo consists mainly of rust code, docker services and end-to-end tests
to tie together the following code bases:

- The [Espresso Sequencer](
https://github.com/EspressoSystems/espresso-sequencer).
- A fork of Polygon's
[zkevm-node](https://github.com/0xPolygonHermez/zkevm-node) used as submodule at
[./zkevm-node](zkevm-node).
- A fork of Polygon's
[zkevm-contracts](https://github.com/0xPolygonHermez/zkevm-contracts) used as
submodule at [./zkevm-contracts](zkevm-contracts).

The diagram below shows the architecture. Note that only one of the zkEVM nodes
is depicted for simplicity. The diagram is intended to give a simple conceptual
overview, there may be slight discrepancies in how individual components are
depicted and implemented.

![Architecture diagram](./doc/architecture_phase2.svg)

# Usage

- To get the latest images: `just pull`
- To start the demo: `just demo`.
- To stop the demo: `just down`

## Metamask
- If not yet set up, install [Metamask](https://metamask.io/) and set up a new
  wallet.
- In metamask click on the three dots in the top right corner, then "Expand view".
- On the newly opened page click on the three dots in the top right corner, then
  "Networks" -> "Add a network" -> "Add a network manually".

Use the following parameters:

- Network name: espresso-polygon-zkevm-1
- New RPC URL: http://localhost:18126
- Chain ID: 1001

For interacting with the second rollup add a network with these parameters instead:

- Network name: espresso-polygon-zkevm-2
- New RPC URL: http://localhost:28126
- Chain ID: 1002

For "Currency symbol" anything can be set and "Block explorer URL" should be
left blank.

## Faucet
To request funds from the local faucet run

```
curl -X POST http://localhost:18111/faucet/request/0x0000000000000000000000000000000000000000
```

replacing the zero address with the desired receiver address. Use
http://localhost:28111 to talk to the faucet of the second node instead.

To copy your Metamask address click on the address at the top of the Metamask panel.

## Preconfirmations

If you tried the demo as instructed above, using the RPC nodes at ports 18126 and 28126, you may
notice some latency (perhaps around 30s) between confirming a transaction in Metamask and having the
transaction complete. In large part, this latency is because of the path the transaction has to
follow between being submitted and being finalized. Refer to
[the architecture diagram](doc/architecture_phase2.svg) and you'll see that, after a transaction has
been sequenced, the HotShot Commitment Service must pick it up and send it to the L1 blockchain, it
must be included in the L1, which can be slow, the L2 node must pick it up from the HotShot
Contract, and only then can the L2 node fetch your transaction from the sequencer query service,
execute it, and report the results to Metamask.

This long round trip through the L1 slows down the transaction a lot, especially on a real L1 like
Ethereum, with a block time of 12 seconds and a finality time of 12 minutes. (For the demo, we use a
block time of 1 second, and the latency is still noticeable!) One of the advantages of using a
decentralized consensus protocol as the sequencer is that it can provide _preconfirmations_:
ahead-of-time assurances that your transaction will be included in the rollup, before the L1 has
even seen that transaction. Since HotShot is a _Byzantine fault tolerant_ consensus protocol, these
preconfirmations come with a strong guarantee: any transaction which is preconfirmed by HotShot will
eventually make it to the L1 (in the same order relative to other transactions), unless at least 1/3
of the total value staked in consensus is corrupt and subjected to slashing. In the right
conditions, this guarantee can be as strong as the safety guarantees of the L1 itself, especially
when
[restaking](https://hackmd.io/@EspressoSystems/EspressoSequencer#3-Interactions-with-Ethereum-Validators-via-Restaking)
is used to let the L1 validators themselves operate the consensus protocol.

Our demo includes a second L2 node for each L2 network, which is configured to fetch new blocks
directly from the sequencer's preconfirmations, bypassing the L1 completely. You can experience the
results for yourself: go back into your Metmask settings, to
"Networks" -> "espresso-polygon-zkevm-1", and set "New RPC URL" to `http://localhost:18127`.
Similarly, set the RPC URL for "espresso-polygon-zkevm-2" to `http://localhost:28127`. This tells
Metamask to use the L2 nodes that use preconfirmations, instead of the L2 nodes that use the L1,
when submitting and tracking transactions.

Once you've updated your settings, you can go back to your Metamask account and try making another
transfer. It should complete noticeably faster, in around 5 seconds.

## Changing the L1 Block Time

For convenience, this demo uses a local L1 blockchain with a block time of 1 second. This is good
for experimentation, but it tends to understate the benefits of preconfirmations. In reality, the L1
will be a blockchain like Ethereum with a substantially longer block time. Ethereum produces a new
block only once every 12 seconds, and blocks do not become final for 12 _minutes_ (until that point
they can be re-orged out of the chain).

If you really want to feel the UX difference between having preconfirmations and not having them,
you can rebuild the local L1 Docker image to use a different block time, and then rerun the above
experiment, using Metamask with ports 18126 and 18127 to try sending transactions without and with
preconfirmations, respectively.

To rebuild the L1 Docker image and change the block time, use the following command:

```bash
ESPRESSO_ZKEVM_L1_BLOCK_PERIOD=12 just build-docker-l1-geth
```

You can replace `12` with whatever block time you want (in seconds).

When you're done and just want things to go fast again, use `just build-docker-l1-geth` to revert to
the default (1 second), or `just pull` to sync all your Docker images with the official, default
versions.

## Hardware Requirements

The demo requires an Intel or AMD CPU. It's currently not possible to run this demo on ARM
architecture CPUs, including Macs with M1 or M2 CPUs. See
[this issue](https://github.com/0xPolygonHermez/zkevm-prover/issues/235) for more information. You
can however run the
[example rollup demo](https://github.com/EspressoSystems/espresso-sequencer/tree/main/example-l2)
of the Espresso Sequencer.

Because the demo runs 4 zkEVM nodes (and thus 4 zk provers) it has fairly substantial resource
requirements. We recommend at least the following:

- 6 GB RAM
- 6 CPUs
- 50 GB storage

On Mac, where Docker containers run in a virtualized Linux environment, you may have to take manual
steps to allow Docker to access these resources, even if your hardware already has them. Open Docker
Desktop and in the left sidebar click on Resources. There you can configure the amount of CPUs, RAM,
and storage allotted to the Linux VM.

## Lightweight Modular Demo

Since the full demo has such intense resource requirements, we have designed the demo to be modular,
so you can get a much lighter version of it by starting only some of the services. To run a
lightweight version of the demo, use `just demo-profiles <modules>`, replacing `<modules>` with the
list of modules you want to start. The available modules are:

- `zkevm1`: start a regular node and prover for the first L2
- `zkevm1-preconfirmations`: start a node for the first L2 that uses fast preconfirmations
- `zkevm2`: start a regular node and prover for the second L2
- `zkevm2-preconfirmations`: start a node for the second L2 that uses fast preconfirmations

For example, to experiment with and without preconfirmations without the overhead of a second L2,
you could run `just demo-profiles zkevm1 zkevm1-preconfirmations`. If you want to try out multiple
simultaneous L2s but don't want the overhead of the secondary preconfirmations nodes, you could use
`just demo-profiles zkevm1 zkevm2`.

# Development

- Obtain code: `git clone --recursive git@github.com:EspressoSystems/espresso-polygon-zkevm-demo`.
- Make sure [nix](https://nixos.org/download.html) is installed.
- Activate the environment with `nix-shell`, or `nix develop`, or `direnv allow`
  if using [direnv](https://direnv.net/).
- Run `just` to see the available just recipes.

To know more about the environment check out the following files

- [.env](.env): Environment variables
- [docker-compose.yaml](docker-compose.yaml): Espresso Sequencer services
- [permissionless-docker-compose.yaml](permissionless-docker-compose.yaml): Polygon zkEVM services

Another good place to start is the end-to-end test in [polygon-zkevm-adaptor/tests/end_to_end.rs](polygon-zkevm-adaptor/tests/end_to_end.rs).

## Test
To run the tests, run

    just pull # to pull docker images
    cargo test --all-features

## Figures
To build the figures, run

    make doc

## Contracts

- Ensure submodules are checkout out: `git submodule update --init --recursive`
- Install dependencies `just npm i`
- Compile the contracts `just hardhat compile`
- Update the rust bindings: `just update-contract-bindings`
- Update the zkevm-node contract bindings to match zkevm-contracts: `just
update-zkevm-node-contract-bindings`

### Misc
#### Building docker images locally
- Build the docker images locally: `just build-docker`.
- Revert to the CI docker images: `just pull`.

#### Authenticate with GitHub container registry
This is only required to download "private" docker images from the GitHub container registry.

- Go to your github profile
- Developer Settings > Personal access tokens > Personal access tokens (classic)
- Generate a new token
  - for the scope options of the token, tick the _repo_ box.
- Run `docker login ghcr.io --username <you_github_id> --password <your_personal_access_token>`

#### Handling git submodules

The project requires to use git submodules. In order to avoid corrupting the
state of one of those submodules you can:

- run `git submodule update` before making changes,
- or configure git to automatically update submodules for the repository with
  `git config submodule.recurse true` inside the repository.

# Disclaimer

**DISCLAIMER:** This software is provided "as is" and its security has not been externally audited. Use at your own risk.
