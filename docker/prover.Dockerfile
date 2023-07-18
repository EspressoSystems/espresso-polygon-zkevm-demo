FROM hermeznetwork/zkevm-prover:88f3835
RUN apt-get update \
    &&  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends awscli curl wait-for-it \
    &&  rm -rf /var/lib/apt/lists/*
ADD ./zkevm-node/test/config/test.prover.1.config.json /app/config.json
ADD ./docker/scripts/zkevm-prover.sh /usr/local/bin/zkevm-prover.sh
CMD ["bash","/usr/local/bin/zkevm-prover.sh"]
