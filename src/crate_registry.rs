use git2::{Repository, Direction, Signature, Commit, Error, ObjectType, BranchType, MergeAnalysis, AnnotatedCommit};
use super::config;
use std::thread;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use git_utils;
use std::fs::File;
use serde_json;

const OFFICIAL_CRATES_REGISTRY : &str = "https://github.com/rust-lang/crates.io-index.git";
const CARGO_SIG_AUTHOR : &str = "Cargo mirage";
const CARGO_SIG_EMAIL : &str = "cargo@mirage.io";

#[derive(Serialize,Deserialize,Clone,Debug)]
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
    find_remote_master_tip(repo)
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
                    .map(|x| Some(x))
                },
                None => Ok(None),
            }
        })
        .map(|_| ()).unwrap_or_else(|e| { 
            eprintln!("Could not merge remote master: {:?}", e);
        });

        repo.cleanup_state().expect("Couldn't clean-up state")
}

fn has_custom_config(repo: &Repository) -> bool {
    repo.head()
        .and_then(|head| head.resolve())
        .and_then(|head| head.peel(ObjectType::Commit))
        .and_then(|head_obj| head_obj.into_commit()
            .map_err(|e| Error::from_str(format!("Could not find commit: {:?}", e).as_str())))
        .map(|head_commit| { 
            head_commit.author().name().unwrap_or("") == CARGO_SIG_AUTHOR 
            && head_commit.author().email().unwrap_or("") == CARGO_SIG_EMAIL
        }).expect("Could not find commit")
}

fn add_custom_config(repo: &Repository, connection_str: &str) {
    let mut index = repo.index().expect("Could not retrieve git index");

    let new_config = CratesIOConfig{ 
        api: String::from("https://crates.io/"),    
        dl: format!("http://{}/", connection_str),
    };

    File::open("config.json")
    .map_err(serde_json::Error::io)
    .and_then(|file| serde_json::to_writer(file, &new_config))
    .unwrap_or_else(|e| eprintln!("Could not write to config.json: {}", e));

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
    }).expect("Could not update registry to a local configuration");
}

fn monitor_registry(
    repo: &Repository,
    stop: mpsc::Receiver<()>,
    download_crates: mpsc::Sender<()>,
    interval: &u32,
    crate_store_connection: &str) {
    loop {
        let mut remote = match repo.find_remote("origin") {
            Ok(r) => r,
            Err(_) => repo.remote("origin", &OFFICIAL_CRATES_REGISTRY).expect("Could add remote git repository"),
        };

        remote.connect(Direction::Fetch).expect("Could not connect to remote repository");
        println!("Fetching remote repository");
        remote.fetch(&["master"], None, None).expect("Could not fetch from remote repository");
        println!("Fetch complete");

        // Try to merge upstream
        merge_upstream_master(repo);

        if !has_custom_config(repo) {
            add_custom_config(repo, crate_store_connection);
        }

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
    let crate_store_connection_string = config::crate_store_connection_string(crate_store_config);
    let (tx_monitoring, rx_monitoring) = mpsc::channel();
    let (tx_download_crates, rx_download_crates) = mpsc::channel();

    thread::spawn(move || {
        let repo = open_git_repo(&registry_config.uri);
        monitor_registry(&repo, rx_monitoring, tx_download_crates, &registry_config.update_interval, &crate_store_connection_string)
    });
    (tx_monitoring, rx_download_crates)
}