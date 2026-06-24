//! Integration test: load a fixture ebuild repository into a PackageDb.
//!
//! Reference: `research/portage/lib/portage/tests/resolver/ResolverPlayground.py`
//! (`_create_ebuilds` layout: `<repo>/<cat>/<pkg>/<pkg>-<ver>.ebuild` plus
//! `<repo>/profiles/repo_name`).

use std::fs;

use diverge::repository::Repository;

use crate::fs_fixture::write;

fn ebuild(metadata: &[(&str, &str)]) -> String {
    metadata
        .iter()
        .map(|(k, v)| format!("{k}=\"{v}\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn fixture_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    write(&repo.join("profiles/repo_name"), "test_repo\n");

    write(
        &repo.join("dev-libs/A/A-1.ebuild"),
        &ebuild(&[
            ("EAPI", "5"),
            ("SLOT", "0"),
            ("KEYWORDS", "x86 amd64"),
            ("IUSE", "+foo bar"),
            ("DEPEND", "dev-libs/B"),
        ]),
    );
    write(
        &repo.join("dev-libs/A/A-2.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "2/2"), ("KEYWORDS", "~x86")]),
    );
    write(
        &repo.join("dev-libs/B/B-1.ebuild"),
        &ebuild(&[("SLOT", "0"), ("KEYWORDS", "x86")]),
    );
    dir
}

#[test]
fn repository_loads_name_and_packages() {
    let dir = fixture_repo();
    let repo = Repository::load(dir.path()).expect("load repo");
    assert_eq!(repo.name, "test_repo");
    assert_eq!(
        repo.db.cpv_all(),
        vec![
            "dev-libs/A-1".to_string(),
            "dev-libs/A-2".to_string(),
            "dev-libs/B-1".to_string(),
        ]
    );
}

#[test]
fn repository_reads_ebuild_metadata() {
    let dir = fixture_repo();
    let repo = Repository::load(dir.path()).expect("load repo");

    let meta = repo.db.metadata("dev-libs/A-1").expect("A-1 metadata");
    assert_eq!(meta.slot.as_deref(), Some("0"));
    assert_eq!(meta.eapi.as_deref(), Some("5"));
    assert_eq!(meta.repo.as_deref(), Some("test_repo"));
    // IUSE default markers are stripped.
    assert_eq!(meta.iuse, vec!["foo".to_string(), "bar".to_string()]);
    assert_eq!(meta.keywords, vec!["x86".to_string(), "amd64".to_string()]);
    assert_eq!(
        meta.deps.get("DEPEND").map(String::as_str),
        Some("dev-libs/B")
    );

    // Sub-slot is parsed from SLOT="2/2".
    let meta = repo.db.metadata("dev-libs/A-2").expect("A-2 metadata");
    assert_eq!(meta.slot.as_deref(), Some("2"));
    assert_eq!(meta.sub_slot.as_deref(), Some("2"));
}

#[test]
fn repository_match_uses_loaded_db() {
    let dir = fixture_repo();
    let repo = Repository::load(dir.path()).expect("load repo");
    assert_eq!(
        repo.db.match_str("dev-libs/A").unwrap(),
        vec!["dev-libs/A-1", "dev-libs/A-2"]
    );
    assert_eq!(
        repo.db.match_str(">=dev-libs/A-2").unwrap(),
        vec!["dev-libs/A-2"]
    );
    // Aux read of a dependency string.
    assert_eq!(
        repo.db.aux_get("dev-libs/A-1", "DEPEND").as_deref(),
        Some("dev-libs/B")
    );
}

#[test]
fn repository_requires_repo_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    // No profiles/repo_name.
    fs::create_dir_all(dir.path().join("dev-libs/A")).expect("mkdir");
    let err = Repository::load(dir.path()).expect_err("must require repo_name");
    assert!(format!("{err}").contains("repo_name"), "got: {err}");
}
