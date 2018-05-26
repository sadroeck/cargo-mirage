use actix_web::{server, App, middleware, Path, fs, http::Method};
use super::config;
use std::io;

pub fn start_crate_store(config: &config::CrateStore) {
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

fn fetch_crate(params: Path<(String, String)>) -> io::Result<fs::NamedFile> {
    let (crate_name, crate_sem_version) = params.into_inner();
    // response
    let crate_uri = format!("{}/{}-{}.crate", &crate_name, &crate_name, &crate_sem_version);
    fs::NamedFile::open(crate_uri)
}