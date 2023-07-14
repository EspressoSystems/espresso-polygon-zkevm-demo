# First run `scripts/build-l1-image` to build the image locally
FROM ethereum/client-go
ADD .espresso-geth-dev-data-dir/ /geth/
