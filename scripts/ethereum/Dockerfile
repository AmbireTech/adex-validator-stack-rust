# Builder
FROM trufflesuite/ganache-cli:latest

LABEL maintainer="dev@ambire.com"

WORKDIR /scripts

# The Chain Id that will be passed to `ganache-cli`
ENV CHAIN_ID=
# The snapshot contracts directory that will be used
ARG SNAPSHOT_CONTRACTS=

COPY ganache-cli.sh .
# Copy the SNAPSHOT_CONTRACTS directory to the /scripts/snapshot folder
COPY ${SNAPSHOT_CONTRACTS} ./snapshot

EXPOSE 8545

ENTRYPOINT [ "./ganache-cli.sh" ]
