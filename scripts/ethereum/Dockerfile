# Builder
FROM trufflesuite/ganache-cli:latest

LABEL maintainer="dev@adex.network"

WORKDIR /scripts

COPY ganache-cli.sh .

EXPOSE 8545

ENTRYPOINT [ "./ganache-cli.sh" ]
