pub mod binding;
pub mod events;
pub mod git;
pub mod index;
pub mod manager;
#[cfg(test)]
pub mod manager_tests;

pub use binding::TaskBinding;
pub use events::{EventBus, WorktreeEvent};
pub use index::{WorktreeEntry, WorktreeIndex};
pub use manager::{detect_repo_root, validate_name, WorktreeManager};
