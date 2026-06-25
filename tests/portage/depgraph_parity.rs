//! Integration tests for the real dependency-graph resolver.
//!
//! These build an in-memory available/installed package set (as `fakedbapi`
//! does in upstream's ResolverPlayground) and assert merge-list outcomes.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_simple.py`
//! - `research/portage/lib/portage/tests/resolver/test_or_choices.py`
//! - `research/portage/lib/portage/tests/resolver/test_blocker.py`
//! - `research/portage/lib/portage/tests/resolver/test_merge_order.py`

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg};

#[test]
fn resolves_simple_dependency_chain() {
    // A depends on B depends on C: merge order must be C, B, A.
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "app-a/B")])),
        ("app-a/B-1", pkg(&[("RDEPEND", "app-a/C")])),
        ("app-a/C-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(
        outcome.mergelist,
        vec!["app-a/C-1", "app-a/B-1", "app-a/A-1"]
    );
}

#[test]
fn picks_highest_visible_version() {
    let available = db(&[("dev-libs/A-1", pkg(&[])), ("dev-libs/A-2", pkg(&[]))]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert_eq!(outcome.mergelist, vec!["dev-libs/A-2"]);
}

#[test]
fn unstable_keyword_is_filtered_unless_accepted() {
    let mut unstable = pkg(&[]);
    unstable.keywords = vec!["~x86".to_string()];
    let available = db(&[("dev-libs/A-2", unstable)]);
    let installed = PackageDb::new();

    // Default params (x86 only): the ~x86 package is invisible -> unsatisfied.
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::Unsatisfied(_))
    ));

    // Accepting ~x86 makes it visible.
    let params = ResolveParams::default().accept_keyword("~x86");
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert_eq!(outcome.mergelist, vec!["dev-libs/A-2"]);
}

#[test]
fn installed_dependency_is_not_remerged() {
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "app-a/B")])),
        ("app-a/B-1", pkg(&[])),
    ]);
    // B already installed: only A should be in the merge list.
    let mut installed = PackageDb::new();
    installed.insert("app-a/B-1", pkg(&[]));
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(outcome.mergelist, vec!["app-a/A-1"]);
}

#[test]
fn or_choice_prefers_installed_branch() {
    // systemd-ui needs || ( vala:0.20 vala:0.18 ); vala-0.18 is installed.
    let mut vala_20 = pkg(&[]);
    vala_20.slot = Some("0.20".to_string());
    let mut vala_18 = pkg(&[]);
    vala_18.slot = Some("0.18".to_string());

    let available = db(&[
        ("dev-lang/vala-0.20.0", vala_20),
        ("dev-lang/vala-0.18.0", vala_18.clone()),
        (
            "sys-apps/systemd-ui-2",
            pkg(&[("RDEPEND", "|| ( dev-lang/vala:0.20 dev-lang/vala:0.18 )")]),
        ),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("dev-lang/vala-0.18.0", vala_18);
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["sys-apps/systemd-ui"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // vala-0.18 satisfied by installed -> only systemd-ui merges.
    assert_eq!(outcome.mergelist, vec!["sys-apps/systemd-ui-2"]);
}

#[test]
fn or_choice_falls_back_to_first_available() {
    let available = db(&[
        ("app-a/X-1", pkg(&[])),
        ("app-a/Y-1", pkg(&[])),
        ("app-a/Z-1", pkg(&[("RDEPEND", "|| ( app-a/X app-a/Y )")])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/Z"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // First branch (X) is chosen; X before Z.
    assert_eq!(outcome.mergelist, vec!["app-a/X-1", "app-a/Z-1"]);
}

#[test]
fn use_conditional_dependency_is_evaluated() {
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "foo? ( app-a/B )")])),
        ("app-a/B-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();

    // foo disabled: B not pulled in.
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/A"]);
    assert_eq!(outcome.mergelist, vec!["app-a/A-1"]);

    // foo enabled: B pulled in before A.
    let params = ResolveParams::default().with_use(["foo"]);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-a/A"]);
    assert_eq!(outcome.mergelist, vec!["app-a/B-1", "app-a/A-1"]);
}

#[test]
fn strong_blocker_conflicts_are_reported() {
    // A blocks B (!!app-a/B) but B is pulled in as another dep -> conflict.
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "!!app-a/B app-a/C")])),
        ("app-a/B-1", pkg(&[])),
        ("app-a/C-1", pkg(&[("RDEPEND", "app-a/B")])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(
        matches!(outcome.error, Some(ResolveFailure::Blocked { .. })),
        "expected blocker conflict, got {:?}",
        outcome
    );
}

#[test]
fn unsatisfiable_dependency_errors() {
    let available = db(&[("app-a/A-1", pkg(&[("RDEPEND", "app-a/missing")]))]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/A"]);
    match outcome.error {
        Some(ResolveFailure::Unsatisfied(atom)) => assert!(atom.contains("app-a/missing")),
        other => panic!("expected unsatisfied, got {other:?}"),
    }
}

#[test]
fn circular_build_dependency_is_detected() {
    // A (DEPEND B) and B (DEPEND A): a build-time cycle.
    let available = db(&[
        ("app-a/A-1", pkg(&[("DEPEND", "app-a/B")])),
        ("app-a/B-1", pkg(&[("DEPEND", "app-a/A")])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(
        matches!(outcome.error, Some(ResolveFailure::CircularDependency(_))),
        "expected circular dependency, got {:?}",
        outcome
    );
}

#[test]
fn required_use_violation_is_rejected() {
    // Ported from research/portage/lib/portage/tests/resolver/test_required_use.py:
    // a package with REQUIRED_USE="^^ ( foo bar )" (exactly one of) must have
    // exactly one of foo/bar enabled. With neither enabled the resolve fails.
    let mut meta = pkg(&[]);
    meta.iuse = vec!["foo".to_string(), "bar".to_string()];
    meta.deps
        .insert("REQUIRED_USE".to_string(), "^^ ( foo bar )".to_string());
    let available = db(&[("app-misc/A-1", meta.clone())]);
    let installed = PackageDb::new();

    // No USE flags enabled -> ^^ ( foo bar ) is violated.
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(
        matches!(
            outcome.error,
            Some(ResolveFailure::RequiredUseUnsatisfied { .. })
        ),
        "expected REQUIRED_USE failure, got {:?}",
        outcome
    );

    // Enabling exactly one (foo) satisfies ^^ ( foo bar ).
    let params = ResolveParams::default().with_use(["foo"]);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(outcome.mergelist, vec!["app-misc/A-1"]);

    // Enabling both violates the exactly-one constraint.
    let params = ResolveParams::default().with_use(["foo", "bar"]);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(
        matches!(
            outcome.error,
            Some(ResolveFailure::RequiredUseUnsatisfied { .. })
        ),
        "expected REQUIRED_USE failure with both enabled, got {:?}",
        outcome
    );
}

#[test]
fn package_mask_makes_version_unselectable() {
    // Ported from research/portage/lib/portage/tests/resolver (package.mask):
    // a masked version is invisible; selection falls back to the unmasked one.
    let available = db(&[("dev-libs/A-1", pkg(&[])), ("dev-libs/A-2", pkg(&[]))]);
    let installed = PackageDb::new();

    // Mask A-2 -> resolve picks A-1.
    let params = ResolveParams::default().with_masks([">=dev-libs/A-2"]);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(outcome.mergelist, vec!["dev-libs/A-1"]);

    // Masking the whole cp -> unsatisfied.
    let params = ResolveParams::default().with_masks(["dev-libs/A"]);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::Unsatisfied(_))
    ));
}
