# AdEx Validator Stack in Rust [![Build Status](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust.svg?token=TBKq9g6p9sWDrzNyX4kC&branch=master)](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust)

The Rust implementation of the Validator Stack

Reference implementation of the [AdEx validator stack](https://github.com/adexnetwork/adex-protocol#validator-stack-platform).

Components:

* Sentry
* Validator worker
* Adapter
* AdView manager

## Local & Testing setup

#### Linux
- `build-essentials` is required to build the project (error: `linker ``cc`` not found`)
- The crate `openssl-sys` requires `libssl-dev` and `pkg-config` for Ubuntu.

### Run Postgres

`docker run --rm  --name pg-docker -e POSTGRES_PASSWORD=docker -d -p 5432:5432 -v $HOME/docker/volumes/postgres:/var/lib/postgresql/data postgres`

- `$HOME/docker/volumes/postgres` - your local storage for postgres (persist the data when we remove the container)
- `POSTGRES_PASSWORD=docker` - the password of `postgres` user

### Run Redis:

`docker run --name some-redis -d redis`

### Run automated tests

Since we have integration tests that require Redis & Postgres,
you need to be running those in order to run the automated tests:

`cargo make test`

### Run Sentry Rest API

* With the DummyAdapter(replace the `DummyIdentity`):

`export PORT=8006; cargo run -p sentry -- -a dummy -i DummyIdentity`

* With the EthereumAdapter:

TODO

### Run Validator Worker

TODO

## Development environment

We use [cargo-make](https://github.com/sagiegurari/cargo-make) for running the checks and build project locally
as well as on CI. For a complete list of out-of-the-box commands you can check
[Makefile.stable.toml](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml).

Locally it's enough to ensure that `cargo make` command (it will execute the default dev. command) is passing.
It will run `rustfmt` for you, it will fail on `clippy` warnings and it will run all the tests.

*Note:* You need to have setup Redis and Postgres as well.

You can related to the [Makefile.stable.toml](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml)
for more commands and cargo-make as a whole.
