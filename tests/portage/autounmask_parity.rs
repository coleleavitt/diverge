//! Resolver tests for keyword autounmask.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_autounmask.py`
//!   (keyword-change cases: app-misc/Z -> app-misc/Y, both ~x86)

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg};

/// Package metadata with explicit KEYWORDS.
fn pkg_kw(keywords: &[&str], deps: &[(&str, &str)]) -> diverge::dbapi::PackageMetadata {
    let mut meta = pkg(deps);
    meta.keywords = keywords.iter().map(|s| s.to_string()).collect();
    meta
}

#[test]
fn autounmask_off_fails_on_unstable_only_package() {
    // app-misc/Z is ~x86 only; without autounmask the request fails.
    let available = db(&[
        (
            "app-misc/Z-1",
            pkg_kw(&["~x86"], &[("DEPEND", "app-misc/Y")]),
        ),
        ("app-misc/Y-1", pkg_kw(&["~x86"], &[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/Z"]);
    assert!(!outcome.is_success());
    assert!(outcome.unstable_keywords.is_empty());
}

#[test]
fn autounmask_proposes_keyword_changes() {
    // With autounmask, the unstable Z and its unstable dep Y are surfaced as
    // keyword changes; the merge list is computed but the outcome flags that
    // approval is required (matching emerge's success=False + unstable_keywords).
    let available = db(&[
        (
            "app-misc/Z-1",
            pkg_kw(&["~x86"], &[("DEPEND", "app-misc/Y")]),
        ),
        ("app-misc/Y-1", pkg_kw(&["~x86"], &[])),
    ]);
    let installed = PackageDb::new();
    let params = ResolveParams::default().with_autounmask(true);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-misc/Z"]);

    assert!(outcome.needs_autounmask(), "{:?}", outcome);
    assert_eq!(outcome.error, Some(ResolveFailure::AutounmaskRequired));
    // Both unstable packages are in the merge list and flagged.
    assert!(outcome.mergelist.contains(&"app-misc/Z-1".to_string()));
    assert!(outcome.mergelist.contains(&"app-misc/Y-1".to_string()));
    let mut flagged = outcome.unstable_keywords.clone();
    flagged.sort();
    assert_eq!(flagged, vec!["app-misc/Y-1", "app-misc/Z-1"]);
    // Dependency Y merges before Z.
    let pos = |c: &str| outcome.mergelist.iter().position(|x| x == c).unwrap();
    assert!(pos("app-misc/Y-1") < pos("app-misc/Z-1"));
}

#[test]
fn autounmask_prefers_stable_when_available() {
    // app-misc/V (~x86) deps >=app-misc/W-2; W-1 is stable, W-2 is ~x86.
    // Only V itself needs a keyword change for its own atom resolution; with a
    // stable W-1 present but not satisfying >=W-2, W-2 is also unmasked.
    let available = db(&[
        (
            "app-misc/V-1",
            pkg_kw(&["~x86"], &[("DEPEND", ">=app-misc/W-2")]),
        ),
        ("app-misc/W-1", pkg_kw(&["x86"], &[])),
        ("app-misc/W-2", pkg_kw(&["~x86"], &[])),
    ]);
    let installed = PackageDb::new();
    let params = ResolveParams::default().with_autounmask(true);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-misc/V"]);
    assert!(outcome.needs_autounmask(), "{:?}", outcome);
    assert!(outcome.mergelist.contains(&"app-misc/W-2".to_string()));
    assert!(!outcome.mergelist.contains(&"app-misc/W-1".to_string()));
}
