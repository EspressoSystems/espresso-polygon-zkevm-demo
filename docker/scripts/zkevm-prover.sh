#!/bin/bash
set -eEu -o pipefail

if [[ -v CONFIG_SECRET_NAME ]]; then
  echo "Loading config file from AWS secrets manager"
   aws secretsmanager  get-secret-value --secret-id ${CONFIG_SECRET_NAME} --query SecretString --output text | tee config.json >/dev/null
fi

zkProver -c /app/config.json
