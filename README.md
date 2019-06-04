# AdEx Validator Stack in Rust [![Build Status](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust.svg?token=TBKq9g6p9sWDrzNyX4kC&branch=master)](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust)

The Rust implementation of the Validator Stack

Reference implementation of the [AdEx validator stack](https://github.com/adexnetwork/adex-protocol#validator-stack-platform).

Components:

* Domain crate
* Sentry - check the list of [opened issues](https://github.com/AdExNetwork/adex-validator-stack-rust/issues?q=is:open is:issue project:AdExNetwork/adex-validator-stack-rust/1)
* Validator worker - TODO

## Domain
Contains all the Domain `Aggregates`, `Entities`, `Value Objects`, interfaces (traits) and `Domain Error`.
The interfaces(traits) include  The `RepositoryFuture` and the `Aggregates`/`Entities` traits for the repositories.

All the structs have defined (de)serialization. The also have incorporated domain rules, e.g.
`TargetingTag.score` (the `Score` struct) should be with a value between `0` and `100`.
This means that once we have a `Score` object, we are guaranteed to have a valid object everywhere.

The `Repository` traits are meant for retrieving the underlying object types, this includes implementations with
Databases (like `Postgres` for `Sentry`), API calls (for the `Validator` to fetch the objects from `Sentry`),
memory (for testing) and etc.

## Sentry: API

#### Do not require authentication, can be cached:

The API documentation can be found on the [adex-validator](https://github.com/AdExNetwork/adex-validator/blob/master/docs/api.md).
Currently implemented endpoints:

- POST `/channel` - creates a new channel
- GET `/channel/list` - get a list of all channels

## Validator worker

TODO

## Testing setup

### Rust setup

- Requires `nightly 2019-05-08`, because of the new syntax for `await` and our `tower-web` dependency fails to build.
We've setup `rust-toolchain` but you can manually override it as well with `rustup override set nightly-2019-05-08`.

#### Linux
- The crate `openssl-sys` requires `libssl-dev` and `pkg-config` for Ubuntu.

### Run Postgres

`docker run --rm  --name pg-docker -e POSTGRES_PASSWORD=docker -d -p 5432:5432 -v $HOME/docker/volumes/postgres:/var/lib/postgresql/data postgres`

- `$HOME/docker/volumes/postgres` - your local storage for postgres (persist the data when we remove the container)
- `POSTGRES_PASSWORD=docker` - the password of `postgres` user

### Run Sentry Rest API

`DATABASE_URL=postgresql://postgres:docker@localhost:5432/sentry cargo run --bin sentry`

#### Environment variables:

- `DATABASE_URL` - The url of the Postgres database used for production.

**NOTE: For development & testing purposes we use `.env` file to define values for those environment variables.**

- `CHANNEL_LIST_LIMIT` - the limit per page for listing channels from the `/channel/list` request.
