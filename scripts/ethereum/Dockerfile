# Builder
FROM trufflesuite/ganache-cli:latest

LABEL maintainer="dev@ambire.com"

WORKDIR /scripts

# The Chain Id that will be passed to `ganache-cli`
ENV CHAIN_ID=

COPY ganache-cli.sh .
# Copy all snapshot_contracts_CHAIN_ID to the /scripts folder
COPY snapshot_contracts_* ./

EXPOSE 8545

ENTRYPOINT [ "./ganache-cli.sh" ]
