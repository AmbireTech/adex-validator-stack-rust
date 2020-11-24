# Builder
FROM rust:1.48.0 as builder

LABEL maintainer="dev@adex.network"

WORKDIR /usr/src/app

COPY . .

# We intall the validator_worker binary with all features in Release mode
# Inlcude the full backtrace for easier debugging
RUN RUST_BACKTRACE=full cargo install --path validator_worker --all-features

WORKDIR /usr/local/bin

RUN cp $CARGO_HOME/bin/validator_worker .

FROM ubuntu:20.04

RUN apt update && apt-get install -y libssl-dev ca-certificates

# `ethereum` or `dummy`
ENV ADAPTER=

# only applicable if you use the `--adapter ethereum`
ENV KEYSTORE_FILE=
ENV KEYSTORE_PWD=

# Only applicable if you use the `--adapter dummy`
ENV DUMMY_IDENTITY=

# If set it will override the configuration file used
ENV CONFIG=
# Defaults to `http://127.0.0.1:8005`
ENV SENTRY_URL=
# Set to any value to run the `validator_worker` in `single tick` mode
# default: `infinite`
ENV SINGLE_TICK=

WORKDIR /usr/local/bin

COPY docs/config/cloudflare_origin.crt /usr/local/share/ca-certificates/

RUN update-ca-certificates

COPY --from=builder /usr/local/bin/validator_worker .

CMD validator_worker -a ${ADAPTER:-ethereum} \
            ${KEYSTORE_FILE:+-k $KEYSTORE_FILE} \
            ${DUMMY_IDENTITY:+-i $DUMMY_IDENTITY} \
            ${SINGLE_TICK:+-t} \
            ${SENTRY_URL:+-u $SENTRY_URL} ${CONFIG}
