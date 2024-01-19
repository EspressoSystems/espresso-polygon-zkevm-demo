# The zkevm-node docker-compose file currently only works if run from the zkevm-node/test directory.
compose-base := "docker compose --project-name demo -f standalone-docker-compose.yaml"
compose-anvil := compose-base + " -f docker-compose-anvil.yaml"
compose := compose-base + " -f docker-compose-geth.yaml"

default:
    just --list

zkevm-node:
    cargo run --all-features --bin zkevm-node

demo *args: (demo-profiles "zkevm1" "zkevm1-preconfirmations" args)

deploy-contracts:
    cargo run --bin deploy -- --hotshot-address 0x0116686e2291dbd5e317f47fadbfb43b599786ef --polling-interval 1000 --account-index 19

deploy-rollup *args: deploy-contracts
    docker compose --profile zkevm1 --profile zkevm1-preconfirmations --env-file .env --env-file deployment.env up {{args}}

demo-profiles *args:
    scripts/check-architecture

    # When we build the L1 image locally, we create an env file with information about the
    # deployment. However, if we are using the image pulled from the container registry, this
    # generated file may not exist. For the following command we need to ensure the file exists, but
    # it is ok if it is empty -- we will just use the defaults from .env.
    touch .env.geth

    # The files .env and .env.geth both contain important information (.env for defaults, .env.geth
    # for environment variables that change when we rebuild the Geth image, such as block period and
    # contract addresses). Newer versions of docker-compose (>= 2.17) allow you to override select
    # variables from .env using `--env-file <another-env>`, but older versions ignore all but the
    # last file passed with `--env-file`. Docker Desktop for Mac comes with docker-compose 2.2, and
    # there doesn't seem to be an obvious way to update it. So for convenience, we support older
    # versions by manually sourcing .env.geth, overriding any variables which were also set in .env.
    scripts/source-dotenv .env.geth scripts/demo-with-profiles {{args}}

down *args:
   {{compose}} down --remove-orphans {{args}}

pull:
    {{compose-anvil}} pull && {{compose}} pull

    # We are now using the default Geth image, so clear information generated when we built a local
    # version of the image.
    rm -f .env.geth

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
    cd zkevm-node && nix develop -c bash -c "make build-docker && docker tag zkevm-node:latest ghcr.io/espressosystems/zkevm-node:release-gibraltar"

build-docker-zkevm-adaptor:
  scripts/build-docker-images

build-docker-l1-geth:
    scripts/build-l1-image
    docker build -t ghcr.io/espressosystems/espresso-polygon-zkevm-demo/geth-with-contracts:release-gibraltar  -f docker/geth.Dockerfile .

build-docker: build-docker-l1-geth build-docker-zkevm-node build-docker-zkevm-adaptor

test:
    cargo test --release --all-features
