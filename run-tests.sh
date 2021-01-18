#!/bin/sh

function setup() {
    cd ./lib/validator && npm install
}

SUBCOMMAND=$1

# set rust validator worker env var
export RUST_VALIDATOR_WORKER=./target/debug/validator_worker 

# RUN Tests
if [ "$SUBCOMMAND" == "--setup" ]; then
    echo "Setup validator tests"
    setup    
else
    cargo build -p validator_worker
    docker-compose -f docker-compose.dev.yml up

    echo "Running route and integration tests"
    ./lib/validator/test/routes.js #&& ./lib/validator/test/integration.js
fi

exitCode=$?

# clean up
if [ "$SUBCOMMAND" != "--setup" ]; then
    docker-compose -f docker-compose.dev.yml down
fi

# stop docker
exit $exitCode
