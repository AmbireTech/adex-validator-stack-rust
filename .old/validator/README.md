# Validator worker

For more information on the possible options on the binary please run:

`cargo run --bin validator -- --help`

The default mode is running the validator in infinite tick mode, basically constantly waiting and never finishing.

It is possible to run it in single tick mode with the `-s` option

Currently you can run the Validator worker only with DummyAdapter as we do not have any other implementations.

The DummyAdapter requires you to specify the Identity that will be used for the adapter in the form of a string.
You can still pass `-s` for single tick mode.

For tracking issue on the options and the configuration of the validator refer to issue [#68](https://github.com/AdExNetwork/adex-validator-stack-rust/issues/68)