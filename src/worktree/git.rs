use git2::Repository;
use std::path::Path;

pub fn is_git_repo(path: &Path) -> bool {
    Repository::discover(path).is_ok()
}

/// Open a git repository.
///
/// # Errors
/// Returns an error if path is not a valid git repo.
pub fn open_repo(path: &Path) -> Result<Repository, String> {
    Repository::open(path).map_err(|e| format!("Not a git repo: {e}"))
}

/// Get the worktree status as a string.
///
/// # Errors
/// Returns an error if the repo cannot be accessed.
pub fn get_worktree_status(path: &Path) -> Result<String, String> {
    let repo = open_repo(path)?;
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| format!("Failed to get status: {e}"))?;
    if statuses.is_empty() {
        Ok("Clean worktree".to_string())
    } else {
        Ok(format!("{} file(s) changed", statuses.len()))
    }
}
