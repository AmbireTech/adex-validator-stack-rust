# Builder
FROM trufflesuite/ganache-cli:latest

LABEL maintainer="dev@ambire.com"

WORKDIR /scripts

# The Chain Id that will be passed to `ganache-cli`
ENV CHAIN_ID=

COPY ganache-cli.sh .
COPY snapshot_contracts_${CHAIN_ID} snapshot

EXPOSE 8545

ENTRYPOINT [ "./ganache-cli.sh" ]
