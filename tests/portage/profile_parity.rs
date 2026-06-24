//! Integration tests for profile parent-chain resolution and stacked settings.
//!
//! Reference:
//! - `research/portage/lib/portage/package/ebuild/_config/LocationsManager.py`
//!   (`_addProfile` parent-chain ordering)
//! - `research/portage/lib/portage/package/ebuild/config.py` (incremental
//!   stacking of `make.defaults`, `package.use`, `package.mask`)

use diverge::profile::{ProfileStack, StackedProfile};

use crate::fs_fixture::write;

/// Builds a base <- middle <- leaf profile chain in a tempdir and returns the
/// leaf directory path along with the tempdir guard.
fn three_level_tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    // base profile.
    write(
        &root.join("base/make.defaults"),
        "USE=\"foo bar\"\nARCH=\"x86\"\n",
    );
    write(&root.join("base/packages"), "*sys-apps/portage\n");
    write(&root.join("base/package.use"), "dev-libs/A foo\n");
    write(&root.join("base/package.mask"), "dev-libs/evil\n");

    // middle profile inherits base.
    write(&root.join("middle/parent"), "../base\n");
    write(&root.join("middle/make.defaults"), "USE=\"baz\"\n");
    write(&root.join("middle/package.use"), "dev-libs/A bar\n");

    // leaf profile inherits middle, removes a USE flag and unmasks.
    write(&root.join("leaf/parent"), "../middle\n");
    write(&root.join("leaf/make.defaults"), "USE=\"-bar qux\"\n");
    write(&root.join("leaf/package.mask"), "-dev-libs/evil\n");

    let leaf = root.join("leaf");
    (dir, leaf)
}

#[test]
fn parent_chain_orders_parents_before_children() {
    let (_guard, leaf) = three_level_tree();
    let stack = ProfileStack::resolve(&leaf).expect("resolve chain");
    let names: Vec<String> = stack
        .profiles
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["base", "middle", "leaf"]);
}

#[test]
fn make_defaults_use_is_incrementally_stacked() {
    let (_guard, leaf) = three_level_tree();
    let profile = StackedProfile::from_dir(&leaf).expect("load profile");
    // USE accumulates across the chain (incremental) and `-bar` removes bar.
    let resolved = profile.incremental_tokens("USE");
    let flags: std::collections::BTreeSet<&str> = resolved.iter().map(String::as_str).collect();
    assert!(flags.contains("foo"), "foo retained: {resolved:?}");
    assert!(flags.contains("baz"), "baz from middle: {resolved:?}");
    assert!(flags.contains("qux"), "qux from leaf: {resolved:?}");
    assert!(!flags.contains("bar"), "bar removed by -bar: {resolved:?}");
    // Non-incremental var: leaf chain keeps base's ARCH.
    assert_eq!(
        profile.variables.get("ARCH").map(String::as_str),
        Some("x86")
    );
}

#[test]
fn package_use_and_mask_stack_across_profiles() {
    let (_guard, leaf) = three_level_tree();
    let profile = StackedProfile::from_dir(&leaf).expect("load profile");

    // package.use for dev-libs/A accumulates foo (base) + bar (middle).
    let a_flags = profile
        .package_use
        .get("dev-libs/A")
        .expect("dev-libs/A use");
    assert!(a_flags.contains(&"foo".to_string()));
    assert!(a_flags.contains(&"bar".to_string()));

    // package.mask: base masks dev-libs/evil, leaf unmasks it -> empty.
    assert!(
        !profile.package_mask.contains(&"dev-libs/evil".to_string()),
        "mask removed by -dev-libs/evil: {:?}",
        profile.package_mask
    );

    // system set picks up the *-prefixed packages line.
    assert!(profile.system_set.contains(&"sys-apps/portage".to_string()));
}

#[test]
fn empty_parent_file_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leaf = dir.path().join("leaf");
    write(&leaf.join("parent"), "\n# only a comment\n");
    let err = ProfileStack::resolve(&leaf).expect_err("empty parent must error");
    assert!(format!("{err}").contains("empty parent"), "got: {err}");
}

#[test]
fn missing_parent_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leaf = dir.path().join("leaf");
    write(&leaf.join("parent"), "../does-not-exist\n");
    let err = ProfileStack::resolve(&leaf).expect_err("missing parent must error");
    assert!(format!("{err}").contains("not found"), "got: {err}");
}
