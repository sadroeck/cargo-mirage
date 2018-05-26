extern crate actix;
extern crate actix_web;
extern crate futures;
#[macro_use]
extern crate serde_derive;
extern crate toml;
extern crate clap;

mod config;
mod crate_store;

use clap::Arg;

fn parse_command_args() -> clap::ArgMatches<'static> {
    clap::App::new("Cargo mirror")
        .version("1.0")
        .author("Sam De Roeck <sadroeck@gmail.com>")
        .about("Creates a crates.io mirror for both registry and crate storage")
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file")
            .takes_value(true))
        .arg(Arg::with_name("verbose")
            .short("v")
            .multiple(true)
            .help("Sets the level of verbosity"))
        .get_matches()
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    let cmd_args = parse_command_args();
    let config = cmd_args.value_of("config")
        .map_or_else(
            || {
                println!("Using default configuration");
                config::Configuration::default()
            }, config::parse_config);
    
    let sys = actix::System::new("Crates mirror");
    crate_store::start_crate_store(&config.crate_store);
    
    let _ = sys.run();
}
