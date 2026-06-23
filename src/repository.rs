//! Filesystem repository loading into the [`crate::dbapi`] views.
//!
//! A Portage ebuild repository is a directory whose `profiles/repo_name` file
//! names the repo and whose packages live at `<cat>/<pkg>/<pkg>-<ver>.ebuild`.
//! Each ebuild assigns metadata as shell `KEY="value"` lines (`EAPI`, `SLOT`,
//! `KEYWORDS`, `IUSE`, `DEPEND`, `RDEPEND`, ...). This loader reads that tree
//! into a [`PackageDb`] using the same `KEY=value` parser as the config layer,
//! so the resolver consumes one uniform package store.
//!
//! Filesystem ownership is explicit: [`Repository::load`] takes the repo root.
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/porttree.py`
//! - `research/portage/lib/portage/tests/resolver/ResolverPlayground.py`
//!   (`_create_ebuilds` ebuild layout)

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{ParseError, getconfig};
use crate::dbapi::{PackageDb, PackageMetadata};

/// Error raised while loading a repository tree.
#[derive(Debug)]
pub enum RepositoryError {
    /// The repository root does not exist or is not a directory.
    MissingRoot(PathBuf),
    /// `profiles/repo_name` was missing or empty.
    MissingRepoName(PathBuf),
    /// An ebuild's metadata failed to parse.
    Parse { path: PathBuf, error: ParseError },
    /// An I/O error reading the tree.
    Io(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRoot(path) => write!(f, "repository root not found: '{}'", path.display()),
            Self::MissingRepoName(path) => {
                write!(
                    f,
                    "missing or empty profiles/repo_name: '{}'",
                    path.display()
                )
            }
            Self::Parse { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for RepositoryError {}

/// A loaded repository: its name and the packages it provides.
#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub db: PackageDb,
}

impl Repository {
    /// Loads the repository rooted at `root` into a [`PackageDb`].
    pub fn load(root: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(RepositoryError::MissingRoot(root.to_path_buf()));
        }
        let name = read_repo_name(root)?;
        let mut db = PackageDb::new();

        for category in read_dir_sorted(root)? {
            let cat_path = root.join(&category);
            if !cat_path.is_dir() || is_reserved_top_dir(&category) {
                continue;
            }
            for package in read_dir_sorted(&cat_path)? {
                let pkg_path = cat_path.join(&package);
                if !pkg_path.is_dir() {
                    continue;
                }
                load_package(&pkg_path, &category, &package, &name, &mut db)?;
            }
        }

        Ok(Self { name, db })
    }
}

/// Top-level repository directories that are not package categories.
fn is_reserved_top_dir(name: &str) -> bool {
    matches!(
        name,
        "profiles" | "metadata" | "eclass" | "licenses" | "scripts"
    ) || name.starts_with('.')
}

fn read_repo_name(root: &Path) -> Result<String, RepositoryError> {
    let path = root.join("profiles").join("repo_name");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let name = text.lines().next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                Err(RepositoryError::MissingRepoName(path))
            } else {
                Ok(name)
            }
        }
        Err(_) => Err(RepositoryError::MissingRepoName(path)),
    }
}

fn read_dir_sorted(path: &Path) -> Result<Vec<String>, RepositoryError> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(path)
        .map_err(|err| RepositoryError::Io(format!("{}: {err}", path.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| RepositoryError::Io(format!("{}: {err}", path.display())))?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn load_package(
    pkg_path: &Path,
    category: &str,
    package: &str,
    repo_name: &str,
    db: &mut PackageDb,
) -> Result<(), RepositoryError> {
    let ebuild_suffix = ".ebuild";
    for file in read_dir_sorted(pkg_path)? {
        let Some(stem) = file.strip_suffix(ebuild_suffix) else {
            continue;
        };
        // The version is the filename stem minus the package-name prefix.
        let Some(version) = stem.strip_prefix(&format!("{package}-")) else {
            continue;
        };
        let ebuild_path = pkg_path.join(&file);
        let content = std::fs::read_to_string(&ebuild_path)
            .map_err(|err| RepositoryError::Io(format!("{}: {err}", ebuild_path.display())))?;
        let metadata = parse_ebuild_metadata(&content, &ebuild_path, repo_name)?;
        let cpv = format!("{category}/{package}-{version}");
        db.insert(cpv, metadata);
    }
    Ok(())
}

/// The dependency variable keys an ebuild may declare.
const DEP_KEYS: &[&str] = &[
    "DEPEND",
    "RDEPEND",
    "PDEPEND",
    "BDEPEND",
    "IDEPEND",
    "REQUIRED_USE",
    "SRC_URI",
];

fn parse_ebuild_metadata(
    content: &str,
    path: &Path,
    repo_name: &str,
) -> Result<PackageMetadata, RepositoryError> {
    let empty = HashMap::new();
    let vars = getconfig(content, true, &empty).map_err(|error| RepositoryError::Parse {
        path: path.to_path_buf(),
        error,
    })?;

    let tokens = |key: &str| -> Vec<String> {
        vars.get(key)
            .map(|v| v.split_whitespace().map(strip_iuse_default).collect())
            .unwrap_or_default()
    };

    let (slot, sub_slot) = split_slot(vars.get("SLOT").map(String::as_str).unwrap_or("0"));
    let mut deps = std::collections::BTreeMap::new();
    for key in DEP_KEYS {
        if let Some(value) = vars.get(*key) {
            deps.insert((*key).to_string(), value.clone());
        }
    }

    Ok(PackageMetadata {
        slot: Some(slot),
        sub_slot,
        repo: Some(repo_name.to_string()),
        eapi: Some(vars.get("EAPI").cloned().unwrap_or_else(|| "0".to_string())),
        iuse: tokens("IUSE"),
        use_enabled: Vec::new(),
        keywords: tokens("KEYWORDS"),
        deps,
    })
}

/// Strips a leading `+`/`-` default marker from an IUSE token (`+foo` -> `foo`).
fn strip_iuse_default(token: &str) -> String {
    token.trim_start_matches(['+', '-']).to_string()
}

fn split_slot(slot: &str) -> (String, Option<String>) {
    match slot.split_once('/') {
        Some((s, sub)) => (s.to_string(), Some(sub.to_string())),
        None => (slot.to_string(), None),
    }
}
