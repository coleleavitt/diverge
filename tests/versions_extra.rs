//! Additional test cases ported from upstream Portage Python tests.
//! Source files:
//! - research/portage/lib/portage/tests/versions/test_vercmp.py
//! - research/portage/lib/portage/tests/versions/test_cpv_sort_key.py

use std::cmp::Ordering;

use diverge::version::{cpv_cmp, sort_cpvs, split_cpv, vercmp};

// ===== vercmp edge cases =====

#[test]
fn vercmp_large_numbers() {
    // Test handling of very large numeric versions
    assert_eq!(
        vercmp(
            "999999999999999999999999999999",
            "999999999999999999999999999998"
        ),
        Ordering::Greater
    );
    assert_eq!(
        vercmp(
            "999999999999999999999999999998",
            "999999999999999999999999999999"
        ),
        Ordering::Less
    );
}

#[test]
fn vercmp_single_digit_vs_float() {
    // Test comparison of single digit against floating point version
    assert_eq!(vercmp("5.0", "5"), Ordering::Greater);
    assert_eq!(vercmp("5", "5.0"), Ordering::Less);
    assert_eq!(vercmp("4.0", "5.0"), Ordering::Less);
}

#[test]
fn vercmp_revision_only() {
    // Test revision number comparisons
    assert_eq!(vercmp("1.0-r1", "1.0-r0"), Ordering::Greater);
    assert_eq!(vercmp("1.0-r0", "1.0-r1"), Ordering::Less);
    assert_eq!(vercmp("1.0-r1", "1.0"), Ordering::Greater);
    assert_eq!(vercmp("1.0", "1.0-r1"), Ordering::Less);
    assert_eq!(vercmp("1.0-r0", "1.0"), Ordering::Equal);
}

#[test]
fn vercmp_suffix_hierarchy() {
    // Test suffix ordering: pre < alpha < beta < rc < p (no suffix)
    assert_eq!(vercmp("1.0_pre2", "1.0_p2"), Ordering::Less);
    assert_eq!(vercmp("1.0_alpha2", "1.0_p2"), Ordering::Less);
    assert_eq!(vercmp("1.0_alpha1", "1.0_beta1"), Ordering::Less);
    assert_eq!(vercmp("1.0_beta3", "1.0_rc3"), Ordering::Less);
    // Upstream: alpha(-4) < pre(-2), so "1.0_alpha" < "1.0_pre"
    assert_eq!(vercmp("1.0_alpha", "1.0_pre"), Ordering::Less);
    // Upstream: beta(-3) > alpha(-4), so "1.0_beta" > "1.0_alpha"
    assert_eq!(vercmp("1.0_beta", "1.0_alpha"), Ordering::Greater);
}

#[test]
fn vercmp_base_with_letters() {
    // Test base version with letter suffixes
    assert_eq!(vercmp("1.0.0", "1.0b"), Ordering::Greater);
    assert_eq!(vercmp("1b", "1"), Ordering::Greater);
    assert_eq!(vercmp("1.0b", "1.0.0"), Ordering::Less);
    assert_eq!(vercmp("1", "1b"), Ordering::Less);
}

#[test]
fn vercmp_letter_and_suffix_combination() {
    // Test combination of letter suffix and underscore suffix
    assert_eq!(vercmp("1b_p1", "1_p1"), Ordering::Greater);
    assert_eq!(vercmp("1_p1", "1b_p1"), Ordering::Less);
    assert_eq!(vercmp("1.1b", "1.1"), Ordering::Greater);
    assert_eq!(vercmp("1.1", "1.1b"), Ordering::Less);
}

#[test]
fn vercmp_dotted_versions() {
    // Test complex dotted version comparisons
    assert_eq!(vercmp("1.0.0", "1.0"), Ordering::Greater);
    assert_eq!(vercmp("1.0", "1.0.0"), Ordering::Less);
    assert_eq!(vercmp("1.01", "1.1"), Ordering::Less);
    assert_eq!(vercmp("12.2.5", "12.2b"), Ordering::Greater);
    assert_eq!(vercmp("12.2b", "12.2.5"), Ordering::Less);
}

#[test]
fn vercmp_leading_zeros() {
    // Test leading zero handling in numeric parts
    assert_eq!(
        vercmp("1.001000000000000000001", "1.001000000000000000002"),
        Ordering::Less
    );
    assert_eq!(
        vercmp("1.00100000000", "1.0010000000000000001"),
        Ordering::Less
    );
}

#[test]
fn vercmp_zero_versions() {
    // Test zero version comparison
    assert_eq!(vercmp("0", "0.0"), Ordering::Less);
    assert_eq!(vercmp("0.0", "0"), Ordering::Greater);
}

#[test]
fn vercmp_equal_same_version() {
    // Test identical version strings
    assert_eq!(vercmp("4.0", "4.0"), Ordering::Equal);
    assert_eq!(vercmp("1.0", "1.0"), Ordering::Equal);
    assert_eq!(vercmp("1.0-r0", "1.0"), Ordering::Equal);
    assert_eq!(vercmp("1.0", "1.0-r0"), Ordering::Equal);
    assert_eq!(vercmp("1.0-r0", "1.0-r0"), Ordering::Equal);
    assert_eq!(vercmp("1.0-r1", "1.0-r1"), Ordering::Equal);
}

#[test]
fn vercmp_not_equal_detection() {
    // Test detection of non-equal versions
    assert_ne!(vercmp("1", "2"), Ordering::Equal);
    assert_ne!(vercmp("1.0_alpha", "1.0_pre"), Ordering::Equal);
    assert_ne!(vercmp("1.0_beta", "1.0_alpha"), Ordering::Equal);
    assert_ne!(vercmp("0", "0.0"), Ordering::Equal);
    assert_ne!(vercmp("1.0-r0", "1.0-r1"), Ordering::Equal);
    assert_ne!(vercmp("1.0-r1", "1.0-r0"), Ordering::Equal);
    assert_ne!(vercmp("1.0", "1.0-r1"), Ordering::Equal);
    assert_ne!(vercmp("1.0-r1", "1.0"), Ordering::Equal);
    assert_ne!(vercmp("1.0", "1.0.0"), Ordering::Equal);
    assert_ne!(vercmp("1_p1", "1b_p1"), Ordering::Equal);
    assert_ne!(vercmp("1b", "1"), Ordering::Equal);
    assert_ne!(vercmp("1.1b", "1.1"), Ordering::Equal);
    assert_ne!(vercmp("12.2b", "12.2"), Ordering::Equal);
}

// ===== split_cpv edge cases =====

#[test]
fn split_cpv_with_version() {
    // Test splitting CPV strings with versions
    let (cp, ver) = split_cpv("dev-libs/A-1.0");
    assert_eq!(cp, "dev-libs/A");
    assert_eq!(ver, Some("1.0".to_string()));
}

#[test]
fn split_cpv_without_version() {
    // Test splitting CPV strings without versions (bare categories/packages)
    let (cp, ver) = split_cpv("dev-libs/A");
    assert_eq!(cp, "dev-libs/A");
    assert_eq!(ver, None);
}

#[test]
fn split_cpv_with_revision() {
    // Test splitting CPV with revision number
    let (cp, ver) = split_cpv("app-misc/foo-1.2.3-r5");
    assert_eq!(cp, "app-misc/foo");
    assert_eq!(ver, Some("1.2.3-r5".to_string()));
}

#[test]
fn split_cpv_with_suffix() {
    // Test splitting CPV with version suffix
    let (cp, ver) = split_cpv("sys-libs/bar-2.1_alpha3");
    assert_eq!(cp, "sys-libs/bar");
    assert_eq!(ver, Some("2.1_alpha3".to_string()));
}

#[test]
fn split_cpv_hyphenated_package() {
    // Test splitting CPV with hyphenated package name and version
    let (cp, ver) = split_cpv("dev-util/pkg-config-0.29.2");
    assert_eq!(cp, "dev-util/pkg-config");
    assert_eq!(ver, Some("0.29.2".to_string()));
}

#[test]
fn split_cpv_complex_version() {
    // Test complex version with multiple dots and suffixes
    let (cp, ver) = split_cpv("x11-base/xorg-server-21.1.4_p1-r1");
    assert_eq!(cp, "x11-base/xorg-server");
    assert_eq!(ver, Some("21.1.4_p1-r1".to_string()));
}

// ===== cpv_cmp comparisons =====

#[test]
fn cpv_cmp_different_categories() {
    // CPVs with different categories should sort by category first
    assert_eq!(cpv_cmp("a/pkg", "b/pkg"), Ordering::Less);
    assert_eq!(cpv_cmp("b/pkg", "a/pkg"), Ordering::Greater);
}

#[test]
fn cpv_cmp_same_category_different_packages() {
    // CPVs with same category, different packages sort by package
    assert_eq!(cpv_cmp("a/a", "a/b"), Ordering::Less);
    assert_eq!(cpv_cmp("a/b", "a/a"), Ordering::Greater);
}

#[test]
fn cpv_cmp_same_cp_different_versions() {
    // Same CP but different versions sorts by version
    assert_eq!(cpv_cmp("a/b-1", "a/b-2"), Ordering::Less);
    assert_eq!(cpv_cmp("a/b-2", "a/b-1"), Ordering::Greater);
}

#[test]
fn cpv_cmp_versioned_vs_unversioned() {
    // Unversioned should come before versioned
    assert_eq!(cpv_cmp("a/b", "a/b-1"), Ordering::Less);
    assert_eq!(cpv_cmp("a/b-1", "a/b"), Ordering::Greater);
}

#[test]
fn cpv_cmp_equal_cpvs() {
    // Identical CPVs should be equal
    assert_eq!(cpv_cmp("a/b-1", "a/b-1"), Ordering::Equal);
    assert_eq!(cpv_cmp("a/b", "a/b"), Ordering::Equal);
}

// ===== sort_cpvs integration tests =====

#[test]
fn sort_cpvs_mixed_order() {
    // Test from upstream: sort multiple CPVs including unversioned entries
    let mut values: Vec<String> = ["a/b-2_alpha", "a", "b", "a/b-2", "a/a-1", "a/b-1"]
        .into_iter()
        .map(String::from)
        .collect();
    sort_cpvs(&mut values);
    assert_eq!(values, ["a", "a/a-1", "a/b-1", "a/b-2_alpha", "a/b-2", "b"]);
}

#[test]
fn sort_cpvs_version_ordering() {
    // Test that versions within same CP are ordered correctly
    let mut values: Vec<String> = [
        "dev-libs/foo-2.0",
        "dev-libs/foo-1.0",
        "dev-libs/foo-1.5",
        "dev-libs/foo-2.0_alpha",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    sort_cpvs(&mut values);
    assert_eq!(
        values,
        [
            "dev-libs/foo-1.0",
            "dev-libs/foo-1.5",
            "dev-libs/foo-2.0_alpha",
            "dev-libs/foo-2.0"
        ]
    );
}

#[test]
fn sort_cpvs_category_then_package() {
    // Test that sorting by category happens before package name
    let mut values: Vec<String> = [
        "z-cat/apkg-1",
        "a-cat/zpkg-1",
        "a-cat/apkg-1",
        "z-cat/zpkg-1",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    sort_cpvs(&mut values);
    assert_eq!(
        values,
        [
            "a-cat/apkg-1",
            "a-cat/zpkg-1",
            "z-cat/apkg-1",
            "z-cat/zpkg-1"
        ]
    );
}

#[test]
fn sort_cpvs_with_revisions() {
    // Test sorting with revision numbers
    let mut values: Vec<String> = [
        "app/pkg-1.0-r2",
        "app/pkg-1.0-r1",
        "app/pkg-1.0",
        "app/pkg-1.0-r3",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    sort_cpvs(&mut values);
    assert_eq!(
        values,
        [
            "app/pkg-1.0",
            "app/pkg-1.0-r1",
            "app/pkg-1.0-r2",
            "app/pkg-1.0-r3"
        ]
    );
}

#[test]
fn sort_cpvs_mixed_versioned_unversioned() {
    // Test sorting with mix of versioned and unversioned entries
    let mut values: Vec<String> = ["cat/pkg-2", "cat/pkg", "cat/other", "cat/pkg-1"]
        .into_iter()
        .map(String::from)
        .collect();
    sort_cpvs(&mut values);
    assert_eq!(values, ["cat/other", "cat/pkg", "cat/pkg-1", "cat/pkg-2"]);
}

// ===== Additional suffix and revision combinations =====

#[test]
fn vercmp_suffix_with_numbers() {
    // Test suffixes with explicit numeric values
    assert_eq!(vercmp("1.0_alpha1", "1.0_alpha2"), Ordering::Less);
    assert_eq!(vercmp("1.0_beta1", "1.0_beta2"), Ordering::Less);
    assert_eq!(vercmp("1.0_rc1", "1.0_rc2"), Ordering::Less);
    assert_eq!(vercmp("1.0_p1", "1.0_p2"), Ordering::Less);
}

#[test]
fn vercmp_implicit_vs_explicit_zero_suffix() {
    // Test implicit zero suffix vs explicit zero
    assert_eq!(vercmp("1.0_alpha", "1.0_alpha0"), Ordering::Equal);
    assert_eq!(vercmp("1.0_beta", "1.0_beta0"), Ordering::Equal);
    assert_eq!(vercmp("1.0_rc", "1.0_rc0"), Ordering::Equal);
    assert_eq!(vercmp("1.0_p", "1.0_p0"), Ordering::Equal);
}

#[test]
fn vercmp_single_letter_base() {
    // Test single letter suffixes in base version.
    // Note: Upstream Python vercmp rejects "1.0ab" (multiple letters after digits)
    // as invalid. The Rust implementation is more lenient.
    assert_eq!(vercmp("1.0a", "1.0"), Ordering::Greater);
}
