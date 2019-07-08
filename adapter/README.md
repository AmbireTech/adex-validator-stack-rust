# Adapter crate

Adapter trait and a DummyAdapter implementation for testing.
It is checking the "sanity" of a channel by checking some rules we have in order for a Channel to
be valid (`validate_channel()`).
The Adapter can also `sign()` and `validate()` `StateRoot`s and can provide the Authentication by which
it can be recognized.

## Features

### DummyAdapter (`dummy-adapter`)

When you enable this feature you get an access to the DummyAdapter implementation, which you can use for testing.