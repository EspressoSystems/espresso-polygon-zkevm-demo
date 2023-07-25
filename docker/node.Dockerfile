FROM ghcr.io/espressosystems/zkevm-node:hotshot-integration
RUN apk add aws-cli bash curl
ADD ./zkevm-node/test/aggregator.keystore /pk/aggregator.keystore
ADD ./zkevm-node/test/sequencer.keystore /pk/sequencer.keystore
ADD ./zkevm-node/test/config/test.node.config.toml /app/config.toml
ADD ./zkevm-node/test/config/test.genesis.config.json /app/genesis.json
ADD ./docker/scripts/zkevm-node.sh /usr/local/bin/zkevm-node.sh
# $COMPONENTS
# Permissionless node "rpc,synchronizer" (DEFAULT)
# aggregator "aggregator"
# eth-tx-manager "eth-tx-manager"
ENV COMPONENTS="rpc,synchronizer"
CMD ["bash","/usr/local/bin/zkevm-node.sh"]
