# Ambire AdEx Validator Stack [![CI](https://github.com/AdExNetwork/adex-validator-stack-rust/workflows/Continuous%20Integration/badge.svg)](https://github.com/AdExNetwork/adex-validator-stack-rust/actions)

Components:

* [Sentry](#sentry)
* [Validator worker](#validator-worker)
* [Adapter](./adapter/README.md) - Ethereum & Dummy (for testing) Adapters
* [AdView manager](./adview-manager/README.md)

## Local & Testing setup

Requirements:

- Rust
  - We target the `stable` version of the Rust compiler.
  - [`cargo-make`](https://github.com/sagiegurari/cargo-make)
- Docker & Docker-compose

#### Linux

- `build-essentials` is required to build the project (error: `linker ``cc`` not found`)
- The crate `openssl-sys` requires `libssl-dev` and `pkg-config` for Ubuntu.

## Sentry

`Sentry` is the REST API that the [`Validator worker`](#validator-worker)
uses for storing and retrieving information.

Two services are needed to run `Sentry`: `Postgres` and `Redis`.

The easiest way to run these services locally is by using the provided `docker-compose` file:

`docker-compose -f ../docker-compose.harness.yml up -d adex-redis adex-postgres`

If you want to run them manually without `docker-compose`:

#### Running Postgres

`docker run --rm --name adex-validator-postgres -e POSTGRES_PASSWORD=postgres -d -p 5432:5432 -v $HOME/docker/volumes/postgres:/var/lib/postgresql/data postgres`

- `$HOME/docker/volumes/postgres` - your local storage for postgres (persist the data when we remove the container)
- `POSTGRES_PASSWORD=postgres` - the password of the default `postgres` user

**NOTE:** Additionally you must setup 2 databases - `sentry_leader` & `sentry_follower` in order for the provided examples below to work. Postgres comes with an environment variable `POSTGRES_DB` that you can use to change the default `postgres` database, but there is currently no way to create multiple using the official `postgres` image.

### Running Redis

`docker run --rm -p 6379:6379 --name adex-validator-redis -d redis`

### Running Sentry Rest API

For a full list of all available CLI options on Sentry run `--help`:

```bash
cargo run -p sentry -- --help
```

Starting the Sentry API in will always run migrations, this will make sure the database is always up to date with the latest migrations, before starting and exposing the web server.

By default, we use the `development` environment ( [`ENV` environment variable](#environment-variables) ) ~~as it will also seed the database~~ (seeding is disabled, see #514).

To enable TLS for the sentry server you need to pass both `--privateKeys` and
`--certificates` cli options (paths to `.pem` files) otherwise the cli will
exit with an error.

For full list of available addresses see [primitives/src/test_util.rs#L39-L118](./primitives/src/test_util.rs#L39-L118)

#### Using the `Ethereum` adapter

The password for the keystore file can be set using the [`KEYSTORE_PWD` environment variable](#adapter).
These examples use the Leader and Follower addresses for testing locally with
`ganache` and the production configuration of the validator.

##### Leader (`0x80690751969B234697e9059e04ed72195c3507fa`)

Sentry API will be accessible at `localhost:8005`

```bash
IP_ADDR=127.0.0.1 REDIS_URL="redis://127.0.0.1:6379/1" \
POSTGRES_DB="sentry_leader" PORT=8005 KEYSTORE_PWD=ganache0 \
cargo run -p sentry -- \
    --adapter ethereum \
    --keystoreFile ./adapter/tests/resources/0x80690751969B234697e9059e04ed72195c3507fa_keystore.json \
    ./docs/config/prod.toml
```

##### Follower (`0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7`)

Sentry API will be accessible at `localhost:8006`

```bash
IP_ADDR=127.0.0.1 REDIS_URL="redis://127.0.0.1:6379/2" \
POSTGRES_DB="sentry_follower" PORT=8006 KEYSTORE_PWD=ganache1 cargo run -p sentry -- \
    --adapter ethereum \
    --keystoreFile ./adapter/test/resources/0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7_keystore.json
    ./docs/config/prod.toml
```

#### Using the `Dummy` adapter

**Dummy** identities:

##### Leader (`0x80690751969B234697e9059e04ed72195c3507fa`)

```bash
IP_ADDR=127.0.0.1 REDIS_URL="redis://127.0.0.1:6379/1" \
POSTGRES_DB="sentry_leader" PORT=8005 cargo run -p sentry -- \
    --adapter dummy \
    --dummyIdentity 0x80690751969B234697e9059e04ed72195c3507fa \
    ./docs/config/prod.toml
```
##### Follower (`0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7`)

```bash
IP_ADDR=127.0.0.1 REDIS_URL="redis://127.0.0.1:6379/2" \
POSTGRES_DB="sentry_follower" PORT=8006 cargo run -p sentry -- \
    --adapter dummy \
    --dummyIdentity 0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7 \
    ./docs/config/prod.toml
```

#### Environment variables

- `ENV` - `production` or `development`; *default*: `development` - passing this env. variable will use the default configuration paths - [`docs/config/dev.toml`](./docs/config/dev.toml) (for `development`) or [`docs/config/prod.toml`](./docs/config/prod.toml) (for `production`). Otherwise you can pass your own configuration file path to the binary (check `cargo run -p sentry --help` for more information). In `development` it will make sure Sentry to seed the database.
- `PORT` - *default*: `8005` - The local port that Sentry API will be accessible at
- `IP_ADDR` - *default*: `0.0.0.0` - the IP address that the API should be listening to

##### Adapter

- `KEYSTORE_PWD` - Password for the `Keystore file`, only available when using `Ethereum` adapter (`--adapter ethereum`)

##### Redis

- `REDIS_URL` - *default*: `redis://127.0.0.1:6379`

##### Postgres

- `POSTGRES_HOST` - *default*: `localhost`
- `POSTGRES_USER` - *default*: `postgres`
- `POSTGRES_PASSWORD` - *default*: `postgres`
- `POSTGRES_DB` - *default*: `user` name - Database name in Postgres to be used for this instance
- `POSTGRES_PORT` - *default*: `5432`


### Validator worker

For a full list of all available CLI options on the Validator worker run `--help`:

```bash
cargo run -p validator_worker -- --help
```

#### Using the `Ethereum` adapter
The password for the Keystore file can be set using the environment variable `KEYSTORE_PWD`.

##### Validator Leader (`0x80690751969B234697e9059e04ed72195c3507fa`)
    Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Leader** on port `8005`:

```bash
KEYSTORE_PWD=ganache0 cargo run -p validator_worker -- \
    --adapter ethereum \
    --keystoreFile ./adapter/test/resources/0x80690751969B234697e9059e04ed72195c3507fa_keystore.json \
    --sentryUrl http://127.0.0.1:8005 \
    ./docs/config/prod.toml
```

##### Validator Follower

Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Follower** on port `8006`:

```bash
KEYSTORE_PWD=ganache1 cargo run -p validator_worker -- \
    --adapter ethereum \
    --keystoreFile ./adapter/test/resources/0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7_keystore.json \
    --sentryUrl http://127.0.0.1:8006 \
    ./docs/config/prod.toml
```

#### Using the `Dummy` adapter

##### Validator Leader (`0x80690751969B234697e9059e04ed72195c3507fa`)

Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Leader** on port `8005`:

```bash
cargo run -p validator_worker -- \
    --adapter dummy \
    --dummyIdentity 0x80690751969B234697e9059e04ed72195c3507fa \
    --sentryUrl http://127.0.0.1:8005 \
    ./docs/config/prod.toml
```

##### Follower: `0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7`

Assuming you have [Sentry API running](#running-sentry-rest-api) for the **Follower** on port `8006`:

```bash
cargo run -p validator_worker -- \
    --adapter dummy \
    --dummyIdentity 0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7 \
    --sentryUrl http://127.0.0.1:8006 \
    ./docs/config/prod.toml
```

#### Environment variables

- `ENV` - `production` or `development` - *default*: `development` - passing this env. variable will use the default configuration paths - [`docs/config/dev.toml`](./docs/config/dev.toml) (for `development`) or [`docs/config/prod.toml`](./docs/config/prod.toml) (for `production`). Otherwise you can pass your own configuration file path to the binary (check `cargo run -p sentry --help` for more information). In `development` it will make sure that Sentry seeds the database.
- `PORT` - The local port that Sentry API will accessible at

##### Adapter

- `KEYSTORE_PWD` - Password for the `Keystore file`, only available when using `Ethereum Adapter` (`--adapter ethereum`)

## Development environment

We use [`cargo-make`][cargo-make overview] for running automated checks
(tests, builds, formatting, code linting, etc.) and building the project locally
as well as on our Continuous Integration (CI).

For a complete list of out-of-the-box commands you can check out the [`Predefined Makefiles`](https://github.com/sagiegurari/cargo-make#usage-predefined-makefiles)
while locally defined commands can be found in the `Makefiles.toml` in each crate directory.

### Local development

It's enough to ensure that the default development command is executing successfully:

```bash
cargo make
```

It will format your code using `rustfmt` and will perform `clippy` checks (it will fail on warnings).
Thanks to `cargo` it will run all the tests (doc tests, unit tests, integration tests, etc.).

Using the provided `docker-compose.harness.yml` setup [`cargo-make`][cargo-make overview] will run
all the required services for the specific crate/application before executing the tests.

[cargo-make overview]: https://github.com/sagiegurari/cargo-make#overview



### License

This project is licensed under the [AGPL-3.0 license](./LICENSE)