# Builder
FROM trufflesuite/ganache-cli:latest

LABEL maintainer="dev@ambire.com"

WORKDIR /scripts

COPY ganache-cli.sh .
COPY snapshot_contracts snapshot

EXPOSE 8545

ENTRYPOINT [ "./ganache-cli.sh" ]
