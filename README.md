# cargo-mirage

NOTE: This is a Work-In-Progress. It works as an MVP, but might require some tweaks to run in all environments. Many features are still missing. A list in order of priority can be found below.

This is a utility to set up a dedicated [crates.io](https://crates.io) mirror. Cargo can be configured to use the newly created mirror by using Cargo's support for [source
replacement](https://doc.rust-lang.org/cargo/reference/source-replacement.html).

## Features

- [x] HTTP Crate download server
- [x] Registry with custom configuration
- [x] Background crate crawler & downloaders
- [ ] Official crate on crates.io
- [ ] Pre-built binaries
- [ ] Configurable logging & logging-levels
- [ ] Custom upstream crates.io sources
- [ ] HTTPS support
- [ ] Git daemonization ( currently has to be set up manually )
- [ ] Yanked crate handling
- [ ] Private crates (not in crates.io registry)
- [ ] Simplify dependencies & decrease build times

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

### Configuring cargo-mirage

```toml
[crate_registry]
update_interval = 600 # Monitoring interval for upstream crates.io-index changes - in seconds
uri = "<local crates.io git repo location>"

[crate_store]
crawlers = 10 # number of crate downloaders
folder = "<local folder where to store crates>"
port = 8080 # port where to host the crate serving mirror
workers = 16 # number of crate store server threads
public_host = "the.public.ip.of.myserver.com | 10.1.2.3"

[crate_store.host]
interface = "localhost | all | custom"
interface_str = "<interface spec in case of custom>"
```

### Configuring cargo

add this to your .cargo/config for this project:

```toml
[source.crates-io]
replace-with = "mirage"

[source.mirage]
registry = "http://<host>:<port>/"
```

## License

This project is licensed under

- Apache License, Version 2.0, ([Apache-v2.0](http://www.apache.org/licenses/LICENSE-2.0))
