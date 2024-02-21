# First run `scripts/build-l1-image` to build the image locally
FROM ethereum/client-go:stable
RUN apk add --no-cache curl
ADD .espresso-geth-dev-data-dir/ /geth/
