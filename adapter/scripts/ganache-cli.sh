#!/bin/sh
# We use shell (`/bin/sh`) since the Docker image doesn't contain `/bin/bash`

# runs in Docker, so leave default port and export it instead of setting it up here
node /app/ganache-core.docker.cli.js --gasLimit 0xfffffffffffff \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501200,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501201,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501202,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501203,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501204,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501205,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501206,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501207,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501208,9000000000000000000000000000" \
    --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501209,9000000000000000000000000000"
