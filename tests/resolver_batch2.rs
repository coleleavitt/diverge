//! Integration tests for resolver behavior: blockers, circular deps, virtuals, slots.
//!
//! This file ports observable behavior from upstream Portage resolver tests:
//! - `research/portage/lib/portage/tests/resolver/test_blocker.py`
//! - `research/portage/lib/portage/tests/resolver/test_circular_dependencies.py`
//! - `research/portage/lib/portage/tests/resolver/test_virtual_minimize_children.py`
//! - `research/portage/lib/portage/tests/resolver/test_or_choices.py`
//! - `research/portage/lib/portage/tests/resolver/test_slot_operator_rebuild.py`
//! - `research/portage/lib/portage/tests/resolver/test_slot_conflict_rebuild.py`
//!
//! Tests that require advanced features (autounmask, BDEPEND-only blockers,
//! dynamic-deps, or world/selective mode) are skipped.

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

// Copy fixture helpers locally (cannot import from a separate test crate).
#[allow(dead_code)]
fn pkg(deps: &[(&str, &str)]) -> diverge::dbapi::PackageMetadata {
    let mut meta = diverge::dbapi::PackageMetadata {
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

#[allow(dead_code)]
fn pkg_slot(slot: &str, deps: &[(&str, &str)]) -> diverge::dbapi::PackageMetadata {
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

#[allow(dead_code)]
fn db(entries: &[(&str, diverge::dbapi::PackageMetadata)]) -> PackageDb {
    let mut db = PackageDb::new();
    for (cpv, meta) in entries {
        db.insert(*cpv, meta.clone());
    }
    db
}

// ============================================================================
// test_blocker.py: testBlocker
// ============================================================================
// SKIPPED: This test exercises blocker atoms (!= atoms) which require
// tracking installed blockers and ensuring they are uninstalled. The
// complexity of modeling [uninstall] markers and blocker handling is
// beyond current diverge scope. The case also uses ambiguous_merge_order
// which requires permutation testing.

// ============================================================================
// test_blocker.py: testBlockerBuildpkgonly
// ============================================================================
// SKIPPED: Tests buildpkgonly mode with BDEPEND-only blockers (!!),
// which requires EAPI 7+ specific BDEPEND handling and --buildpkgonly
// mode not yet implemented in diverge.

// ============================================================================
// test_circular_dependencies.py: testCircularDependency
// ============================================================================
// SKIPPED: Tests circular dependency detection across build-time and
// runtime dependencies with USE flag changes and REQUIRED_USE constraints.
// Diverge detects circular build-deps but doesn't model circular runtime
// deps or offer circular_dependency_solutions (USE flag suggestions).

// ============================================================================
// test_virtual_minimize_children.py: testVirtualMinimizeChildren
// ============================================================================

#[test]
fn virtual_minimizes_overlapping_choices() {
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

    // Exactly one of A/B/C is pulled in, and it should be B (shared by both choices).
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
}

#[test]
fn virtual_prefers_installed_overlap() {
    // When multiple packages satisfy overlapping || deps, and some are
    // installed, prefer the installed ones to minimize merges.
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
    let mut installed = PackageDb::new();
    // Install both A and C; the resolver should use them for the choice overlap.
    installed.insert("app-misc/A-1", pkg(&[]));
    installed.insert("app-misc/C-1", pkg(&[]));

    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/bar"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // Only the virtual and bar should be merged (A and C are installed).
    assert!(
        outcome.mergelist.contains(&"virtual/foo-1".to_string()),
        "virtual/foo-1 must be merged: {:?}",
        outcome.mergelist
    );
    assert!(
        outcome.mergelist.contains(&"app-misc/bar-1".to_string()),
        "app-misc/bar-1 must be merged: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome.mergelist.contains(&"app-misc/A-1".to_string()),
        "A is installed, should not be in mergelist: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome.mergelist.contains(&"app-misc/C-1".to_string()),
        "C is installed, should not be in mergelist: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome.mergelist.contains(&"app-misc/B-1".to_string()),
        "B should not be pulled in: {:?}",
        outcome.mergelist
    );
}

// ============================================================================
// test_virtual_minimize_children.py: testOverlapSlotConflict
// ============================================================================
// SKIPPED: Tests overlapping || deps with version constraints that create
// slot conflicts. The specific case "|| ( A >=B-2 ) || ( <B-2 C )" is not
// satisfiable for the B choice overlap, which requires advanced conflict
// detection. diverge does not yet implement slot conflict backtracking.

// ============================================================================
// test_virtual_minimize_children.py: testVirtualPackageManager
// ============================================================================
// SKIPPED: Tests permutations and ambiguous_merge_order with complex virtual
// package relationships. diverge's resolver does not support permutation testing.

// ============================================================================
// test_virtual_minimize_children.py: testVirtualDevManager
// ============================================================================
// SKIPPED: Tests nested virtual packages. While simpler than the above,
// it requires exact merge-order validation. diverge may resolve it but the
// order may differ from Portage's.

// ============================================================================
// test_virtual_minimize_children.py: testVirtualWine
// ============================================================================

#[test]
fn virtual_shared_provider_across_disjunctions() {
    // bug 701996: wine virtual package with overlapping choices.
    // RDEPEND="|| ( wine-staging wine-any ) || ( wine-vanilla wine-staging wine-any )"
    // should pick wine-staging (shared across both disjunctions).
    let available = db(&[
        (
            "virtual/wine-0-r6",
            pkg(&[(
                "RDEPEND",
                "|| ( app-emulation/wine-staging app-emulation/wine-any ) \
                 || ( app-emulation/wine-vanilla app-emulation/wine-staging app-emulation/wine-any )",
            )]),
        ),
        ("app-emulation/wine-staging-4", pkg(&[])),
        ("app-emulation/wine-any-4", pkg(&[])),
        ("app-emulation/wine-vanilla-4", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["virtual/wine"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // Only wine-staging should be pulled in (shared by both choices).
    assert!(
        outcome
            .mergelist
            .contains(&"app-emulation/wine-staging-4".to_string()),
        "wine-staging should be selected: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome
            .mergelist
            .contains(&"app-emulation/wine-any-4".to_string()),
        "wine-any should not be pulled in: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome
            .mergelist
            .contains(&"app-emulation/wine-vanilla-4".to_string()),
        "wine-vanilla should not be pulled in: {:?}",
        outcome.mergelist
    );
}

// ============================================================================
// test_or_choices.py: testOrChoices
// ============================================================================
// SKIPPED: This test uses world file, @world target, --update/--deep/--selective
// flags, and permutation testing. These features are not yet implemented in diverge.

// ============================================================================
// test_or_choices.py: testInitiallyUnsatisfied
// ============================================================================
// SKIPPED: Tests world/selective mode with initially unsatisfied dependencies.

// ============================================================================
// test_or_choices.py: testUseMask
// ============================================================================
// SKIPPED: Tests profile-based use.mask constraints and autounmask.

// ============================================================================
// test_or_choices.py: testConflictMissedUpdate
// ============================================================================
// SKIPPED: Complex test with multiple packages and USE-based conflicts.

// ============================================================================
// test_or_choices.py: test_python_slot and beyond
// ============================================================================
// SKIPPED: Advanced USE flag and slot constraint tests.

// ============================================================================
// test_slot_operator_rebuild.py: testSlotOperatorRebuild
// ============================================================================

#[test]
fn slot_operator_rebuild_basic() {
    // Test that when a dependency's subslot changes, packages with :=
    // (slot operator) dependency are marked for rebuild.
    let available = db(&[
        ("app-misc/A-1", pkg_slot("0/1", &[])),
        ("app-misc/A-2", pkg_slot("0/2", &[])),
        (
            "app-misc/B-0",
            pkg_slot("0", &[("RDEPEND", "app-misc/A:=")]),
        ),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("app-misc/A-1", pkg_slot("0/1", &[]));
    installed.insert(
        "app-misc/B-0",
        pkg_slot("0", &[("RDEPEND", "app-misc/A:0/1=")]),
    );

    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // A-2 is merged, and B-0 must be rebuilt against the new subslot.
    assert!(
        outcome.mergelist.contains(&"app-misc/A-2".to_string()),
        "A-2 should be merged: {:?}",
        outcome.mergelist
    );
    assert!(
        outcome.mergelist.contains(&"app-misc/B-0".to_string()),
        "B-0 must be rebuilt: {:?}",
        outcome.mergelist
    );
    // B's rebuild must occur after A's merge.
    let pos = |cpv: &str| outcome.mergelist.iter().position(|x| x == cpv).unwrap();
    assert!(pos("app-misc/A-2") < pos("app-misc/B-0"));
}

#[test]
fn slot_operator_unchanged_subslot_no_rebuild() {
    // When a package's subslot does NOT change, packages depending on it
    // with := should not be rebuilt.
    let available = db(&[
        ("app-misc/A-1", pkg_slot("0/1", &[])),
        (
            "app-misc/B-0",
            pkg_slot("0", &[("RDEPEND", "app-misc/A:=")]),
        ),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("app-misc/A-1", pkg_slot("0/1", &[]));
    installed.insert(
        "app-misc/B-0",
        pkg_slot("0", &[("RDEPEND", "app-misc/A:0/1=")]),
    );

    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // A-1 is the only candidate (reinstall); B must NOT be rebuilt.
    assert!(
        !outcome.mergelist.contains(&"app-misc/B-0".to_string()),
        "B should not rebuild when subslot is unchanged: {:?}",
        outcome.mergelist
    );
}

// ============================================================================
// test_slot_conflict_rebuild.py: testSlotConflictRebuild
// ============================================================================
// SKIPPED: Tests complex @world updates with slot conflicts and deep backtracking.
// Requires --update, --deep, world file, and advanced conflict resolution.

// ============================================================================
// test_slot_conflict_rebuild.py: testSlotConflictMassRebuild
// ============================================================================
// SKIPPED: Tests mass rebuild scenarios with world file and backtracking limits.

// ============================================================================
// test_slot_conflict_rebuild.py: testSlotConflictForgottenChild
// ============================================================================
// SKIPPED: Tests world file and --update/--deep flags.

// ============================================================================
// Additional simple test cases directly inspired by upstream tests
// ============================================================================

#[test]
fn virtual_resolves_to_provider() {
    // Basic virtual package resolution: app-misc/bar -> virtual/foo -> || ( A | B )
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
fn or_choice_prefers_installed() {
    // systemd-ui needs || ( vala:0.20 vala:0.18 ); vala-0.18 is installed.
    // The resolver should prefer to use the installed version.
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

    // vala-0.18 is installed and should be preferred; vala-0.20 should not be pulled in.
    assert!(
        !outcome
            .mergelist
            .contains(&"dev-lang/vala-0.20.0".to_string()),
        "vala-0.20 should not be pulled in: {:?}",
        outcome.mergelist
    );
    assert!(
        !outcome
            .mergelist
            .contains(&"dev-lang/vala-0.18.0".to_string()),
        "vala-0.18 is installed; should not be in mergelist: {:?}",
        outcome.mergelist
    );
    assert!(
        outcome
            .mergelist
            .contains(&"sys-apps/systemd-ui-2".to_string())
    );
}

#[test]
fn simple_dependency_resolution() {
    // Basic test: A -> B -> C; merge order must be C, B, A.
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
fn multiple_dependency_resolution() {
    // Request multiple independent packages: A and B both need C.
    // Merge order should be C first, then A and B (order between A and B is arbitrary).
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "app-a/C")])),
        ("app-a/B-1", pkg(&[("RDEPEND", "app-a/C")])),
        ("app-a/C-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A", "app-a/B"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // C must come before A and B.
    assert_eq!(outcome.mergelist[0], "app-a/C-1");
    assert!(outcome.mergelist.contains(&"app-a/A-1".to_string()));
    assert!(outcome.mergelist.contains(&"app-a/B-1".to_string()));
}

// SKIPPED: use_conditional_dependency
// USE flag evaluation in package metadata not yet fully supported by diverge.
// The resolver doesn't propagate use_enabled flags from resolved packages
// to dep_reduce for proper USE conditional evaluation.

#[test]
fn circular_build_dependency_detected() {
    // Circular build-time dependency: A DEPEND on B, B DEPEND on A.
    // This should be detected as an error.
    let available = db(&[
        ("app-a/A-1", pkg(&[("DEPEND", "app-a/B")])),
        ("app-a/B-1", pkg(&[("DEPEND", "app-a/A")])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(
        !outcome.is_success(),
        "Expected circular dependency error, but got: {:?}",
        outcome.mergelist
    );
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::CircularDependency(_))
    ));
}

#[test]
fn unsatisfied_dependency_error() {
    // Request a package that doesn't exist.
    let available = db(&[("app-a/A-1", pkg(&[]))]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["nonexistent/X"]);
    assert!(!outcome.is_success());
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::Unsatisfied(_))
    ));
}

#[test]
fn installed_dependency_not_remerged() {
    // A depends on B; B is already installed. Only A should be in merge list.
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "app-a/B")])),
        ("app-a/B-1", pkg(&[])),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("app-a/B-1", pkg(&[]));

    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(outcome.mergelist, vec!["app-a/A-1"]);
}

#[test]
fn slot_constraint_selection() {
    // Package depends on a specific slot: app-a/A -> dev-libs/B:0.20
    let mut b_20 = pkg(&[]);
    b_20.slot = Some("0.20".to_string());
    let mut b_18 = pkg(&[]);
    b_18.slot = Some("0.18".to_string());

    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", "dev-libs/B:0.20")])),
        ("dev-libs/B-1.0", b_20),
        ("dev-libs/B-0.18.0", b_18),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // Only B in slot 0.20 should be selected.
    assert!(outcome.mergelist.contains(&"dev-libs/B-1.0".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/B-0.18.0".to_string()));
}

#[test]
fn highest_version_selection() {
    // Multiple versions available; resolver should select the highest.
    let available = db(&[
        ("app-a/A-1", pkg(&[])),
        ("app-a/A-2", pkg(&[])),
        ("app-a/A-3", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert_eq!(outcome.mergelist, vec!["app-a/A-3"]);
}

#[test]
fn version_constraint_satisfaction() {
    // Package depends on a version range: app-a/A -> >=dev-libs/B-2
    let available = db(&[
        ("app-a/A-1", pkg(&[("RDEPEND", ">=dev-libs/B-2")])),
        ("dev-libs/B-1", pkg(&[])),
        ("dev-libs/B-2", pkg(&[])),
        ("dev-libs/B-3", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // B-3 should be selected (highest version >= 2).
    assert!(outcome.mergelist.contains(&"dev-libs/B-3".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/B-1".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/B-2".to_string()));
}

#[test]
fn or_choice_first_available() {
    // || ( A B C ): A doesn't exist, but B and C do; should pick B (first available).
    let available = db(&[
        (
            "app-a/X-1",
            pkg(&[("RDEPEND", "|| ( app-a/A app-a/B app-a/C )")]),
        ),
        ("app-a/B-1", pkg(&[])),
        ("app-a/C-1", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());

    let outcome = resolver.resolve(&["app-a/X"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert!(outcome.mergelist.contains(&"app-a/B-1".to_string()));
    assert!(!outcome.mergelist.contains(&"app-a/C-1".to_string()));
}

// SKIPPED: negated_version_constraint
// Negated atom matching (!=) is not yet properly implemented in diverge's
// matcher. Blocker-style negations require special handling that differs
// from normal version constraint satisfaction.
