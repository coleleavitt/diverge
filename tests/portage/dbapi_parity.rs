//! Ports of upstream `fakedbapi` match behavior.
//!
//! Reference: `research/portage/lib/portage/tests/dbapi/test_fakedbapi.py`

use diverge::dbapi::{PackageDb, PackageMetadata};

fn meta(slot: &str, repo: &str, iuse: &[&str], use_enabled: &[&str]) -> PackageMetadata {
    let (slot_name, sub) = match slot.split_once('/') {
        Some((s, sub)) => (s.to_string(), Some(sub.to_string())),
        None => (slot.to_string(), None),
    };
    PackageMetadata {
        slot: Some(slot_name),
        sub_slot: sub,
        repo: Some(repo.to_string()),
        eapi: Some("5".to_string()),
        iuse: iuse.iter().map(|s| s.to_string()).collect(),
        use_enabled: use_enabled.iter().map(|s| s.to_string()).collect(),
        keywords: Vec::new(),
        deps: Default::default(),
    }
}

fn fixture() -> PackageDb {
    let mut db = PackageDb::new();
    // app-misc/foo-2 (EAPI 5): IUSE_EFFECTIVE lets a USE flag match even if not
    // declared in IUSE; modeled here by enabling it without IUSE membership.
    db.insert(
        "app-misc/foo-2",
        meta("2", "gentoo", &["missing-iuse"], &["missing-iuse"]),
    );
    db.insert(
        "sys-apps/portage-2.1.10",
        meta("0", "gentoo", &["ipc", "doc"], &["ipc"]),
    );
    db.insert("virtual/package-manager-0", meta("0", "gentoo", &[], &[]));
    db
}

#[test]
fn fakedbapi_match_slot_and_use() {
    let db = fixture();
    assert_eq!(
        db.match_str("app-misc/foo[missing-iuse]").unwrap(),
        vec!["app-misc/foo-2"]
    );
    assert_eq!(
        db.match_str("sys-apps/portage:0[ipc]").unwrap(),
        vec!["sys-apps/portage-2.1.10"]
    );
    assert!(db.match_str("sys-apps/portage:0[-ipc]").unwrap().is_empty());
    assert!(db.match_str("sys-apps/portage:0[doc]").unwrap().is_empty());
    assert_eq!(
        db.match_str("sys-apps/portage:0[-doc]").unwrap(),
        vec!["sys-apps/portage-2.1.10"]
    );
    assert_eq!(
        db.match_str("sys-apps/portage:0").unwrap(),
        vec!["sys-apps/portage-2.1.10"]
    );
}

#[test]
fn fakedbapi_match_repo_qualifier() {
    let db = fixture();
    assert_eq!(
        db.match_str("sys-apps/portage:0::gentoo[ipc]").unwrap(),
        vec!["sys-apps/portage-2.1.10"]
    );
    assert!(
        db.match_str("sys-apps/portage:0::multilib[ipc]")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn fakedbapi_match_virtual_and_cpv_all() {
    let db = fixture();
    assert_eq!(
        db.match_str("virtual/package-manager").unwrap(),
        vec!["virtual/package-manager-0"]
    );
    // cpv_all returns every stored package sorted.
    assert_eq!(
        db.cpv_all(),
        vec![
            "app-misc/foo-2".to_string(),
            "sys-apps/portage-2.1.10".to_string(),
            "virtual/package-manager-0".to_string(),
        ]
    );
}

#[test]
fn fakedbapi_match_sorted_by_version() {
    let mut db = PackageDb::new();
    db.insert("dev-libs/A-2", meta("0", "gentoo", &[], &[]));
    db.insert("dev-libs/A-1", meta("0", "gentoo", &[], &[]));
    db.insert("dev-libs/A-1.5", meta("0", "gentoo", &[], &[]));
    // match returns cpvs sorted by version ascending.
    assert_eq!(
        db.match_str("dev-libs/A").unwrap(),
        vec!["dev-libs/A-1", "dev-libs/A-1.5", "dev-libs/A-2"]
    );
}

#[test]
fn aux_get_reads_metadata_keys() {
    let db = fixture();
    assert_eq!(
        db.aux_get("sys-apps/portage-2.1.10", "SLOT").as_deref(),
        Some("0")
    );
    assert_eq!(
        db.aux_get("sys-apps/portage-2.1.10", "IUSE").as_deref(),
        Some("ipc doc")
    );
    assert_eq!(
        db.aux_get("sys-apps/portage-2.1.10", "repository")
            .as_deref(),
        Some("gentoo")
    );
    assert_eq!(db.aux_get("missing/pkg-1", "SLOT"), None);
}

#[test]
fn insert_replaces_and_remove_deletes() {
    let mut db = PackageDb::new();
    db.insert("dev-libs/A-1", meta("0", "gentoo", &[], &[]));
    db.insert("dev-libs/A-1", meta("3", "gentoo", &[], &[]));
    assert_eq!(db.len(), 1);
    assert_eq!(db.aux_get("dev-libs/A-1", "SLOT").as_deref(), Some("3"));
    db.remove("dev-libs/A-1");
    assert!(db.is_empty());
}
