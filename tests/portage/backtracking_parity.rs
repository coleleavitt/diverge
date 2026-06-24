//! Resolver tests for backtracking over slot/version conflicts.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_backtracking.py`

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg};

#[test]
fn backtracks_to_satisfy_exact_version_constraint() {
    // C deps "dev-libs/A dev-libs/B" (would greedily pick A-2/B-2).
    // D deps "=dev-libs/A-1 =dev-libs/B-1". Resolving [C, D] must pin
    // A->A-1 and B->B-1 so both C and D are satisfied (bug: backtracking).
    let available = db(&[
        ("dev-libs/A-1", pkg(&[])),
        ("dev-libs/A-2", pkg(&[])),
        ("dev-libs/B-1", pkg(&[])),
        ("dev-libs/B-2", pkg(&[])),
        ("dev-libs/C-1", pkg(&[("DEPEND", "dev-libs/A dev-libs/B")])),
        (
            "dev-libs/D-1",
            pkg(&[("DEPEND", "=dev-libs/A-1 =dev-libs/B-1")]),
        ),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/C", "dev-libs/D"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // A-1 and B-1 are chosen (not A-2/B-2) so D's exact deps hold.
    assert!(outcome.mergelist.contains(&"dev-libs/A-1".to_string()));
    assert!(outcome.mergelist.contains(&"dev-libs/B-1".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/A-2".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/B-2".to_string()));
    assert!(outcome.mergelist.contains(&"dev-libs/C-1".to_string()));
    assert!(outcome.mergelist.contains(&"dev-libs/D-1".to_string()));
}

#[test]
fn backtracks_to_satisfy_lower_bound_constraint() {
    // A deps dev-libs/Z (any), B deps >=dev-libs/Z-2. Resolving [A, B] must
    // pick Z-2 to satisfy both.
    let available = db(&[
        ("dev-libs/A-1", pkg(&[("DEPEND", "dev-libs/Z")])),
        ("dev-libs/B-1", pkg(&[("DEPEND", ">=dev-libs/Z-2")])),
        ("dev-libs/Z-1", pkg(&[])),
        ("dev-libs/Z-2", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/A", "dev-libs/B"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // Z-2 satisfies both the unconstrained and the >=2 atom.
    assert!(outcome.mergelist.contains(&"dev-libs/Z-2".to_string()));
    assert!(!outcome.mergelist.contains(&"dev-libs/Z-1".to_string()));
}

#[test]
fn unsatisfiable_conflict_fails_cleanly() {
    // A needs =Z-1, B needs =Z-2: no single Z version satisfies both.
    let available = db(&[
        ("dev-libs/A-1", pkg(&[("DEPEND", "=dev-libs/Z-1")])),
        ("dev-libs/B-1", pkg(&[("DEPEND", "=dev-libs/Z-2")])),
        ("dev-libs/Z-1", pkg(&[])),
        ("dev-libs/Z-2", pkg(&[])),
    ]);
    let installed = PackageDb::new();
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/A", "dev-libs/B"]);
    assert!(
        !outcome.is_success(),
        "expected conflict: {:?}",
        outcome.mergelist
    );
}
