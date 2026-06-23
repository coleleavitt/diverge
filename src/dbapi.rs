//! In-memory package database views, ported from Portage's `dbapi`.
//!
//! [`PackageDb`] is the shared package-store abstraction the resolver queries:
//! an ordered set of packages, each a `category/package-version` plus metadata
//! (`SLOT`, `IUSE`, `USE`, `repository`, `KEYWORDS`, `EAPI`, dependency
//! strings). It reproduces the observable behavior of `fakedbapi`: `cpv_all`
//! lists known packages, `match` filters by atom via [`crate::matching`], and
//! `aux_get` reads metadata keys.
//!
//! The same type backs the ebuild tree (available packages), the binary tree,
//! and the installed `vartree` view; callers tag each store with its
//! [`DbKind`]. Filesystem-backed loaders live in [`crate::repository`].
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/__init__.py`
//! - `research/portage/lib/portage/dbapi/virtual.py` (`fakedbapi`)
//! - `research/portage/lib/portage/tests/dbapi/test_fakedbapi.py`

use std::collections::BTreeMap;

use crate::atom::Atom;
use crate::matching::{Candidate, match_from_list};
use crate::version::cpv_cmp;

/// Which package store a [`PackageDb`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    /// Ebuilds available in a repository (the "porttree").
    Ebuild,
    /// Binary packages (the "bintree").
    Binary,
    /// Installed packages (the "vartree", `/var/db/pkg`).
    Installed,
}

/// One package's metadata, mirroring the `aux_get` keys emerge relies on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageMetadata {
    pub slot: Option<String>,
    pub sub_slot: Option<String>,
    pub repo: Option<String>,
    pub eapi: Option<String>,
    /// Flags declared in `IUSE` (with any `+`/`-` default prefix stripped).
    pub iuse: Vec<String>,
    /// Flags currently enabled (`USE`).
    pub use_enabled: Vec<String>,
    /// `KEYWORDS` tokens (e.g. `amd64`, `~x86`).
    pub keywords: Vec<String>,
    /// Dependency strings, keyed by variable name (`DEPEND`, `RDEPEND`, ...).
    pub deps: BTreeMap<String, String>,
}

impl PackageMetadata {
    /// Reads one `aux_get`-style metadata key into its raw string form.
    pub fn aux(&self, key: &str) -> Option<String> {
        match key {
            "SLOT" => Some(match (&self.slot, &self.sub_slot) {
                (Some(slot), Some(sub)) => format!("{slot}/{sub}"),
                (Some(slot), None) => slot.clone(),
                _ => "0".to_string(),
            }),
            "repository" => self.repo.clone(),
            "EAPI" => Some(self.eapi.clone().unwrap_or_else(|| "0".to_string())),
            "IUSE" => Some(self.iuse.join(" ")),
            "USE" => Some(self.use_enabled.join(" ")),
            "KEYWORDS" => Some(self.keywords.join(" ")),
            other => self.deps.get(other).cloned(),
        }
    }
}

/// One stored package: its `cpv` and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    pub cpv: String,
    pub metadata: PackageMetadata,
}

impl PackageEntry {
    /// Projects this entry into the [`Candidate`] view the matcher consumes.
    fn as_candidate(&self) -> Candidate {
        Candidate {
            cpv: self.cpv.clone(),
            slot: self.metadata.slot.clone(),
            sub_slot: self.metadata.sub_slot.clone(),
            repo: self.metadata.repo.clone(),
            use_enabled: self.metadata.use_enabled.iter().cloned().collect(),
            iuse: self.metadata.iuse.iter().cloned().collect(),
        }
    }
}

/// An in-memory package database. Insertion order is preserved for `cpv_all`
/// before sorting; `match` returns cpvs sorted by version (ascending), like
/// `fakedbapi.match`.
#[derive(Debug, Clone, Default)]
pub struct PackageDb {
    entries: Vec<PackageEntry>,
}

impl PackageDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a package by cpv (mirrors `fakedbapi.cpv_inject`).
    pub fn insert(&mut self, cpv: impl Into<String>, metadata: PackageMetadata) {
        let cpv = cpv.into();
        if let Some(existing) = self.entries.iter_mut().find(|e| e.cpv == cpv) {
            existing.metadata = metadata;
        } else {
            self.entries.push(PackageEntry { cpv, metadata });
        }
    }

    /// Removes a package by cpv (mirrors `fakedbapi.cpv_remove`).
    pub fn remove(&mut self, cpv: &str) {
        self.entries.retain(|e| e.cpv != cpv);
    }

    /// Returns true when no packages are stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of stored packages.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// All stored cpvs, sorted by category/package then version.
    pub fn cpv_all(&self) -> Vec<String> {
        let mut all: Vec<String> = self.entries.iter().map(|e| e.cpv.clone()).collect();
        all.sort_by(|a, b| cpv_cmp(a, b));
        all
    }

    /// Reads a metadata key for a cpv (mirrors `dbapi.aux_get`).
    pub fn aux_get(&self, cpv: &str, key: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.cpv == cpv)
            .and_then(|e| e.metadata.aux(key))
    }

    /// Returns the metadata for a cpv, if present.
    pub fn metadata(&self, cpv: &str) -> Option<&PackageMetadata> {
        self.entries
            .iter()
            .find(|e| e.cpv == cpv)
            .map(|e| &e.metadata)
    }

    /// Port of `fakedbapi.match`: returns the cpvs matching `atom`, sorted by
    /// version ascending.
    pub fn match_atom(&self, atom: &Atom) -> Vec<String> {
        let candidates: Vec<Candidate> = self
            .entries
            .iter()
            .map(PackageEntry::as_candidate)
            .collect();
        let mut matched: Vec<String> = match_from_list(atom, &candidates)
            .into_iter()
            .map(|c| c.cpv.clone())
            .collect();
        matched.sort_by(|a, b| cpv_cmp(a, b));
        matched
    }

    /// Convenience: parse `atom_str` and return the matching cpvs.
    pub fn match_str(&self, atom_str: &str) -> Result<Vec<String>, crate::atom::AtomError> {
        let atom = Atom::parse_with_options(
            atom_str,
            crate::atom::AtomParseOptions {
                allow_wildcard: true,
                allow_repo: true,
            },
        )?;
        Ok(self.match_atom(&atom))
    }
}
