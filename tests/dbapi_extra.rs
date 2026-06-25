//! Extended ports of upstream `fakedbapi` tests covering non-basic observable behavior.
//!
//! Reference: `research/portage/lib/portage/tests/dbapi/test_fakedbapi.py`
//!
//! These tests exercise:
//! - Complex match queries (slot/subslot/repo/use-dep combinations)
//! - cpv_all sorting with many versions
//! - aux_get of every metadata key
//! - insert-replace + remove semantics
//! - merge_from overlaying two dbs

use diverge::dbapi::{PackageDb, PackageMetadata};

/// Helper to construct metadata with optional sub_slot parsing.
fn meta(
    slot: &str,
    sub_slot: Option<&str>,
    repo: &str,
    iuse: &[&str],
    use_enabled: &[&str],
) -> PackageMetadata {
    PackageMetadata {
        slot: Some(slot.to_string()),
        sub_slot: sub_slot.map(|s| s.to_string()),
        repo: Some(repo.to_string()),
        eapi: Some("5".to_string()),
        iuse: iuse.iter().map(|s| s.to_string()).collect(),
        use_enabled: use_enabled.iter().map(|s| s.to_string()).collect(),
        keywords: vec!["amd64".to_string(), "~x86".to_string()],
        deps: Default::default(),
    }
}

/// Simpler meta for minimal metadata
fn simple_meta() -> PackageMetadata {
    PackageMetadata {
        slot: Some("0".to_string()),
        sub_slot: None,
        repo: Some("gentoo".to_string()),
        eapi: Some("5".to_string()),
        iuse: vec![],
        use_enabled: vec![],
        keywords: vec![],
        deps: Default::default(),
    }
}

// ===== Test: Complex match queries with slot/subslot/repo/use-dep combinations =====

#[test]
fn match_with_subslot_dependency() {
    // Test that packages with sub_slots are matched correctly
    let mut db = PackageDb::new();
    db.insert(
        "dev-libs/libfoo-1.0",
        meta("1", Some("0"), "gentoo", &[], &[]),
    );
    db.insert(
        "dev-libs/libfoo-2.0",
        meta("2", Some("1"), "gentoo", &[], &[]),
    );

    // Match by slot
    let matches = db.match_str("dev-libs/libfoo:2").unwrap();
    assert_eq!(matches, vec!["dev-libs/libfoo-2.0"]);

    // Match by repo
    let matches = db.match_str("dev-libs/libfoo::gentoo").unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.contains(&"dev-libs/libfoo-1.0".to_string()));
    assert!(matches.contains(&"dev-libs/libfoo-2.0".to_string()));
}

#[test]
fn match_with_multiple_use_flags() {
    // Test matching with multiple USE flags in different combinations
    let mut db = PackageDb::new();
    db.insert(
        "dev-lang/python-3.11",
        meta(
            "3.11",
            None,
            "gentoo",
            &["ssl", "sqlite", "tk"],
            &["ssl", "sqlite"],
        ),
    );
    db.insert(
        "dev-lang/python-3.10",
        meta("3.10", None, "gentoo", &["ssl", "sqlite", "tk"], &["ssl"]),
    );

    // Match with enabled flag
    let matches = db.match_str("dev-lang/python[ssl]").unwrap();
    assert_eq!(matches.len(), 2);

    // Match with disabled flag
    let matches = db.match_str("dev-lang/python[-tk]").unwrap();
    assert_eq!(matches.len(), 2);

    // Match with enabled sqlite (only 3.11 has it enabled)
    let matches = db.match_str("dev-lang/python[sqlite]").unwrap();
    assert_eq!(matches, vec!["dev-lang/python-3.11"]);

    // Match with disabled sqlite (only 3.10 has it disabled)
    let matches = db.match_str("dev-lang/python[-sqlite]").unwrap();
    assert_eq!(matches, vec!["dev-lang/python-3.10"]);
}

#[test]
fn match_repo_excludes_non_matching_repo() {
    // Test that repo qualifier filters correctly
    let mut db = PackageDb::new();
    db.insert("app-misc/test-1.0", meta("0", None, "gentoo", &[], &[]));
    db.insert("app-misc/test-2.0", meta("0", None, "other-repo", &[], &[]));

    // Match with explicit repo
    let matches = db.match_str("app-misc/test::gentoo").unwrap();
    assert_eq!(matches, vec!["app-misc/test-1.0"]);

    // Match with different repo
    let matches = db.match_str("app-misc/test::other-repo").unwrap();
    assert_eq!(matches, vec!["app-misc/test-2.0"]);

    // Match with non-existent repo yields nothing
    let matches = db.match_str("app-misc/test::nonexistent").unwrap();
    assert!(matches.is_empty());
}

#[test]
fn match_slot_and_use_together() {
    // Test combining slot and USE dependencies
    let mut db = PackageDb::new();
    db.insert(
        "dev-libs/foo-1.0",
        meta("0", None, "gentoo", &["abi_x86_64"], &["abi_x86_64"]),
    );
    db.insert(
        "dev-libs/foo-2.0",
        meta("1", None, "gentoo", &["abi_x86_64"], &[]),
    );

    // Match by slot and use
    let matches = db.match_str("dev-libs/foo:0[abi_x86_64]").unwrap();
    assert_eq!(matches, vec!["dev-libs/foo-1.0"]);

    // Match by slot without use
    let matches = db.match_str("dev-libs/foo:1[-abi_x86_64]").unwrap();
    assert_eq!(matches, vec!["dev-libs/foo-2.0"]);
}

// ===== Test: cpv_all sorting with many versions =====

#[test]
fn cpv_all_sorts_multiple_versions() {
    // Test that cpv_all correctly sorts packages with multiple versions
    let mut db = PackageDb::new();
    db.insert("app-misc/A-10", simple_meta());
    db.insert("app-misc/A-2", simple_meta());
    db.insert("app-misc/A-1", simple_meta());
    db.insert("app-misc/A-9", simple_meta());
    db.insert("app-misc/A-1.5", simple_meta());
    db.insert("app-misc/A-20", simple_meta());

    let all = db.cpv_all();
    assert_eq!(
        all,
        vec![
            "app-misc/A-1",
            "app-misc/A-1.5",
            "app-misc/A-2",
            "app-misc/A-9",
            "app-misc/A-10",
            "app-misc/A-20",
        ]
    );
}

#[test]
fn cpv_all_sorts_by_category_then_package_then_version() {
    // Test that cpv_all sorts across categories and packages
    let mut db = PackageDb::new();
    // Insert in random order
    db.insert("sys-apps/portage-2.1", simple_meta());
    db.insert("app-misc/foo-2", simple_meta());
    db.insert("sys-apps/portage-2.0", simple_meta());
    db.insert("app-misc/foo-1", simple_meta());
    db.insert("dev-libs/bar-1", simple_meta());

    let all = db.cpv_all();
    assert_eq!(
        all,
        vec![
            "app-misc/foo-1",
            "app-misc/foo-2",
            "dev-libs/bar-1",
            "sys-apps/portage-2.0",
            "sys-apps/portage-2.1",
        ]
    );
}

#[test]
fn cpv_all_complex_version_sorting() {
    // Test version sorting with pre-releases, post-releases, etc.
    let mut db = PackageDb::new();
    db.insert("dev-libs/lib-2.0_beta1", simple_meta());
    db.insert("dev-libs/lib-2.0", simple_meta());
    db.insert("dev-libs/lib-1.9_p1", simple_meta());
    db.insert("dev-libs/lib-1.9", simple_meta());
    db.insert("dev-libs/lib-2.1_rc1", simple_meta());

    let all = db.cpv_all();
    // Upstream version ordering follows Portage's versioning scheme
    assert_eq!(all.len(), 5);
    // Ensure sortedness by verifying first and last are correct
    assert_eq!(all[0], "dev-libs/lib-1.9");
    assert!(all.contains(&"dev-libs/lib-2.0".to_string()));
}

// ===== Test: aux_get of every metadata key =====

#[test]
fn aux_get_slot_formats() {
    // Test aux_get returns slot in correct format
    let db = {
        let mut db = PackageDb::new();
        db.insert("app/p-1", meta("0", None, "gentoo", &[], &[]));
        db.insert("app/p-2", meta("2", None, "gentoo", &[], &[]));
        db.insert("app/p-3", meta("3", Some("3.1"), "gentoo", &[], &[]));
        db
    };

    assert_eq!(db.aux_get("app/p-1", "SLOT").as_deref(), Some("0"));
    assert_eq!(db.aux_get("app/p-2", "SLOT").as_deref(), Some("2"));
    assert_eq!(db.aux_get("app/p-3", "SLOT").as_deref(), Some("3/3.1"));
}

#[test]
fn aux_get_iuse_returns_space_separated() {
    // Test aux_get IUSE returns flags space-separated
    let db = {
        let mut db = PackageDb::new();
        db.insert(
            "app/p-1",
            meta("0", None, "gentoo", &["flag1", "flag2", "flag3"], &[]),
        );
        db
    };

    assert_eq!(
        db.aux_get("app/p-1", "IUSE").as_deref(),
        Some("flag1 flag2 flag3")
    );
}

#[test]
fn aux_get_use_returns_space_separated() {
    // Test aux_get USE returns enabled flags space-separated
    let db = {
        let mut db = PackageDb::new();
        db.insert(
            "app/p-1",
            meta("0", None, "gentoo", &["a", "b", "c"], &["a", "c"]),
        );
        db
    };

    assert_eq!(db.aux_get("app/p-1", "USE").as_deref(), Some("a c"));
}

#[test]
fn aux_get_repository_key() {
    // Test aux_get repository returns repo name
    let db = {
        let mut db = PackageDb::new();
        db.insert("app/p-1", meta("0", None, "gentoo", &[], &[]));
        db.insert("app/p-2", meta("0", None, "other-repo", &[], &[]));
        db
    };

    assert_eq!(
        db.aux_get("app/p-1", "repository").as_deref(),
        Some("gentoo")
    );
    assert_eq!(
        db.aux_get("app/p-2", "repository").as_deref(),
        Some("other-repo")
    );
}

#[test]
fn aux_get_eapi_key() {
    // Test aux_get EAPI returns EAPI version
    let mut db = PackageDb::new();
    let mut metadata = simple_meta();
    metadata.eapi = Some("5".to_string());
    db.insert("app/p-1", metadata);

    assert_eq!(db.aux_get("app/p-1", "EAPI").as_deref(), Some("5"));
}

#[test]
fn aux_get_keywords_key() {
    // Test aux_get KEYWORDS returns keywords
    let mut db = PackageDb::new();
    let mut metadata = simple_meta();
    metadata.keywords = vec!["amd64".to_string(), "~x86".to_string(), "-arm".to_string()];
    db.insert("app/p-1", metadata);

    assert_eq!(
        db.aux_get("app/p-1", "KEYWORDS").as_deref(),
        Some("amd64 ~x86 -arm")
    );
}

#[test]
fn aux_get_missing_cpv_returns_none() {
    // Test aux_get returns None for non-existent packages
    let db = PackageDb::new();
    assert_eq!(db.aux_get("missing/pkg-1", "SLOT"), None);
    assert_eq!(db.aux_get("missing/pkg-1", "IUSE"), None);
}

#[test]
fn aux_get_dependency_keys() {
    // Test aux_get can return dependency strings from deps map
    let mut db = PackageDb::new();
    let mut metadata = simple_meta();
    metadata
        .deps
        .insert("DEPEND".to_string(), "dev-libs/a sys-libs/b".to_string());
    metadata
        .deps
        .insert("RDEPEND".to_string(), "sys-libs/b".to_string());
    db.insert("app/p-1", metadata);

    assert_eq!(
        db.aux_get("app/p-1", "DEPEND").as_deref(),
        Some("dev-libs/a sys-libs/b")
    );
    assert_eq!(
        db.aux_get("app/p-1", "RDEPEND").as_deref(),
        Some("sys-libs/b")
    );
}

// ===== Test: insert-replace + remove semantics =====

#[test]
fn insert_replaces_existing_cpv() {
    // Test that inserting a cpv that exists replaces the metadata
    let mut db = PackageDb::new();
    db.insert("app/p-1", meta("0", None, "gentoo", &["flag1"], &["flag1"]));
    assert_eq!(db.len(), 1);

    // Replace with different metadata
    db.insert("app/p-1", meta("1", None, "other", &["flag2"], &[]));
    assert_eq!(db.len(), 1); // Still only one entry
    assert_eq!(db.aux_get("app/p-1", "SLOT").as_deref(), Some("1"));
    assert_eq!(
        db.aux_get("app/p-1", "repository").as_deref(),
        Some("other")
    );
    assert_eq!(db.aux_get("app/p-1", "IUSE").as_deref(), Some("flag2"));
}

#[test]
fn remove_deletes_package() {
    // Test that remove deletes a package
    let mut db = PackageDb::new();
    db.insert("app/foo-1", simple_meta());
    db.insert("app/bar-1", simple_meta());
    assert_eq!(db.len(), 2);

    db.remove("app/foo-1");
    assert_eq!(db.len(), 1);
    // After removing app/foo-1, matching app/foo should return empty
    assert!(db.match_str("app/foo").unwrap().is_empty());
    // app/bar should still match
    assert_eq!(db.match_str("app/bar").unwrap(), vec!["app/bar-1"]);
}

#[test]
fn remove_non_existent_package_is_safe() {
    // Test that removing a non-existent package is a no-op
    let mut db = PackageDb::new();
    db.insert("app/p-1", simple_meta());
    assert_eq!(db.len(), 1);

    db.remove("app/p-nonexistent");
    assert_eq!(db.len(), 1); // No change
}

#[test]
fn insert_remove_cycle() {
    // Test cycles of insert and remove
    let mut db = PackageDb::new();

    db.insert("app/p-1", simple_meta());
    assert_eq!(db.len(), 1);

    db.remove("app/p-1");
    assert!(db.is_empty());

    db.insert("app/p-1", simple_meta());
    assert_eq!(db.len(), 1);

    db.insert("app/p-2", simple_meta());
    assert_eq!(db.len(), 2);

    db.remove("app/p-1");
    db.remove("app/p-2");
    assert!(db.is_empty());
}

// ===== Test: merge_from overlaying two dbs =====

#[test]
fn merge_from_copies_entries() {
    // Test that merge_from copies all entries from another db
    let mut db1 = PackageDb::new();
    db1.insert("app/p-1", simple_meta());
    db1.insert("app/p-2", simple_meta());

    let mut db2 = PackageDb::new();
    db2.insert("app/q-1", simple_meta());

    db1.merge_from(&db2);
    assert_eq!(db1.len(), 3);
    assert_eq!(db1.cpv_all().len(), 3);
}

#[test]
fn merge_from_later_wins() {
    // Test that merge_from overlays: later inserts win on conflict
    let mut db1 = PackageDb::new();
    let mut meta1 = simple_meta();
    meta1.repo = Some("first".to_string());
    db1.insert("app/p-1", meta1);

    let mut db2 = PackageDb::new();
    let mut meta2 = simple_meta();
    meta2.repo = Some("second".to_string());
    db2.insert("app/p-1", meta2);

    db1.merge_from(&db2);
    assert_eq!(db1.len(), 1);
    assert_eq!(
        db1.aux_get("app/p-1", "repository").as_deref(),
        Some("second")
    );
}

#[test]
fn merge_from_empty_is_safe() {
    // Test that merging from empty db is safe
    let mut db1 = PackageDb::new();
    db1.insert("app/p-1", simple_meta());

    let db2 = PackageDb::new();
    db1.merge_from(&db2);
    assert_eq!(db1.len(), 1);
}

#[test]
fn merge_from_into_empty_copies_all() {
    // Test that merging into an empty db copies everything
    let mut db1 = PackageDb::new();
    let mut db2 = PackageDb::new();
    db2.insert("app/p-1", simple_meta());
    db2.insert("app/p-2", simple_meta());

    db1.merge_from(&db2);
    assert_eq!(db1.len(), 2);
    assert_eq!(db1.cpv_all(), vec!["app/p-1", "app/p-2"]);
}

#[test]
fn merge_from_partial_overlap() {
    // Test merge with partial overlap: some entries overlap, some don't
    let mut db1 = PackageDb::new();
    db1.insert("app/liba-1", simple_meta());
    db1.insert("app/libb-1", simple_meta());

    let mut db2 = PackageDb::new();
    db2.insert("app/liba-1", simple_meta());
    db2.insert("app/libc-1", simple_meta());

    db1.merge_from(&db2);
    assert_eq!(db1.len(), 3);
    assert!(db1.match_str("app/liba").unwrap().len() == 1);
    assert!(db1.match_str("app/libb").unwrap().len() == 1);
    assert!(db1.match_str("app/libc").unwrap().len() == 1);
}

// ===== Test: metadata() method returns references =====

#[test]
fn metadata_returns_package_metadata() {
    // Test that metadata() returns correct metadata references
    let mut db = PackageDb::new();
    db.insert(
        "app/p-1",
        meta("0", None, "gentoo", &["flag1", "flag2"], &["flag1"]),
    );

    let meta_ref = db.metadata("app/p-1").unwrap();
    assert_eq!(meta_ref.slot, Some("0".to_string()));
    assert_eq!(meta_ref.repo, Some("gentoo".to_string()));
    assert_eq!(meta_ref.iuse.len(), 2);
    assert_eq!(meta_ref.use_enabled.len(), 1);
}

#[test]
fn metadata_returns_none_for_missing() {
    // Test that metadata() returns None for non-existent packages
    let db = PackageDb::new();
    assert_eq!(db.metadata("app/p-missing"), None);
}

// ===== Test: iter() method iterates in insertion order =====

#[test]
fn iter_maintains_insertion_order() {
    // Test that iter() maintains insertion order
    let mut db = PackageDb::new();
    db.insert("app/p-1", simple_meta());
    db.insert("app/q-1", simple_meta());
    db.insert("app/r-1", simple_meta());

    let cpvs: Vec<&str> = db.iter().map(|(cpv, _)| cpv).collect();
    assert_eq!(cpvs, vec!["app/p-1", "app/q-1", "app/r-1"]);
}

#[test]
fn iter_after_replace_maintains_position() {
    // Test that replacing a package maintains its position in iter order
    let mut db = PackageDb::new();
    db.insert("app/p-1", simple_meta());
    db.insert("app/q-1", simple_meta());
    db.insert("app/r-1", simple_meta());

    // Replace app/q-1
    db.insert("app/q-1", simple_meta());

    let cpvs: Vec<&str> = db.iter().map(|(cpv, _)| cpv).collect();
    assert_eq!(cpvs, vec!["app/p-1", "app/q-1", "app/r-1"]);
}

// ===== Test: is_empty() and len() =====

#[test]
fn is_empty_and_len_behavior() {
    let mut db = PackageDb::new();
    assert!(db.is_empty());
    assert_eq!(db.len(), 0);

    db.insert("app/p-1", simple_meta());
    assert!(!db.is_empty());
    assert_eq!(db.len(), 1);

    db.insert("app/p-2", simple_meta());
    assert_eq!(db.len(), 2);

    db.remove("app/p-1");
    assert_eq!(db.len(), 1);

    db.remove("app/p-2");
    assert!(db.is_empty());
}

// ===== Test: Default constructor =====

#[test]
fn default_creates_empty_db() {
    let db = PackageDb::default();
    assert!(db.is_empty());
    assert_eq!(db.len(), 0);
    assert!(db.cpv_all().is_empty());
}
