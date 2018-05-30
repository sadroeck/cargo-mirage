use git2::{Repository, build::CheckoutBuilder, MergeOptions, FileFavor, IndexAddOption, AnnotatedCommit, Error, Oid, BranchType, ObjectType};

pub fn force_merge_remote_commit<'a>(repo: &Repository, remote_commit: AnnotatedCommit<'a>) -> Result<Option<AnnotatedCommit<'a>>, Error> {
    let mut checkout_opts = CheckoutBuilder::new();
    let mut merge_opts = MergeOptions::new();
    merge_opts.file_favor(FileFavor::Theirs);
    checkout_opts.force().use_theirs(true);
    let remote_commit_opt = repo.merge(&[&remote_commit], Some(&mut merge_opts), Some(&mut checkout_opts)).map(|()| Some(remote_commit));

    if remote_commit_opt.is_ok() {
        repo.index()
        .and_then(|mut index| index.add_all(["*"].into_iter(), IndexAddOption::FORCE, None))
        .expect("Could not commit merge");
    }

    remote_commit_opt
}

// TODO: fix FF merge, still consolidates into single commit
pub fn fast_forward_merge<'a>(repo: &Repository, remote_commmit_id: Oid) -> Result<Option<AnnotatedCommit<'a>>, Error> {
    repo.find_branch("origin/master", BranchType::Remote)
    .and_then(|remote_branch| remote_branch.into_reference().peel(ObjectType::Tree))
    .and_then(|tree| repo.checkout_tree(&tree, None))
    .and_then(|()| repo.head())
    .and_then(|mut head| head.set_target(remote_commmit_id, "fast-forward to remote master"))
    .and_then(|_| { 
        println!("Fast-forward merge");
        Ok(None)
    })
}