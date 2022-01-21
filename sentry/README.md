# Sentry

## REST API documentation for AdEx Protocol V5

For full details see [AIP #61](https://github.com/AmbireTech/aips/issues/61) and the tracking issue for this implementation https://github.com/AmbireTech/adex-validator-stack-rust/issues/377.

REST API documentation can be generated using `rustdoc`:

`cargo doc --all-features --open --lib`

and checking the `sentry::routes` module.

## Development

### Migrations
While you can create the migration files yourself, you can also use `migrant`
to create them for you.

#### Migrant
1) In order to use `migrant` you need to first install it:

    * via cargo: `cargo install migrant --features postgres`
    
    (for more options see the [migrant homepage](https://github.com/jaemk/migrant))

2) And setup the `Migrant.toml` file, a reference to which you can find
in [Migrant.dist.toml](Migrant.dist.toml).

For more information see [migrant homepage](https://github.com/jaemk/migrant).