use git2::{Repository, Direction, Signature, Commit, Error, ObjectType, BranchType, MergeAnalysis, AnnotatedCommit};
use super::config;
use std::thread;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

const OFFICIAL_CRATES_REGISTRY : &str = "https://github.com/rust-lang/crates.io-index.git";
const CARGO_SIG_AUTHOR : &str = "Cargo mirage";
const CARGO_SIG_EMAIL : &str = "cargo@mirage.io";

enum MergeAction<'a> {
    FastForward,
    Normal(AnnotatedCommit<'a>),
    Nop,
}

fn merge_analysis_to_action<'a>(merge_analysis: MergeAnalysis, commit: AnnotatedCommit<'a>) -> MergeAction {
    if merge_analysis.contains(MergeAnalysis::ANALYSIS_FASTFORWARD) {
        MergeAction::FastForward
    } else if merge_analysis.contains(MergeAnalysis::ANALYSIS_NORMAL) {
        MergeAction::Normal(commit)
    } else {
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
                MergeAction::FastForward => {
                    // TODO: fix FF merge, still consolidates into single commit
                    repo.find_branch("origin/master", BranchType::Remote)
                        .and_then(|remote_branch| remote_branch.into_reference().peel(ObjectType::Tree))
                        .and_then(|tree| repo.checkout_tree(&tree, None))
                        .and_then(|()| repo.head())
                        .and_then(|mut head| head.set_target(remote_id, "fast-forward to remote master"))
                        .and_then(|_| { 
                            println!("Fast-forward merge");
                            Ok(None)
                        })
                },
                MergeAction::Nop => {
                    println!("repo up-to-date");
                    Ok(None)
                },
                MergeAction::Normal(remote_commit) => repo.merge(&[&remote_commit], None, None).map(|()| Some(remote_commit)) // cleanup failed merge
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
                    println!("Committing merge");
                    let mut index = repo.index().expect("Could not retrieve git index");
                    index.write_tree()
                    .and_then(|oid| { repo.find_tree(oid) })
                    .and_then(|tree| {
                        println!("Actually committing");
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

fn add_custom_config(repo: &Repository) {
    let mut index = repo.index().expect("Could not retrieve git index");

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

fn monitor_registry(repo: &Repository, stop: mpsc::Receiver<()>, interval: &u32) {
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
            add_custom_config(repo);
        }

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

pub fn start(config: &config::CrateRegistry) -> mpsc::Sender<()> {
    let config = config.clone();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let repo = open_git_repo(&config.uri);
        monitor_registry(&repo, rx, &config.update_interval)
    });
    tx
}