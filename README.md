# AdEx Validator Stack in Rust [![Build Status](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust.svg?token=TBKq9g6p9sWDrzNyX4kC&branch=master)](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust)

The Rust implementation of the Validator Stack

Reference implementation of the [AdEx validator stack](https://github.com/adexnetwork/adex-protocol#validator-stack-platform).

Components:

* Domain crate
* Sentry - check the list of [opened issues](https://github.com/AdExNetwork/adex-validator-stack-rust/issues?q=is:open+is:issue+project:AdExNetwork/adex-validator-stack-rust/1)
* Validator worker - The validator worker(`Leader` or `Follower`) that validates/proposes new states.
* memory-repository - Generic helper crate for creating InMemory repositories for testing.
* adapter - Adapter trait for `sign`, `verify` and `validate_channel` with Dummy implementation for testing.

**Note:** Please refer to the README.md of the component for a more detailed overview of it.

## Domain
Contains all the Domain `Aggregates`, `Entities`, `Value Objects`, interfaces (traits) and `Domain Error`.
The interfaces(traits) include  The `RepositoryFuture` and the `Aggregates`/`Entities` traits for the repositories.

All the structs have defined (de)serialization. The also have incorporated domain rules, e.g.
`TargetingTag.score` (the `Score` struct) should be with a value between `0` and `100`.
This means that once we have a `Score` object, we are guaranteed to have a valid object everywhere.

The `Repository` traits are meant to help you create the correct abstractions in the underlying application,
as every application has different requirements for the way and things it will fetch.

## Sentry & Validator worker

Split into 3 layer - Domain, Infrastructure & Application.
- Domain - the domain objects/structs that are defining the business rules and constraints.
- Infrastructure - specific implementations of e.g. Repositories, Logging and etc.
like `Memory__Repository`, `Api__Repository` and so on.
- Application - all the application specific logic, which means services, structs and etc. that use the Domain and it's
traits to achieve the task at hand. For example: In sentry we have the `resource`s, there we define the
`channel_create`. Which handles the request, validates it and uses the `ChannelRepository` trait to
`add` the new Channel and returns the appropriate Response. It is not however limited to Request -> Response.

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

### Run Validator Worker

`cargo run --bin validator`

For the available options:

`cargo run --bin validator -- --help`

#### Environment variables:

**NOTE: Currently we use `.env` file to define values for those environment variables.
We need to see if we want configuration files per binary instead.**

##### Sentry: 
- `DATABASE_URL` - The url of the Postgres database used for production.
- `SENTRY_CHANNEL_LIST_LIMIT` - the limit per page for listing channels from the `/channel/list` request.

##### Validator:
- `VALIDATOR_TICKS_WAIT_TIME` - The time for a whole cycle(tick) of the validator worker to get & loop channels,
validate and send statuses and etc.
- `VALIDATOR_SENTRY_URL` - The url of the Sentry API that should be used
- `VALIDATOR_VALIDATION_TICK_TIMEOUT` - The maximum time for validation of a single channel as a `Leader` or `Follower`

## Development environment

We use [cargo-make](https://github.com/sagiegurari/cargo-make) for running the checks and build project locally
as well as on CI. For a complete list of out-of-the-box commands you can check
[Makefile.stable.toml](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml).

Locally it's enough to ensure that `cargo make` command (it will execute the default dev. command) is passing.
It will run `rustfmt` for you, it will fail on `clippy` warnings and it will run all the tests.

You can related to the [Makefile.stable.toml](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml)
for more commands and cargo-make as a whole.
