//! Repository synchronization, ported from `portage.sync`.
//!
//! emerge `--sync` updates a repository from its configured source via a sync
//! backend (rsync, git, webrsync, local copy). This module models that with an
//! injectable [`SyncBackend`] over an explicit repo location, plus a built-in
//! [`LocalSync`] that copies an isolated fixture tree — so the flow is testable
//! without network access.
//!
//! Reference:
//! - `research/portage/lib/portage/sync/syncbase.py`
//! - `research/portage/lib/portage/sync/modules/rsync/`
//! - `research/portage/lib/portage/tests/sync/test_sync_local.py`

use std::fmt;
use std::path::{Path, PathBuf};

/// The sync mechanism for a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncType {
    Rsync,
    Git,
    WebRsync,
    Local,
}

/// A repository's sync configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfig {
    /// The repository name.
    pub name: String,
    /// The on-disk location to sync into.
    pub location: PathBuf,
    /// The source URI (an rsync URL, git remote, or local path).
    pub uri: String,
    pub sync_type: SyncType,
}

/// The outcome of a sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// True when the local tree changed.
    pub updated: bool,
    /// Files written/updated (relative paths), for reporting/tests.
    pub changed_files: Vec<String>,
}

/// Error raised during a sync.
#[derive(Debug)]
pub enum SyncError {
    /// The configured source does not exist.
    SourceMissing(String),
    Io(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceMissing(s) => write!(f, "sync source missing: '{s}'"),
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for SyncError {}

/// An injectable sync backend. Production backends shell out to rsync/git via a
/// structured spawner; tests use [`LocalSync`].
pub trait SyncBackend {
    /// Syncs `config.location` from `config.uri`, returning what changed.
    fn sync(&mut self, config: &SyncConfig) -> Result<SyncOutcome, SyncError>;
}

/// A sync backend that recursively copies a local source tree into the repo
/// location (models `sync-type = local` and is the test double for others).
#[derive(Debug, Default, Clone)]
pub struct LocalSync;

impl SyncBackend for LocalSync {
    fn sync(&mut self, config: &SyncConfig) -> Result<SyncOutcome, SyncError> {
        let source = Path::new(&config.uri);
        if !source.is_dir() {
            return Err(SyncError::SourceMissing(config.uri.clone()));
        }
        std::fs::create_dir_all(&config.location)
            .map_err(|e| SyncError::Io(format!("{}: {e}", config.location.display())))?;
        let mut changed = Vec::new();
        copy_tree(source, &config.location, source, &mut changed)?;
        Ok(SyncOutcome {
            updated: !changed.is_empty(),
            changed_files: changed,
        })
    }
}

/// Recursively copies `dir` into `dest_root`, recording the relative paths of
/// files that were created or whose contents changed.
fn copy_tree(
    dir: &Path,
    dest_root: &Path,
    source_root: &Path,
    changed: &mut Vec<String>,
) -> Result<(), SyncError> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| SyncError::Io(format!("{}: {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| SyncError::Io(format!("{}: {e}", dir.display())))?;
        let src = entry.path();
        let rel = src
            .strip_prefix(source_root)
            .map_err(|_| SyncError::Io("path escaped source root".to_string()))?
            .to_path_buf();
        let dest = dest_root.join(&rel);
        let file_type = entry
            .file_type()
            .map_err(|e| SyncError::Io(format!("{}: {e}", src.display())))?;
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest)
                .map_err(|e| SyncError::Io(format!("{}: {e}", dest.display())))?;
            copy_tree(&src, dest_root, source_root, changed)?;
        } else if file_type.is_file() && copy_file_if_changed(&src, &dest)? {
            changed.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

/// Copies `src` to `dest` when the contents differ (or `dest` is absent).
/// Returns whether a write happened.
fn copy_file_if_changed(src: &Path, dest: &Path) -> Result<bool, SyncError> {
    let new_contents =
        std::fs::read(src).map_err(|e| SyncError::Io(format!("{}: {e}", src.display())))?;
    let differs = std::fs::read(dest)
        .map(|old| old != new_contents)
        .unwrap_or(true);
    if !differs {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SyncError::Io(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(dest, &new_contents)
        .map_err(|e| SyncError::Io(format!("{}: {e}", dest.display())))?;
    Ok(true)
}
