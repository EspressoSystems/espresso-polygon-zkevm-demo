# The zkevm-node docker-compose file currently only works if run from the zkevm-node/test directory.
compose-cmd := "docker compose --project-name demo"
compose-base := compose-cmd + " -f docker-compose.yaml -f permissionless-docker-compose.yaml"
compose-espresso := compose-cmd + " -f docker-compose.yaml"
compose-anvil := compose-base + " -f docker-compose-anvil.yaml"
compose := compose-base + " -f docker-compose-geth.yaml"
compose-zkevm-node := compose-cmd + " -f permissionless-docker-compose.yaml -f docker-compose-geth.yaml"

default:
    just --list

zkevm-node:
    cargo run --all-features --bin zkevm-node

demo *args:
   {{compose}} up {{args}}

down:
   {{compose}} down

pull:
    {{compose-anvil}} pull && {{compose}} pull

hardhat *args:
    cd zkevm-contracts && nix develop -c bash -c "npx hardhat {{args}}"

update-contract-bindings:
    cargo run --bin gen-bindings

update-zkevm-node-contract-bindings:
    scripts/update-zkevm-node-contract-bindings

npm *args:
   cd zkevm-contracts && nix develop -c bash -c "npm {{args}}"

compose *args:
   {{compose}} {{args}}

docker-stop-rm:
    docker stop $(docker ps -aq); docker rm $(docker ps -aq)

anvil *args:
    docker run ghcr.io/foundry-rs/foundry:latest "anvil {{args}}"

build-docker-zkevm-node:
    cd zkevm-node && nix develop -c bash -c "make build-docker && docker tag zkevm-node:latest ghcr.io/espressosystems/zkevm-node:hotshot-integration"

build-docker-l1-geth:
    scripts/build-l1-image
    docker build -t ghcr.io/espressosystems/espresso-polygon-zkevm-demo/geth-with-contracts:main  -f docker/geth.Dockerfile .

build-docker: build-docker-l1-geth build-docker-zkevm-node

test:
    cargo test --release --all-features
