#!/bin/bash
# Check if docker image is running, if not - start it up
echo $(pwd)
[[ $(docker ps -f "name=adex-ganache-cli" --format '{{.Names}}') == "adex-ganache-cli" ]] ||
docker run --rm --name adex-ganache-cli --detach --publish 8545:8545 --volume $(pwd)/scripts:/scripts --entrypoint /scripts/ganache-cli.sh trufflesuite/ganache-cli:latest \
&& sleep 10
