//! Install-image to root merge transaction.
//!
//! After a build produces an install image (`D`), emerge merges it into the
//! live root, recording every installed path and applying CONFIG_PROTECT so an
//! admin's edited config files are not clobbered. This module models that as an
//! explicit, isolated-root transaction: it takes the image dir and the target
//! root, detects collisions with files owned by *other* packages, applies
//! CONFIG_PROTECT, and records the merged file list (the VDB `CONTENTS`).
//!
//! Filesystem ownership is explicit: [`MergeTransaction`] carries its image
//! root, target root, and config-protect policy. All tests use temp roots.
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/vartree.py` (`dblink.merge`,
//!   `treewalk`, `mergeme`, collision-protect, CONFIG_PROTECT handling)
//! - `research/portage/lib/_emerge/EbuildMerge.py`, `PackageMerge.py`

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use super::config_protect::ConfigProtect;

/// One entry recorded in a package's installed-file manifest (VDB CONTENTS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentEntry {
    /// A regular file: relative path plus its md5 and a marker for whether it
    /// was redirected to a `._cfg` protected name.
    File { path: String, protected: bool },
    /// A directory.
    Dir { path: String },
    /// A symlink: path and its target.
    Symlink { path: String, target: String },
}

impl ContentEntry {
    /// The root-relative path this entry occupies.
    pub fn path(&self) -> &str {
        match self {
            Self::File { path, .. } | Self::Dir { path } | Self::Symlink { path, .. } => path,
        }
    }
}

/// Error raised during a merge.
#[derive(Debug)]
pub enum MergeError {
    /// The install image directory does not exist.
    MissingImage(PathBuf),
    /// A file in the image collides with a file owned by another package.
    Collision { path: String, owner: String },
    /// An I/O error during the merge.
    Io(String),
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingImage(path) => write!(f, "install image not found: '{}'", path.display()),
            Self::Collision { path, owner } => {
                write!(f, "file collision at '{path}' (owned by {owner})")
            }
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for MergeError {}

/// The result of a successful merge: the recorded CONTENTS list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeResult {
    pub contents: Vec<ContentEntry>,
}

impl MergeResult {
    /// The root-relative paths that were installed.
    pub fn installed_paths(&self) -> Vec<String> {
        self.contents.iter().map(|e| e.path().to_string()).collect()
    }
}

/// A merge transaction from an install image into a target root.
pub struct MergeTransaction<'a> {
    image: PathBuf,
    root: PathBuf,
    protect: &'a ConfigProtect,
    /// cpv -> set of root-relative paths it already owns, for collision checks.
    owned_by_others: Vec<(String, BTreeSet<String>)>,
}

impl<'a> MergeTransaction<'a> {
    /// Creates a transaction merging `image` into `root` under `protect`.
    pub fn new(
        image: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        protect: &'a ConfigProtect,
    ) -> Self {
        Self {
            image: image.into(),
            root: root.into(),
            protect,
            owned_by_others: Vec::new(),
        }
    }

    /// Registers paths owned by an already-installed package, so a collision is
    /// reported if the new image would overwrite one of them.
    pub fn with_existing_owner(mut self, cpv: impl Into<String>, paths: &[&str]) -> Self {
        self.owned_by_others.insert(
            0,
            (
                cpv.into(),
                paths
                    .iter()
                    .map(|p| p.trim_start_matches('/').to_string())
                    .collect(),
            ),
        );
        self
    }

    /// Returns the owner cpv of a path already owned by another package.
    fn collision_owner(&self, rel: &str) -> Option<&str> {
        self.owned_by_others
            .iter()
            .find(|(_, paths)| paths.contains(rel))
            .map(|(cpv, _)| cpv.as_str())
    }

    /// Executes the merge: walk the image depth-first, create directories,
    /// copy files (redirecting protected existing configs to `._cfg` names),
    /// and reproduce symlinks. Returns the recorded CONTENTS.
    pub fn run(&self) -> Result<MergeResult, MergeError> {
        if !self.image.is_dir() {
            return Err(MergeError::MissingImage(self.image.clone()));
        }
        let mut contents = Vec::new();
        self.merge_dir(&self.image, &mut contents)?;
        // Sort by path for a deterministic CONTENTS list.
        contents.sort_by(|a, b| a.path().cmp(b.path()));
        Ok(MergeResult { contents })
    }

    fn merge_dir(&self, dir: &Path, contents: &mut Vec<ContentEntry>) -> Result<(), MergeError> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| MergeError::Io(format!("{}: {e}", dir.display())))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let src = entry.path();
            let rel = self.relative(&src);
            let file_type = entry
                .file_type()
                .map_err(|e| MergeError::Io(format!("{}: {e}", src.display())))?;

            if file_type.is_symlink() {
                self.merge_symlink(&src, &rel, contents)?;
            } else if file_type.is_dir() {
                self.merge_subdir(&src, &rel, contents)?;
            } else {
                self.merge_file(&src, &rel, contents)?;
            }
        }
        Ok(())
    }

    fn merge_subdir(
        &self,
        src: &Path,
        rel: &str,
        contents: &mut Vec<ContentEntry>,
    ) -> Result<(), MergeError> {
        let dest = self.root.join(rel);
        std::fs::create_dir_all(&dest)
            .map_err(|e| MergeError::Io(format!("{}: {e}", dest.display())))?;
        contents.push(ContentEntry::Dir {
            path: rel.to_string(),
        });
        self.merge_dir(src, contents)
    }

    fn merge_symlink(
        &self,
        src: &Path,
        rel: &str,
        contents: &mut Vec<ContentEntry>,
    ) -> Result<(), MergeError> {
        if let Some(owner) = self.collision_owner(rel) {
            return Err(MergeError::Collision {
                path: rel.to_string(),
                owner: owner.to_string(),
            });
        }
        let target = std::fs::read_link(src)
            .map_err(|e| MergeError::Io(format!("{}: {e}", src.display())))?;
        let dest = self.root.join(rel);
        ensure_parent(&dest)?;
        // Replace an existing symlink/file at the destination; a missing path
        // is not an error, but any other failure should surface.
        match std::fs::remove_file(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(MergeError::Io(format!("{}: {e}", dest.display()))),
        }
        symlink(&target, &dest)?;
        contents.push(ContentEntry::Symlink {
            path: rel.to_string(),
            target: target.to_string_lossy().into_owned(),
        });
        Ok(())
    }

    fn merge_file(
        &self,
        src: &Path,
        rel: &str,
        contents: &mut Vec<ContentEntry>,
    ) -> Result<(), MergeError> {
        if let Some(owner) = self.collision_owner(rel) {
            return Err(MergeError::Collision {
                path: rel.to_string(),
                owner: owner.to_string(),
            });
        }

        let dest = self.root.join(rel);
        ensure_parent(&dest)?;

        let abs = format!("/{rel}");
        let mut protected = false;
        let final_dest = if dest.exists() && self.protect.is_protected(&abs) {
            // Write to a sibling ._cfgNNNN_ name instead of overwriting.
            protected = true;
            let basename = file_name(rel);
            let siblings = siblings_of(&dest);
            let protected_name = ConfigProtect::protect_filename(&basename, &siblings, true);
            dest.with_file_name(protected_name)
        } else {
            dest.clone()
        };

        std::fs::copy(src, &final_dest)
            .map_err(|e| MergeError::Io(format!("{}: {e}", final_dest.display())))?;

        let recorded = if protected {
            self.relative(&final_dest)
        } else {
            rel.to_string()
        };
        contents.push(ContentEntry::File {
            path: recorded,
            protected,
        });
        Ok(())
    }

    fn relative(&self, path: &Path) -> String {
        // Path relative to whichever root it lives under (image or target).
        let stripped = path
            .strip_prefix(&self.image)
            .or_else(|_| path.strip_prefix(&self.root))
            .unwrap_or(path);
        stripped.to_string_lossy().into_owned()
    }
}

fn ensure_parent(dest: &Path) -> Result<(), MergeError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MergeError::Io(format!("{}: {e}", parent.display())))?;
    }
    Ok(())
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

fn siblings_of(dest: &Path) -> Vec<String> {
    let Some(parent) = dest.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

#[cfg(unix)]
fn symlink(target: &Path, dest: &Path) -> Result<(), MergeError> {
    std::os::unix::fs::symlink(target, dest)
        .map_err(|e| MergeError::Io(format!("{}: {e}", dest.display())))
}

#[cfg(not(unix))]
fn symlink(_target: &Path, dest: &Path) -> Result<(), MergeError> {
    Err(MergeError::Io(format!(
        "symlinks unsupported on this platform: {}",
        dest.display()
    )))
}
