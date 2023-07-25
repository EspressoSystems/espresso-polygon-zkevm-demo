#!/bin/bash
set -eEu -o pipefail

if [[ -v CONFIG_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
  aws secretsmanager  get-secret-value --secret-id ${CONFIG_SECRET_NAME} --query SecretString --output text | tee /app/config.toml >/dev/null
fi

if [[ -v AGGREGATOR_KEYSTORE_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
  aws secretsmanager  get-secret-value --secret-id ${AGGREGATOR_KEYSTORE_SECRET_NAME} --query SecretString --output text | tee /pk/aggregator.keystore >/dev/null
fi


if [[ -v SEQUENCER_KEYSTORE_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
  aws secretsmanager  get-secret-value --secret-id ${SEQUENCER_KEYSTORE_SECRET_NAME} --query SecretString --output text | tee /pk/sequencer.keystore >/dev/null
fi

if [[ -v GENESIS_S3_URI ]]; then
  echo "Loading config file from AWS secrets manager"
  aws s3 cp $GENESIS_S3_URI /app/genesis.json
fi

# $COMPONENTS
# Permissionless node "rpc,synchronizer"
# aggregator "aggregator"
# eth-tx-manager "eth-tx-manager"

/app/zkevm-node run --genesis /app/genesis.json --cfg /app/config.toml --components "$COMPONENTS"
