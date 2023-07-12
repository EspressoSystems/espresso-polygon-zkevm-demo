# First run `scripts/build-l1-image` to build the image locally
#
# For building locally the `docker build` step needs to run as root because geth
# creates files with 600 permissions.
#
# E.g. sudo docker build -f docker/geth.Dockerfile .
#
FROM ethereum/client-go
ADD .espresso-geth-dev-data-dir/ /geth/
