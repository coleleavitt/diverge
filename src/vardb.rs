//! Installed-package database (`vartree`) loading from a real `/var/db/pkg`.
//!
//! Each installed package is a directory `<category>/<pf>/` under the VDB root
//! containing one file per metadata key (`SLOT`, `KEYWORDS`, `IUSE`, `USE`,
//! `DEPEND`, `RDEPEND`, ...) whose contents are the value. This loader reads
//! that tree into a [`PackageDb`] tagged as installed.
//!
//! This module is **read-only**: it never writes to the VDB. Mutating installed
//! state goes through the executor's merge/unmerge transactions against an
//! explicit (and in tests, isolated) root.
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/vartree.py` (`vardbapi`)

use std::fmt;
use std::path::{Path, PathBuf};

use crate::dbapi::{PackageDb, PackageMetadata};
use crate::executor::merge::ContentEntry;

/// Error raised while loading the installed-package database.
#[derive(Debug)]
pub enum VardbError {
    /// An I/O error reading the VDB tree.
    Io(String),
}

impl fmt::Display for VardbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for VardbError {}

/// Metadata-file keys read from each installed package directory.
const DEP_KEYS: &[&str] = &[
    "DEPEND",
    "RDEPEND",
    "PDEPEND",
    "BDEPEND",
    "IDEPEND",
    "REQUIRED_USE",
];

/// Loads the installed-package database rooted at `vdb_root` (typically
/// `<eroot>/var/db/pkg`). A missing root yields an empty db (nothing installed).
pub fn load(vdb_root: impl AsRef<Path>) -> Result<PackageDb, VardbError> {
    let vdb_root = vdb_root.as_ref();
    let mut db = PackageDb::new();
    if !vdb_root.is_dir() {
        return Ok(db);
    }

    for category in read_dir_sorted(vdb_root)? {
        let cat_path = vdb_root.join(&category);
        if !cat_path.is_dir() || category.starts_with('.') {
            continue;
        }
        for pf in read_dir_sorted(&cat_path)? {
            let pkg_dir = cat_path.join(&pf);
            if !pkg_dir.is_dir() {
                continue;
            }
            let cpv = format!("{category}/{pf}");
            db.insert(cpv, read_entry(&pkg_dir));
        }
    }
    Ok(db)
}

/// Reads one installed package directory into [`PackageMetadata`].
fn read_entry(pkg_dir: &Path) -> PackageMetadata {
    let read = |key: &str| -> Option<String> {
        std::fs::read_to_string(pkg_dir.join(key))
            .ok()
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    let tokens = |key: &str| -> Vec<String> {
        read(key)
            .map(|v| v.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    };

    let (slot, sub_slot) = split_slot(read("SLOT").as_deref().unwrap_or("0"));
    let mut deps = std::collections::BTreeMap::new();
    for key in DEP_KEYS {
        if let Some(value) = read(key) {
            deps.insert((*key).to_string(), value);
        }
    }

    PackageMetadata {
        slot: Some(slot),
        sub_slot,
        repo: read("repository"),
        eapi: read("EAPI").or_else(|| Some("0".to_string())),
        iuse: tokens("IUSE").iter().map(|t| strip_default(t)).collect(),
        use_enabled: tokens("USE"),
        keywords: tokens("KEYWORDS"),
        deps,
    }
}

fn strip_default(token: &str) -> String {
    token.trim_start_matches(['+', '-']).to_string()
}

fn split_slot(slot: &str) -> (String, Option<String>) {
    match slot.split_once('/') {
        Some((s, sub)) => (s.to_string(), Some(sub.to_string())),
        None => (slot.to_string(), None),
    }
}

fn read_dir_sorted(path: &Path) -> Result<Vec<String>, VardbError> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .map_err(|e| VardbError::Io(format!("{}: {e}", path.display())))?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    Ok(names)
}

/// The conventional VDB path under an EROOT (`<eroot>/var/db/pkg`).
pub fn vdb_path(eroot: &Path) -> PathBuf {
    eroot.join("var/db/pkg")
}

/// Records a newly-merged package into the VDB at `vdb_root`: writes the
/// per-key metadata files and the `CONTENTS` manifest. Mirrors what
/// `dblink.merge` records in `/var/db/pkg/<cat>/<pf>/`.
///
/// `cpv` is `category/package-version`; `contents` is the merged file list
/// (typically a [`crate::executor::merge::MergeResult`]'s `contents`). This
/// only ever writes under `vdb_root`, which callers must keep isolated in tests.
pub fn record_install(
    vdb_root: &Path,
    cpv: &str,
    metadata: &PackageMetadata,
    contents: &[ContentEntry],
) -> Result<(), VardbError> {
    let (category, pf) = cpv
        .split_once('/')
        .ok_or_else(|| VardbError::Io(format!("invalid cpv: '{cpv}'")))?;
    let pkg_dir = vdb_root.join(category).join(pf);
    std::fs::create_dir_all(&pkg_dir)
        .map_err(|e| VardbError::Io(format!("{}: {e}", pkg_dir.display())))?;

    let write = |key: &str, value: &str| -> Result<(), VardbError> {
        std::fs::write(pkg_dir.join(key), format!("{value}\n"))
            .map_err(|e| VardbError::Io(format!("{}: {e}", pkg_dir.join(key).display())))
    };

    write("CATEGORY", category)?;
    write("PF", pf)?;
    write(
        "SLOT",
        &metadata.aux("SLOT").unwrap_or_else(|| "0".to_string()),
    )?;
    if let Some(eapi) = &metadata.eapi {
        write("EAPI", eapi)?;
    }
    if let Some(repo) = &metadata.repo {
        write("repository", repo)?;
    }
    write("IUSE", &metadata.iuse.join(" "))?;
    write("USE", &metadata.use_enabled.join(" "))?;
    write("KEYWORDS", &metadata.keywords.join(" "))?;
    for (key, value) in &metadata.deps {
        write(key, value)?;
    }
    write("CONTENTS", &render_contents(contents))?;
    Ok(())
}

/// Reads a recorded package's `CONTENTS` manifest back into [`ContentEntry`]
/// values (the inverse of [`record_install`]'s `render_contents`). Returns an
/// empty list when the package or its CONTENTS file is absent.
pub fn read_contents(vdb_root: &Path, cpv: &str) -> Vec<ContentEntry> {
    let Some((category, pf)) = cpv.split_once('/') else {
        return Vec::new();
    };
    let path = vdb_root.join(category).join(pf).join("CONTENTS");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some("dir"), Some(p)) => out.push(ContentEntry::Dir {
                path: p.trim_start_matches('/').to_string(),
            }),
            (Some("obj"), Some(p)) => out.push(ContentEntry::File {
                path: p.trim_start_matches('/').to_string(),
                protected: false,
            }),
            (Some("sym"), Some(p)) => {
                // `sym /path -> target mtime`
                let target = parts
                    .skip_while(|t| *t != "->")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                out.push(ContentEntry::Symlink {
                    path: p.trim_start_matches('/').to_string(),
                    target,
                });
            }
            _ => {}
        }
    }
    out
}

/// Removes a package's VDB entry directory (after an unmerge). A missing entry
/// is not an error.
pub fn remove_install(vdb_root: &Path, cpv: &str) -> Result<(), VardbError> {
    let Some((category, pf)) = cpv.split_once('/') else {
        return Err(VardbError::Io(format!("invalid cpv: '{cpv}'")));
    };
    let pkg_dir = vdb_root.join(category).join(pf);
    match std::fs::remove_dir_all(&pkg_dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(VardbError::Io(format!("{}: {e}", pkg_dir.display()))),
    }
}

/// Renders a CONTENTS manifest in Portage's format: `dir <path>`,
/// `obj <path> <md5> <mtime>` (md5/mtime are placeholders here), `sym <path> ->
/// <target> <mtime>`.
fn render_contents(contents: &[ContentEntry]) -> String {
    let mut out = String::new();
    for entry in contents {
        match entry {
            ContentEntry::Dir { path } => out.push_str(&format!("dir /{path}\n")),
            ContentEntry::File { path, .. } => {
                out.push_str(&format!("obj /{path} 0 0\n"));
            }
            ContentEntry::Symlink { path, target } => {
                out.push_str(&format!("sym /{path} -> {target} 0\n"));
            }
        }
    }
    out
}
