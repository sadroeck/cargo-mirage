use std::fs::File;
use std::io::prelude::Read;
use super::config;
use toml;

#[derive(Deserialize, Serialize, Debug,Clone)]
#[serde(rename = "configuration")]
pub struct Configuration {
    pub crate_store: CrateStore,
    pub crate_registry: CrateRegistry,
}

#[derive(Deserialize, Serialize, Debug,Clone)]
#[serde(tag = "interface", content="interface_str")]
pub enum ListeningInterface {
    #[serde(rename = "localhost")]
    Localhost,
    #[serde(rename = "all")]
    All,
    #[serde(rename = "custom")]
    Custom(String),
}

#[derive(Deserialize, Serialize, Debug,Clone)]
#[serde(rename = "crate_store")]
pub struct CrateStore {
    pub port: i32,
    pub host: ListeningInterface,
    pub folder: String,
    pub workers: i32,
    pub crawlers: i32,
    pub public_host: String,
}

#[derive(Deserialize, Serialize, Debug,Clone)]
#[serde(rename = "crate_registry")]
pub struct CrateRegistry {
    pub uri: String,
    pub update_interval: u32, // In Seconds
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            crate_store: CrateStore{
                port: 8080,
                host: ListeningInterface::Localhost,
                folder: String::from("crates"),
                workers: 16,
                crawlers: 10,
                public_host: String::from("127.0.0.1"),
            },
            crate_registry: CrateRegistry{
                uri: String::from("./crates.io-index"),
                update_interval: 600,
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
