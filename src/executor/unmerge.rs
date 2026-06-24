//! Unmerge: removing an installed package's recorded files from a root.
//!
//! Given the package's CONTENTS list (the files/dirs/symlinks it owns) and the
//! target root, this removes its files and symlinks, then removes any
//! directories it owned that are now empty (deepest first), mirroring emerge's
//! `dblink.unmerge`. Directories that still hold files owned by other packages
//! are left in place.
//!
//! Reference: `research/portage/lib/portage/dbapi/vartree.py` (`dblink.unmerge`)
//! and `research/portage/lib/_emerge/PackageUninstall.py`.

use std::fmt;
use std::path::Path;

use super::merge::ContentEntry;

/// Error raised during an unmerge.
#[derive(Debug)]
pub enum UnmergeError {
    Io(String),
}

impl fmt::Display for UnmergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for UnmergeError {}

/// The result of an unmerge: which paths were removed and which directories
/// were kept because they were not empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnmergeResult {
    pub removed: Vec<String>,
    pub kept_dirs: Vec<String>,
}

/// Removes the files/symlinks listed in `contents` from `root`, then removes
/// now-empty owned directories (deepest first). Missing files are tolerated
/// (the package state may be partially merged), matching emerge's resilience.
pub fn unmerge(
    root: impl AsRef<Path>,
    contents: &[ContentEntry],
) -> Result<UnmergeResult, UnmergeError> {
    let root = root.as_ref();
    let mut removed = Vec::new();
    let mut kept_dirs = Vec::new();

    // Remove files and symlinks first.
    for entry in contents {
        if let ContentEntry::File { path, .. } | ContentEntry::Symlink { path, .. } = entry
            && remove_path(&root.join(path), false)?
        {
            removed.push(path.clone());
        }
    }

    // Remove owned directories deepest-first if now empty.
    let mut dirs: Vec<&String> = contents
        .iter()
        .filter_map(|e| match e {
            ContentEntry::Dir { path } => Some(path),
            _ => None,
        })
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(depth(p)));

    for dir in dirs {
        let dest = root.join(dir);
        if !is_empty_dir(&dest) {
            kept_dirs.push(dir.clone());
        } else if remove_path(&dest, true)? {
            removed.push(dir.clone());
        }
    }

    Ok(UnmergeResult { removed, kept_dirs })
}

/// Removes a file or directory, tolerating a missing path. Returns whether
/// something was actually removed.
fn remove_path(dest: &Path, is_dir: bool) -> Result<bool, UnmergeError> {
    let result = if is_dir {
        std::fs::remove_dir(dest)
    } else {
        std::fs::remove_file(dest)
    };
    match result {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(UnmergeError::Io(format!("{}: {e}", dest.display()))),
    }
}

fn depth(path: &str) -> usize {
    path.matches('/').count()
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}
