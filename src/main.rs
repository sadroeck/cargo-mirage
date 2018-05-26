extern crate actix;
extern crate actix_web;
extern crate futures;
#[macro_use]
extern crate serde_derive;
extern crate toml;
extern crate clap;

mod config;

use actix_web::{server, App, middleware, Path, fs, http::Method};
use clap::Arg;
use std::io;
use std::fs::File;
use std::io::prelude::Read;

fn fetch_crate(params: Path<(String, String)>) -> io::Result<fs::NamedFile> {
    let (crate_name, crate_sem_version) = params.into_inner();
    // response
    let crate_uri = format!("{}/{}-{}.crate", &crate_name, &crate_name, &crate_sem_version);
    fs::NamedFile::open(crate_uri)
}

fn parse_config(config_uri : &str) -> config::Configuration {
    let cfg_str = File::open(config_uri)
        .and_then(|mut file| {
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map(|_| contents)
        })
        .expect(&format!("Could not open {}", &config_uri));
    
    toml::from_str::<config::Configuration>(cfg_str.as_str())
        .expect(&format!("Could not parse as configuration: {}", cfg_str))
}

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

fn start_crate_store(config: &config::CrateStore) {
    let crate_store_connection_str = config::crate_store_connection_string(&config);    
    server::new(|| {
        App::new()
        .middleware(middleware::Logger::default())
        .resource("/{name}/{version}/download", |r| r.method(Method::GET).with(fetch_crate))
    })
    .bind(&crate_store_connection_str)
    .expect(&format!("Can not bind to {}", crate_store_connection_str))
    .shutdown_timeout(0)    // <- Set shutdown timeout to 0 seconds (default 60s)
    .workers(16)
    .start();
    println!("Starting crate store on {}", crate_store_connection_str);
}

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    let cmd_args = parse_command_args();
    let config = cmd_args.value_of("config")
        .map_or_else(
            || {
                println!("Using default configuration");
                config::Configuration::default()
            }, parse_config);
    
    let sys = actix::System::new("Crates mirror");
    start_crate_store(&config.crate_store);
    
    let _ = sys.run();
}
