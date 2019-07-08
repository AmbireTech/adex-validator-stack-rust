# `Domain` crate for validator/sentry stack

This crate is meant to hold all Domain structures required for both the Validator and Sentry.
It defines all the structs(Entities, Value Objects and etc.), Domain errors and the generic Repository
types and Errors.
It also defines the serialization and deserialization of them.

The Repository traits and the actual implementations of them is left to the underlying usage.
The traits can have different interfaces based on the requirements of the application and can have
different implementation based on the needs, whether this is: Postgres, InMemory, RestAPI and etc. implementations.

### Features:

#### Repositories

If the usage of this crate, requires not only (de)serialization, but also retrieving or storing
the objects in any way (database, API calls, memory and etc.) you would need this feature, to use
the generic types and traits to define the underling Repository interfaces for the domain objects
and implementations.

The trait `RepositoryFuture` with a generic `RepositoryError` is used as the common return type for
repository methods.

#### Fixtures

For testing purposes there are set of fixtures found under the `fixtures` module,
to easily create valid objects in UnitTests.

There is also the `domain::test_util` module that gives you some handy functions
for usage in tests:

- `take_one` which gives you a random element from a slice.
- `time::datetime_between`
- `time::past_datetime`

### Utils

The `domain::util` has some domain utilities.

- `ts_milliseconds_option` allows you to have `Option<DateTime<Utc>>` that is
(de)serialized into Milliseconds Timestamp.
