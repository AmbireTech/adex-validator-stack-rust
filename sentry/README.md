# Sentry

## REST API documentation for AdEx Protocol V5

For full details see [AIP #61](https://github.com/AmbireTech/aips/issues/61) and the tracking issue for this implementation https://github.com/AmbireTech/adex-validator-stack-rust/issues/377.

REST API documentation can be generated using `rustdoc`:

`cargo doc --all-features --lib --no-deps --open`

and checking the `sentry::routes` module.

## Development

### Migrations
While you can create the migration files yourself, you can also use `migrant`
to create them for you.

#### Migrant
1) In order to use `migrant` you need to first install it:

`cargo install migrant --features postgres`

2) And setup the `Migrant.toml` file, a reference to which you can find
in [Migrant.dist.toml](Migrant.dist.toml).

For more options see the [migrant homepage](https://github.com/jaemk/migrant)

#### Benchmarks

Starts the `Sentry` application along side `redis` and `postgres` databases,
and runs `wrk2` on the POST `/v5/campaign/0xXXXX../events` route for 3 campaigns:

```bash
cargo make run-benchmark
```

### License

This project is licensed under the [AGPL-3.0 license](./LICENSE)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in AdEx Validator by you, shall be licensed as AGPL-3.0, without any additional terms or conditions.

#### Sign the CLA
When you contribute to a AdEx Validator open source project on GitHub with a new pull request, a bot will evaluate whether you have signed the CLA. If required, the bot will comment on the pull request, including a link to this system to accept the agreement. 
