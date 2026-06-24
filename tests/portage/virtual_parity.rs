//! Resolver tests for virtual packages and overlapping `|| ( ... )` choices.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_virtual_minimize_children.py`
//!   (bug 632026: minimize providers across overlapping any-of deps)
//! - `research/portage/lib/portage/tests/resolver/test_virtual_slot.py`

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg};

#[test]
fn virtual_resolves_to_a_provider() {
    // app-misc/bar -> virtual/foo -> || ( app-misc/A app-misc/B ).
    let available = db(&[
        ("app-misc/bar-1", pkg(&[("RDEPEND", "virtual/foo")])),
        (
            "virtual/foo-1",
            pkg(&[("RDEPEND", "|| ( app-misc/A app-misc/B )")]),
        ),
        ("app-misc/A-1", pkg(&[])),
        ("app-misc/B-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/bar"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // The virtual and its provider (first branch, A) and bar are all merged.
    assert!(outcome.mergelist.contains(&"virtual/foo-1".to_string()));
    assert!(outcome.mergelist.contains(&"app-misc/A-1".to_string()));
    assert!(outcome.mergelist.contains(&"app-misc/bar-1".to_string()));
    // bar is merged after the virtual it depends on.
    let pos = |cpv: &str| outcome.mergelist.iter().position(|x| x == cpv).unwrap();
    assert!(pos("virtual/foo-1") < pos("app-misc/bar-1"));
}

#[test]
fn overlapping_any_of_choices_minimize_children() {
    // bug 632026: virtual/foo RDEPEND="|| ( A B ) || ( B C )" should pick B
    // for BOTH choices (one provider) rather than A and B (two providers).
    let available = db(&[
        ("app-misc/bar-1", pkg(&[("RDEPEND", "virtual/foo")])),
        (
            "virtual/foo-1",
            pkg(&[(
                "RDEPEND",
                "|| ( app-misc/A app-misc/B ) || ( app-misc/B app-misc/C )",
            )]),
        ),
        ("app-misc/A-1", pkg(&[])),
        ("app-misc/B-1", pkg(&[])),
        ("app-misc/C-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/bar"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // Exactly one of A/B/C is pulled in, and it is B (shared by both choices).
    assert!(
        outcome.mergelist.contains(&"app-misc/B-1".to_string()),
        "B should be the shared provider: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome.mergelist.contains(&"app-misc/A-1".to_string()),
        "A should not be pulled in: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome.mergelist.contains(&"app-misc/C-1".to_string()),
        "C should not be pulled in: {:?}",
        outcome.mergelist
    );
    assert_eq!(
        outcome.mergelist,
        vec!["app-misc/B-1", "virtual/foo-1", "app-misc/bar-1"]
    );
}

#[test]
fn installed_virtual_provider_is_preferred() {
    // virtual/jdk -> || ( icedtea sun-jdk ); icedtea installed -> prefer it.
    let available = db(&[
        ("app-misc/java-app-1", pkg(&[("RDEPEND", "virtual/jdk")])),
        (
            "virtual/jdk-1.6.0",
            pkg(&[("RDEPEND", "|| ( dev-java/icedtea dev-java/sun-jdk )")]),
        ),
        ("dev-java/icedtea-6", pkg(&[])),
        ("dev-java/sun-jdk-1.6.0", pkg(&[])),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("dev-java/icedtea-6", pkg(&[]));
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/java-app"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // icedtea is installed -> not remerged; sun-jdk is not pulled in.
    assert!(
        !outcome
            .mergelist
            .contains(&"dev-java/sun-jdk-1.6.0".to_string())
    );
    assert!(
        !outcome
            .mergelist
            .contains(&"dev-java/icedtea-6".to_string())
    );
    assert!(outcome.mergelist.contains(&"virtual/jdk-1.6.0".to_string()));
}
