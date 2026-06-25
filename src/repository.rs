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
        // The eclass-resolved metadata cache, if the repo ships one.
        let cache_root = root.join("metadata").join("md5-cache");

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
                load_package(&pkg_path, &cache_root, &category, &package, &name, &mut db)?;
            }
        }

        Ok(Self { name, db })
    }
}

/// Regenerates the `metadata/md5-cache/<cat>/<pf>` cache for the repository at
/// `root`, writing one `KEY=value`-per-line entry per package (the keys emerge
/// reads: EAPI, SLOT, KEYWORDS, IUSE, and the dependency variables). Returns the
/// number of cache entries written.
///
/// This is a from-metadata regen (no eclass sourcing): it mirrors the cache
/// *format* emerge consumes, populated from each ebuild's directly-declared
/// `KEY="value"` assignments. Reference: `portage.cache.flat_hash`/`md5-cache`.
pub fn regen_md5_cache(root: impl AsRef<Path>) -> Result<usize, RepositoryError> {
    let root = root.as_ref();
    let repo = Repository::load(root)?;
    let cache_root = root.join("metadata").join("md5-cache");
    let mut count = 0usize;

    for (cpv, meta) in repo.db.iter() {
        let (cp, _) = crate::version::split_cpv(cpv);
        let Some((category, _)) = cp.split_once('/') else {
            continue;
        };
        // The cache entry filename is the package-version (`pf`).
        let pf = cpv.split_once('/').map(|x| x.1).unwrap_or(cpv);
        let entry_dir = cache_root.join(category);
        std::fs::create_dir_all(&entry_dir)
            .map_err(|e| RepositoryError::Io(format!("{}: {e}", entry_dir.display())))?;
        let entry_path = entry_dir.join(pf);

        let mut body = String::new();
        let slot = match (&meta.slot, &meta.sub_slot) {
            (Some(s), Some(sub)) => format!("{s}/{sub}"),
            (Some(s), None) => s.clone(),
            _ => "0".to_string(),
        };
        for key in [
            "DEPEND",
            "RDEPEND",
            "PDEPEND",
            "BDEPEND",
            "IDEPEND",
            "REQUIRED_USE",
        ] {
            if let Some(v) = meta.deps.get(key) {
                body.push_str(&format!("{key}={v}\n"));
            }
        }
        body.push_str(&format!("SLOT={slot}\n"));
        body.push_str(&format!("EAPI={}\n", meta.eapi.as_deref().unwrap_or("0")));
        body.push_str(&format!("KEYWORDS={}\n", meta.keywords.join(" ")));
        body.push_str(&format!("IUSE={}\n", meta.iuse.join(" ")));

        std::fs::write(&entry_path, body)
            .map_err(|e| RepositoryError::Io(format!("{}: {e}", entry_path.display())))?;
        count += 1;
    }
    Ok(count)
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
    cache_root: &Path,
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
        let pf = format!("{package}-{version}");

        // Prefer the eclass-resolved md5-cache entry (which carries the real
        // KEYWORDS/SLOT/EAPI/deps for ebuilds that inherit eclasses); fall back
        // to parsing the raw ebuild when no cache entry exists.
        let cache_entry = cache_root.join(category).join(&pf);
        let metadata = if let Ok(cache_text) = std::fs::read_to_string(&cache_entry) {
            parse_md5_cache_entry(&cache_text, repo_name)
        } else {
            let ebuild_path = pkg_path.join(&file);
            let content = std::fs::read_to_string(&ebuild_path)
                .map_err(|err| RepositoryError::Io(format!("{}: {err}", ebuild_path.display())))?;
            parse_ebuild_metadata(&content, &ebuild_path, repo_name)?
        };
        db.insert(format!("{category}/{pf}"), metadata);
    }
    Ok(())
}

/// Parses an md5-cache entry (`KEY=value` per line, eclass-resolved) into
/// [`PackageMetadata`]. Unlike the ebuild parser, values are already final —
/// no shell expansion is needed.
fn parse_md5_cache_entry(text: &str, repo_name: &str) -> PackageMetadata {
    let mut vars: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            vars.insert(key.to_string(), value.to_string());
        }
    }
    metadata_from_vars(&vars, repo_name)
}

/// Builds [`PackageMetadata`] from a resolved `KEY -> value` map (shared by the
/// md5-cache and ebuild paths).
fn metadata_from_vars(vars: &HashMap<String, String>, repo_name: &str) -> PackageMetadata {
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
    PackageMetadata {
        slot: Some(slot),
        sub_slot,
        repo: Some(repo_name.to_string()),
        eapi: Some(vars.get("EAPI").cloned().unwrap_or_else(|| "0".to_string())),
        iuse: tokens("IUSE"),
        use_enabled: Vec::new(),
        keywords: tokens("KEYWORDS"),
        deps,
    }
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
    Ok(metadata_from_vars(&vars, repo_name))
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
