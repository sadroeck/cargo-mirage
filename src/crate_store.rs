use actix_web::{server, App, middleware, Path, fs, http::Method};
use super::config;
use std::io;
use std::io::{BufRead, BufReader};
use std::fs::{OpenOptions, create_dir_all, remove_file, File};
use std::path;
use reqwest;
use futures_cpupool::CpuPool;
use std::sync::mpsc;
use std::thread;
use glob::glob;
use serde_json;

#[derive(Deserialize,Debug,Clone, PartialEq)]
struct CrateMetadata {
    pub name: String,
    pub vers: String,
    pub cksum: String,
    pub yanked: bool,
}

pub fn start(config : &config::CrateStore, registry_uri: &str, crate_download_trigger: mpsc::Receiver<()>) {
    let crate_store_connection_str = config::crate_store_connection_string(&config);
    let folder_for_server = config.folder.clone();
    server::new(move || {
        let folder = folder_for_server.clone();
        App::new()
        .middleware(middleware::Logger::default())
        .resource("/{name}/{version}/download",
            |r| r.method(Method::GET).with(move |args| fetch_crate(&folder, args)))
    })
    .bind(&crate_store_connection_str)
    .expect(&format!("Can not bind to {}", crate_store_connection_str))
    .shutdown_timeout(0)    // <- Set shutdown timeout to 0 seconds (default 60s)
    .workers(16)
    .start();
    println!("Starting crate store on {}", crate_store_connection_str);

    let threadpool = CpuPool::new(10);
    let registry_uri = String::from(registry_uri);
    let folder_for_threadpool = config.folder.clone();
    thread::spawn(move || {
        loop {
            // Block while waiting for trigger
            crate_download_trigger.recv()
            .map_err(|_| eprintln!("Fail to get trigger to download crates"))
            .expect("Could not wait on download trigger");

            println!("Starting fetching crates");            
            let matches = glob(&format!("{}/**/*", registry_uri)).expect("Could not match crate glob pattern");
            matches
            .filter_map(|glob_result| {
                match glob_result {
                    Ok(path) => {
                        if path.is_file() {
                            Some(path)
                        } else {
                            None
                        }
                    },
                    Err(_) => None
                }
            })
            .map(|path| File::open(path).unwrap())
            .map(crates_as_json)
            .for_each(|crate_list| 
                crate_list
                .into_iter()
                .for_each(|crate_entry| {
                    let folder = folder_for_threadpool.clone();
                    threadpool.spawn_fn(move || {
                        download_crate(folder, crate_entry.name, crate_entry.vers, crate_entry.cksum.into_bytes())
                    }).forget();
                }));
        }
    });
}

fn fetch_crate(folder: &str, params: Path<(String, String)>) -> io::Result<fs::NamedFile> {
    let (crate_name, crate_sem_version) = params.into_inner();
    // response
    let crate_uri = format!("{folder}/{name}/{name}-{version}.crate", folder=folder, name=&crate_name, version=&crate_sem_version);
    fs::NamedFile::open(crate_uri)
}

fn crate_exists(folder: &str, name: &str, version: &str) -> bool {
    path::Path::new(
        &format!("{folder}/{name}/{name}-{version}.crate", folder=folder, name=name, version=version))
        .exists()
}

fn download_crate(folder: String, name: String, version: String, _checksum: Vec<u8>) -> Result<(), io::Error> {
    if crate_exists(&folder, &name, &version) {
        return Ok(())
    }

    let name = String::from(name);
    let version = String::from(version);
    let file_uri = format!("{folder}/{name}/{name}-{version}.crate", folder=folder, name=&name, version=&version);
    let file_uri_copy = file_uri.clone();
    let path = path::Path::new(file_uri_copy.as_str());
    let mut file = create_dir_all(path.parent().unwrap())
        .and_then(|()| OpenOptions::new().write(true).create(true).open(path))
        .expect(&format!("Could not open file {}", file_uri));

    reqwest::get(format!("https://crates.io/api/v1/crates/{name}/{version}/download", name=name, version=version).as_str())
    .and_then(move |mut x| x.copy_to(&mut file).map(|_| println!("Downloaded crate {}-{}", name, version)))
    .or_else(|e| {
        eprintln!("Removing file: {:?}", e);
        remove_file(file_uri).map(|_| ())
    })

    // TODO: Use Actix framework for the request. Interpret the "Location" header in the original request and forward
    // to the new static location
    // TODO: Use streaming to allow buffered writing
    /*Arbiter::handle().spawn({
        client::get(format!("https://crates.io/api/v1/crates/{name}/{version}/download", name=name, version=version))
        .finish().unwrap()
        .send()
        .map_err(move |e| eprintln!("Could not download crate {}: {}", err_msg, e))
        .inspect(|_| println!("request sent"))
        .map(move |response| {
            println!("Received response: {:?}", response);
            let file_uri = format!("{folder}/{name}/{name}-{version}.crate", folder=folder, name=&name, version=&version);
            let path = path::Path::new(file_uri.as_str());
            create_dir_all(path.parent().unwrap())
            .and_then(|()| OpenOptions::new().write(true).create(true).open(path))
            .map_err(|e| eprintln!("failed to download: {:?}", e))
            .map(|mut file| {
                // TODO: streamed write
                println!("Polling response");
                response.concat2()
                    .map(move |data| {
                        file.write_all(&data)
                    })
            }).map(|_| {
                // TODO verify checksum
                println!("Downloaded crate");
                ()
            })
        })
        .map(|_| { println!("Downloading crate"); () })
    });*/
}

fn crates_as_json(f: File) -> Vec<CrateMetadata> {
    BufReader::new(f)
    .lines()
    .filter_map(|line| line.ok())
    .map(|line| serde_json::from_str(line.as_str()))
    .filter_map(|x : Result<CrateMetadata,_>| x.ok())
    .collect()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parse_crate_into_json() {
        let crates_string = r#"{"name":"test_crate","vers":"0.0.1","deps":[],"cksum":"aabb","features":{},"yanked":false}"#;
        let res : Result<CrateMetadata, _> = serde_json::from_str(crates_string);
        let metadata = res.unwrap();
        assert_eq!(
            CrateMetadata{ name: String::from("test_crate"), vers: String::from("0.0.1"), cksum: String::from("aabb"), yanked: false},
            metadata);
    }

    #[test]
    fn parse_crates_file_into_json() {
        let file = File::open("test/crate_store/crate_metadata").unwrap();
        let res = crates_as_json(file);
        let expected = vec![
            CrateMetadata{ name: String::from("test_crate"), vers: String::from("0.0.1"), cksum: String::from("aabb"), yanked: false},
            CrateMetadata{ name: String::from("test_crate2"), vers: String::from("0.0.2"), cksum: String::from("aabbb"), yanked: true}];
        assert_eq!(
            expected,
            res);
    }
}
