//! Shared fixture builders for resolver/depgraph parity tests.
//!
//! These construct an in-memory [`PackageDb`] the way upstream's
//! ResolverPlayground builds `fakedbapi` stores: a cpv plus metadata with a
//! default SLOT/KEYWORDS and a set of dependency strings.
//!
//! Included via `#[path]` into multiple test files; not every file uses every
//! helper, so individual unused helpers are allowed.

use diverge::dbapi::{PackageDb, PackageMetadata};

/// Builds package metadata with SLOT=0, KEYWORDS=x86, EAPI=7 and the given
/// dependency variables (e.g. `[("RDEPEND", "dev-libs/B")]`).
#[allow(dead_code)]
pub fn pkg(deps: &[(&str, &str)]) -> PackageMetadata {
    let mut meta = PackageMetadata {
        slot: Some("0".to_string()),
        sub_slot: None,
        repo: Some("test_repo".to_string()),
        eapi: Some("7".to_string()),
        iuse: Vec::new(),
        use_enabled: Vec::new(),
        keywords: vec!["x86".to_string()],
        deps: Default::default(),
    };
    for (k, v) in deps {
        meta.deps.insert((*k).to_string(), (*v).to_string());
    }
    meta
}

/// Builds metadata with an explicit SLOT (and optional sub-slot via `slot/sub`).
#[allow(dead_code)]
pub fn pkg_slot(slot: &str, deps: &[(&str, &str)]) -> PackageMetadata {
    let mut meta = pkg(deps);
    match slot.split_once('/') {
        Some((s, sub)) => {
            meta.slot = Some(s.to_string());
            meta.sub_slot = Some(sub.to_string());
        }
        None => meta.slot = Some(slot.to_string()),
    }
    meta
}

/// Builds a [`PackageDb`] from `(cpv, metadata)` entries.
#[allow(dead_code)]
pub fn db(entries: &[(&str, PackageMetadata)]) -> PackageDb {
    let mut db = PackageDb::new();
    for (cpv, meta) in entries {
        db.insert(*cpv, meta.clone());
    }
    db
}
