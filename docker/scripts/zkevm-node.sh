#!/bin/bash
set -eEu -o pipefail

if [[ -v CONFIG_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
  aws secretsmanager  get-secret-value --secret-id ${CONFIG_SECRET_NAME} --query SecretString --output text | tee /app/config.toml >/dev/null
fi

if [[ -v KEYSTORE_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
  aws secretsmanager  get-secret-value --secret-id ${KEYSTORE_SECRET_NAME} --query SecretString --output text | tee /app/genesis.json >/dev/null
fi

if [[ -v GENESIS_S3_URI ]]; then
  echo "Loading config file from AWS secrets manager"
  aws s3 cp $GENESIS_S3_URI /pk/keystore
fi

/app/zkevm-node run --genesis /app/genesis.json --cfg /app/config.toml --components "rpc,synchronizer"
