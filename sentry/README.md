# Sentry

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