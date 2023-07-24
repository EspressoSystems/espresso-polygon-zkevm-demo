FROM ghcr.io/espressosystems/zkevm-node:hotshot-integration
RUN apk add aws-cli bash curl
ADD ./zkevm-node/test/sequencer.keystore /pk/keystore
ADD ./zkevm-node/test/config/test.node.config.toml /app/config.toml
ADD ./zkevm-node/test/config/test.genesis.config.json /app/genesis.json
ADD ./docker/scripts/zkevm-node.sh /usr/local/bin/zkevm-node.sh
CMD ["bash","/usr/local/bin/zkevm-node.sh"]
