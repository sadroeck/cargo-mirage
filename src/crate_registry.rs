use git2::{Repository, Direction, Signature, Commit, Error, ObjectType, BranchType, MergeAnalysis, AnnotatedCommit};
use super::config;
use std::thread;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use git_utils;
use std::fs::OpenOptions;
use serde_json;

const OFFICIAL_CRATES_REGISTRY : &str = "https://github.com/rust-lang/crates.io-index.git";
const CARGO_SIG_AUTHOR : &str = "Cargo mirage";
const CARGO_SIG_EMAIL : &str = "cargo@mirage.io";

#[derive(Serialize,Deserialize,Clone,Debug, PartialEq)]
struct CratesIOConfig {
    pub dl: String,
    pub api: String,
}

enum MergeAction<'a> {
    FastForward,
    Normal(AnnotatedCommit<'a>),
    Nop,
}

fn merge_analysis_to_action<'a>(merge_analysis: MergeAnalysis, commit: AnnotatedCommit<'a>) -> MergeAction {
    if merge_analysis.contains(MergeAnalysis::ANALYSIS_FASTFORWARD) {
        println!("Fast-forward merge of remote changes");
        MergeAction::FastForward
    } else if merge_analysis.contains(MergeAnalysis::ANALYSIS_NORMAL) {
        println!("Merging remote changes");
        MergeAction::Normal(commit)
    } else {
        println!("Repo is up-to-date");
        MergeAction::Nop
    }
}

fn find_head_commit(repo: &Repository) -> Result<Commit, Error> {
    let obj = repo.head()?.resolve()?.peel(ObjectType::Commit)?;
    obj.into_commit().map_err(|_| Error::from_str("Couldn't find commit"))
}

fn find_remote_master_tip(repo : &Repository) -> Result<Commit, Error> {
    repo.find_branch("origin/master", BranchType::Remote)
        .and_then(|branch| branch.into_reference().peel(ObjectType::Commit))
        .and_then(|obj| obj.into_commit()
            .map_err(|e| Error::from_str(format!("Could find remote master tip: {:?}", e).as_str())))
}

fn merge_upstream_master(repo: &Repository) {
    git_utils::clean_working_dir(repo)
    .and_then(|()| find_remote_master_tip(repo))
    .and_then(|remote_commit| repo.find_annotated_commit(remote_commit.id()))
    .and_then(|remote| {
        repo.merge_analysis(&[&remote])
            .map(|(analysis, _)| (remote.id(), merge_analysis_to_action(analysis, remote)))
    })
    .and_then(|(remote_id, action)| {
        match action {
            MergeAction::FastForward => git_utils::fast_forward_merge(&repo, remote_id),
            MergeAction::Nop => Ok(None),
            MergeAction::Normal(remote_commit) => git_utils::force_merge_remote_commit(&repo, remote_commit),
        }
    })
    .and_then(|annotated_remote_commit_opt| {
        match annotated_remote_commit_opt {
            Some(annotated_remote_commit) => repo.find_commit(annotated_remote_commit.id()).map(|x| Some(x)),
            None => Ok(None),
        }
    })
    .and_then(|remote_commit_opt| {
        match remote_commit_opt {
            Some(remote_commit) => {
                let mut index = repo.index().expect("Could not retrieve git index");
                index.write_tree()
                .and_then(|oid| { repo.find_tree(oid) })
                .and_then(|tree| {
                    let signature = Signature::now(CARGO_SIG_AUTHOR, CARGO_SIG_EMAIL)
                        .expect("Could not create signature");
                    let parent_commit = find_head_commit(&repo)?;
                    repo.commit(Some("HEAD"), //  point HEAD to our new commit
                        &signature, // author
                        &signature, // committer
                        "Merge crates.io-index master", // commit message
                        &tree, // tree
                        &[&parent_commit, &remote_commit]) // parents
                })
                .map(|_| ())
            },
            None => Ok(()),
        }
    })
    .and_then(|_| git_utils::clean_working_dir(repo))
    .unwrap_or_else(|e| eprintln!("Could not merge remote master: {:?}", e));

    repo.cleanup_state().expect("Couldn't clean-up state")
}

fn read_config_from_file(registry_uri: &str) -> Option<CratesIOConfig> {
    let config_json_path = Path::new(registry_uri).join("config.json");
    let read_file = OpenOptions::new().read(true).open(config_json_path)
        .expect("Could not read config.json");
    serde_json::from_reader(&read_file).ok()
}

fn write_config_to_file(config: &CratesIOConfig, registry_uri: &str) -> Result<(), serde_json::Error> {
    let config_json_path = Path::new(registry_uri).join("config.json");
    let write_file = OpenOptions::new().write(true).create(true).truncate(true).open(config_json_path)
            .expect("Could not config.json for writing");
    serde_json::to_writer(&write_file, &config)
}

fn add_custom_config(repo: &Repository, registry_uri: &str, public_interface: &str) {
    let new_config = CratesIOConfig{ 
        api: String::from("https://crates.io/"),    
        dl: String::from(public_interface),
    };

    let current_config_opt = read_config_from_file(registry_uri);
    let equal = current_config_opt
        .map(|current_config| current_config == new_config)
        .unwrap_or(false);

    if !equal {
        write_config_to_file(&new_config, registry_uri).expect("Could not write config.json");
        commit_custom_config(repo).expect("Could not commit config.json");
    }
}

fn commit_custom_config(repo: &Repository) -> Result<(), Error> {
    let mut index = repo.index()?;

    index.add_path(Path::new("config.json"))
    .and_then(|()| index.write_tree())
    .and_then(|oid| { repo.find_tree(oid) })
    .and_then(|tree| {
        let signature = Signature::now(CARGO_SIG_AUTHOR, CARGO_SIG_EMAIL)
            .expect("Could not create signature");
        let parent_commit = find_head_commit(&repo)?;
        repo.commit(Some("HEAD"), //  point HEAD to our new commit
            &signature, // author
            &signature, // committer
            "API mirror as configuration", // commit message
            &tree, // tree
            &[&parent_commit]) // parents
    })
    .and_then(|_| git_utils::clean_working_dir(repo))
}

fn monitor_registry(
    repo: &Repository,
    stop: mpsc::Receiver<()>,
    download_crates: mpsc::Sender<()>,
    registry_uri: &str,
    interval: &u32,
    public_crate_store_interface: &str) {
    loop {
        let mut remote = match repo.find_remote("origin") {
            Ok(r) => r,
            Err(_) => repo.remote("origin", &OFFICIAL_CRATES_REGISTRY).expect("Could add remote git repository"),
        };

        remote.connect(Direction::Fetch).expect("Could not connect to remote repository");
        println!("Fetching remote repository");
        remote.fetch(&["master"], None, None).expect("Could not fetch from remote repository");
        println!("Fetch complete");
        remote.disconnect();

        // Try to merge upstream
        merge_upstream_master(repo);
        add_custom_config(repo, registry_uri, public_crate_store_interface);

        // Start downloading crates
        download_crates.send(()).unwrap_or_else(|e| eprintln!("Could not trigger crates for download: {:?}", e));

        let start_time = SystemTime::now();
        loop {
            // Check if we need to exit the monitoring loop
            if let Ok(()) = stop.try_recv() {
                break;
            }

            let waiting_time_over = SystemTime::now()
            .duration_since(start_time)
            .ok().map(|delta| delta > Duration::from_secs(*interval as u64))
            .unwrap_or(false);
            if waiting_time_over { break; }
            thread::sleep(Duration::from_secs(5))
        }
    }
}

fn open_git_repo(uri: &str) -> Repository {
    let repo = if Path::new(&uri).exists() {
        Repository::open(uri)
    } else {
        Repository::clone(OFFICIAL_CRATES_REGISTRY, uri)
    };
    repo.expect(&format!("Could not open repository: {}", &uri))
}

pub fn start(registry_config: &config::CrateRegistry, crate_store_config: &config::CrateStore) -> (mpsc::Sender<()>, mpsc::Receiver<()>) {
    let registry_config = registry_config.clone();
    let public_crate_store_interface = format!("http://{}:{}", crate_store_config.public_host, crate_store_config.port);
    let (tx_monitoring, rx_monitoring) = mpsc::channel();
    let (tx_download_crates, rx_download_crates) = mpsc::channel();

    thread::spawn(move || {
        let repo = open_git_repo(&registry_config.uri);
        monitor_registry(&repo, rx_monitoring, tx_download_crates, &registry_config.uri, &registry_config.update_interval, &public_crate_store_interface)
    });
    (tx_monitoring, rx_download_crates)
}