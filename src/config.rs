use std::fs::File;
use std::io::prelude::Read;
use super::config;
use toml;

#[derive(Deserialize,Debug)]
pub struct Configuration {
    pub crate_store: CrateStore,
}

#[derive(Deserialize,Debug)]
#[serde(tag = "type")]
pub enum ListeningInterface {
    Localhost,
    All,
    Custom(String),
}

#[derive(Deserialize,Debug)]
pub struct CrateStore {
    port: i32,
    host: ListeningInterface,    
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            crate_store: CrateStore{
                port: 8080,
                host: ListeningInterface::Localhost,
            }
        }
    }
}

pub fn crate_store_connection_string(crate_store: &CrateStore) -> String {
    let host_str = match crate_store.host {
        ListeningInterface::All => "0.0.0.0",
        ListeningInterface::Localhost => "127.0.0.1",
        ListeningInterface::Custom(ref custom) => custom.as_str(),
    };
    format!("{}:{}", host_str, crate_store.port)
}

pub fn parse_config(config_uri : &str) -> config::Configuration {
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
