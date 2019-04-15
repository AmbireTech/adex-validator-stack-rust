# adex-validator-stack-rust

Rust implementation of the Validator Stack

Reference implementation of the [AdEx validator stack](https://github.com/adexnetwork/adex-protocol#validator-stack-platform).

Components:

* Sentry - TODO
* Validator worker - TODO

## Sentry: API

#### Do not require authentication, can be cached:

GET `/channel/list` - get a list of all channels - TODO

## Testing setup

### Rust setup

Currently requires `rustup override set nightly-2019-04-07` as `futures-preview` fails to build

#### Linux
- The crate `openssl-sys` requires `libssl-dev` and `pkg-config` for Ubuntu.

### Run Postgres

`docker run --rm  --name pg-docker -e POSTGRES_PASSWORD=docker -d -p 5432:5432 -v $HOME/docker/volumes/postgres:/var/lib/postgresql/data postgres`

- `$HOME/docker/volumes/postgres` - your local storage for postgres (persist the data when we remove the container)
- `POSTGRES_PASSWORD=docker` - the password of `postgres` user
