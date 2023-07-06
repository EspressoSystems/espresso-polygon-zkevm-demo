# Espresso Sequencer - Polygon zkEVM - Integration Demo

This repo contains a demo where Polygon zkEVM's sequencing and data availability
(DA) are provided by the Espresso Sequencer & DA instead of Polygon zkEVM's
sequencer and Ethereum L1 DA.

![Architecture diagram](./doc/architecture_phase2.svg)

## Development

- Obtain code: `git clone --recursive git@github.com:EspressoSystems/espresso-polygon-zkevm-demo`.
- Make sure [nix](https://nixos.org/download.html) is installed.
- Activate the environment with `nix-shell`, or `nix develop`, or `direnv allow`
  if using [direnv](https://direnv.net/).

## Run the tests

    just pull # to pull docker images
    cargo test --all-features

## Building figures

    make doc

### Running with Docker

#### Authenticate with GitHub container registry

- Go to your github profile
- Developer Settings > Personal access tokens > Personal access tokens (classic)
- Generate a new token
  - for the scope options of the token, tick the _repo_ box.
- Run `docker login ghcr.io --username <you_github_id> --password <your_personal_access_token>`

#### Run the demo

To get the latest images: `just pull`

To start the demo: `just demo`. Note: currently broken due to failing genesis
check in the synchronizer.

To start the demo in the background: `just demo-background`. This can be useful because the command should exit sucessfully only once the demo is running.

To stop the demo: `just down`

To build the docker images locally: `just build-docker`. To revert to the CI docker images: `just pull`.

#### Run the integration tests

- `cargo test --all-features end_to_end`

### Running natively

Build all executables with `cargo build --release`. You may then start a single CDN server and
connect as many sequencer nodes as you'd like. To start the CDN, choose a port `$PORT` to run it on
and decide how many sequencer nodes `$N` you will use, then run
`target/release/cdn-server -p $PORT -n $N`.

The sequencer will distribute a HotShot configuration to all the nodes which connect to it, which
specifies consensus parameters like view timers. There is a default config, but you can override any
parameters you want by passing additional options to the `cdn-server` executable. Run
`target/release/cdn-server --help` to see a list of available options.

Once you have started the CDN server, you must connect `$N` sequencer nodes to it, after which the
network will start up automatically. To start one node, run
`target/release/sequencer --cdn-url tcp://localhost:$PORT`. A useful Bash snippet for running `$N`
nodes simultaneously in the background of your shell is:

```bash
for i in `seq $N`; do
    target/release/sequencer --cdn-url tcp://localhost:$PORT &
done
```

Note: if the sequencer shows a `"Connection refused"` error you may need to use
`127.0.0.1` instead of `localhost` when connecting to the CDN. This is because
`localhost` may resolve to `::1` if dual stack (ipv4 and ipv6) networking is
enabled.

### Developing contracts

### Working on Polygon zkEVM contracts in zkevm-contracts

- Ensure submodules are checkout out: `git submodule update --init --recursive`
- Install dependencies `just npm i`
- Compile the contracts `just hardhat compile`
- Update the rust bindings: `just update-contract-bindings`
- Update the zkevm-node contract bindings to match zkevm-contracts: `just
update-zkevm-node-contract-bindings`

### Handling git submodules

The project requires to use git submodules.
In order to avoid corrupting the state of one of those submodules you can:

- run `git submodule update` before making changes,
- or configure git to automatically update submodules for the repository with `git config submodule.recurse true`
  inside the repository.

## Implementation Plan

We will work towards the architecture illustrated above in three phases.

### Phase I: Basic Sequencing (done)

Replace the Polygon zkEVM trusted sequencer with a HotShot-based permissionless sequencer.

### Phase II: Off-Chain Data Availability (done)

Only store batch commitments, not full batches, in the rollup contract. Use HotShot for data
availability.

### Phase III: Final Integration

Move adaptor service into zkEVM node for a smoother integration.
