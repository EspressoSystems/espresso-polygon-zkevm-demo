FROM ubuntu:jammy

RUN apt-get update \
    &&  apt-get install -y curl wait-for-it \
    &&  rm -rf /var/lib/apt/lists/*

COPY target/x86_64-unknown-linux-musl/release/polygon-zkevm-adaptor /bin/polygon-zkevm-adaptor
RUN chmod +x /bin/polygon-zkevm-adaptor

CMD [ "/bin/polygon-zkevm-adaptor"]
