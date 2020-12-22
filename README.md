# AdEx Validator Stack in Rust [![Build Status](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust.svg?token=TBKq9g6p9sWDrzNyX4kC&branch=master)](https://travis-ci.com/AdExNetwork/adex-validator-stack-rust)

The Rust implementation of the Validator Stack

Reference implementation of the [AdEx validator stack](https://github.com/adexnetwork/adex-protocol#validator-stack-platform).

Components:

* [Sentry](#sentry)
* [Validator worker](#validator-worker)
* Adapter - Ethereum & Dummy (for testing) Adapters
* AdView manager

## Local & Testing setup

Requirements:

- Rust

  Check the [`rust-toolchain`](./rust-toolchain) file for specific version of rust.
  - [`cargo-make`](https://github.com/sagiegurari/cargo-make)
- Docker

#### Linux
- `build-essentials` is required to build the project (error: `linker ``cc`` not found`)
- The crate `openssl-sys` requires `libssl-dev` and `pkg-config` for Ubuntu.

## Sentry

`Sentry` is the REST API that the [`Validator worker`](#validator-worker) uses for storing and retrieving information.
We need two services to be able to run `Sentry`: `Postgres` and `Redis`.

### Running Postgres

`docker run --rm --name adex-validator-postgres -e POSTGRES_PASSWORD=postgres -d -p 5432:5432 -v $HOME/docker/volumes/postgres:/var/lib/postgresql/data postgres`

- `$HOME/docker/volumes/postgres` - your local storage for postgres (persist the data when we remove the container)
- `POSTGRES_PASSWORD=postgres` - the password of `postgres` user

### Running Redis

`docker run --rm -p 6379:6379 --name adex-validator-redis -d redis`

### Running Sentry Rest API

For a full list of all available CLI options on Sentry run `--help`:

```bash
cargo run -p sentry -- --help
```

Starting the Sentry API in will always run migrations, this will make sure the database is always up to date with the latest migrations, before starting and exposing the web server.

In `development` ( [`ENV` environment variable](#environment-variables) ) it will seed the database as well.

#### Using the `Ethereum Adapter`

The password for the Keystore file can be set using the [environment variable `KEYSTORE_PWD`](#adapter).

- Leader
    ```bash
    POSTGRES_DB="sentry_leader" PORT=8006 cargo run -p sentry -- \
        --adapter ethereum \
        --keystoreFile ./adapter/resources/keystore.json \
        ./docs/config/dev.toml
    ```

- Follower
    ```bash
    POSTGRES_DB="sentry_follower" PORT=8006 cargo run -p sentry -- \
        --adapter ethereum \
        --keystoreFile ./adapter/resources/keystore.json
        ./docs/config/dev.toml
    ```

#### Using the `Dummy Adapter`

**Dummy** identities:

- Leader: `ce07CbB7e054514D590a0262C93070D838bFBA2e`

```bash
    POSTGRES_DB="sentry_leader" PORT=8005 cargo run -p sentry -- \
        --adapter dummy \
        --dummyIdentity ce07CbB7e054514D590a0262C93070D838bFBA2e \
        ./docs/config/dev.toml
```
- Follower: `c91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3`

```bash
    POSTGRES_DB="sentry_follower" PORT=8006 cargo run -p sentry -- \
        --adapter dummy \
        --dummyIdentity c91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3 \
        ./docs/config/dev.toml
```

For full list, check out [primitives/src/util/tests/prep_db.rs#L29-L43](./primitives/src/util/tests/prep_db.rs#L29-L43)

#### Environment variables

- `ENV` - `production` or `development`; *default*: `development` - passing this env. variable will use the default configuration paths - [`docs/config/dev.toml`](./docs/config/dev.toml) (for `development`) or [`docs/config/prod.toml`](./docs/config/prod.toml) (for `production`). Otherwise you can pass your own configuration file path to the binary (check `cargo run -p sentry --help` for more information). In `development` it will make sure Sentry to seed the database.
- `PORT` - *default*: `8005` - The local port that Sentry API will be accessible at
- `ANALYTICS_RECORDER` - accepts any non-zero value - whether or not to start the `Analytics recorder` that will track analytics stats for payout events (`IMPRESSION` & `CLICK`)
##### Adapter
- `KEYSTORE_PWD` - Password for the `Keystore file`, only available when using `Ethereum Adapter` (`--adapter ethereum`)

##### Redis
- `REDIS_URL` - *default*: `redis://127.0.0.1:6379`

##### Postgres
- `POSTGRES_HOST` - *default*: `localhost`
- `POSTGRES_USER` - *default*: `postgres`
- `POSTGRES_PASSWORD` - *default*: `postgres`
- `POSTGRES_DB` - *default*: `user` name - Database name in Postgres to be used for this instance
- `POSTGRES_PORT` - *default*: `5432`

#####

### Running the Validator Worker

For a full list of all available CLI options on the Validator worker run `--help`:

```bash
cargo run -p validator_worker -- --help
```

#### Using the `Ethereum Adapter`
TODO: Update Keystore file and Keystore password for Leader/Follower as they are using the same at the moment.

The password for the Keystore file can be set using the environment variable `KEYSTORE_PWD`.

- Leader
    Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Leader** on port `8005`:

    ```bash
    cargo run -p validator_worker
        --adapter ethereum
        --keystoreFile ./adapter/resources/keystore.json
        --sentryUrl http://127.0.0.1:8005
        ./docs/config/dev.toml
    ```

- Follower

    Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Follower** on port `8006`:

    ```bash
    cargo run -p validator_worker
        --adapter ethereum
        --keystoreFile ./adapter/resources/keystore.json
        --sentryUrl http://127.0.0.1:8006
        ./docs/config/dev.toml
    ```

#### Using the `Dummy Adapter`
- Leader: `ce07CbB7e054514D590a0262C93070D838bFBA2e`

    Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Leader** on port `8005`:

    ```bash
    cargo run -p validator_worker
        --adapter dummy
        --dummyIdentity ce07CbB7e054514D590a0262C93070D838bFBA2e
        --sentryUrl http://127.0.0.1:8005
        ./docs/config/dev.toml
    ```

- Follower: `c91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3`

    Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Follower** on port `8006`:

    ```bash
    cargo run -p validator_worker
        --adapter dummy
        --dummyIdentity c91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3
        --sentryUrl http://127.0.0.1:8006
        ./docs/config/dev.toml
    ```

#### Environment variables

- `ENV`: `production` or `development` ( *default* ) - passing this env. variable will use the default configuration paths - [`docs/config/dev.toml`](./docs/config/dev.toml) (for `development`) or [`docs/config/prod.toml`](./docs/config/prod.toml) (for `production`). Otherwise you can pass your own configuration file path to the binary (check `cargo run -p sentry --help` for more information). In `development` it will make sure Sentry to seed the database.
- `PORT` - The local port that Sentry API will accessible at

##### Adapter
- `KEYSTORE_PWD` - Password for the `Keystore file`, only available when using `Ethereum Adapter` (`--adapter ethereum`)

## Development environment

We use [`cargo-make`](https://github.com/sagiegurari/cargo-make#overview) for running automated checks (tests, builds, formatting, code linting, etc.) and building the project locally
as well as on our Continuous Integration (CI). For a complete list of out-of-the-box commands you can check
[Makefile.stable.toml](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml).

### Local development

Locally it's enough to ensure that the default development command is executing successfully:

```bash
cargo make
```

It will run `rustfmt` for you as well as `clippy` (it will fail on warnings) and it will run all the tests thanks to `cargo` (doc tests, unit tests, integration tests, etc.).

This will also run the [Automated tests](#automated-tests), so you must have `Redis` & `Postgres` running.

#### Automated tests

This requires [`cargo-make`](https://github.com/sagiegurari/cargo-make#overview) and since we have integration tests that require `Redis` ([see `Running Redis`](#running-redis)) & `Postgres` (see [`Running Postgres`](#running-postgres)), you need to be running those in order to run the automated tests:

`cargo make test`

You can relate to the [`Makefile.stable.toml`](https://github.com/sagiegurari/cargo-make/blob/master/src/lib/Makefile.stable.toml)
for more commands and cargo-make as a whole.
