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
