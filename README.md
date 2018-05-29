# cargo-mirage

This is a utility to set up a dedicated [crates.io](https://crates.io) mirror. Cargo can be configured to use the newly created mirror by using Cargo's support for [source
replacement](https://doc.rust-lang.org/cargo/reference/source-replacement.html).

## Installation

### Building from source

This also requires access to crates.io or a local copy of all dependent crates

```sh
cargo build
```

## Example Usage

### Running the mirror

Run `cargo-mirage` on the host where you'd like the mirror to be located.
Configuration can be specified with a `-c <my_config>.toml` command line argument.
If no configuration is specified, a default configuration will be used.

WIP

### Configuring cargo

add this to your .cargo/config for this project:

```toml
[source.crates-io]
replace-with = 'mirage'

[source.mirage]
registry = 'http://<host>:<port>/'
```

## License

This project is licensed under either of

* Apache License, Version 2.0, ([Apache-v2.0](http://www.apache.org/licenses/LICENSE-2.0))
