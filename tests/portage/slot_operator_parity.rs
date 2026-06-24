//! Resolver tests for slot-operator (`:=`) rebuild semantics.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_slot_operator_rebuild.py`
//! - `research/portage/lib/portage/tests/resolver/test_slot_abi.py`

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg_slot};

#[test]
fn subslot_change_triggers_dependent_rebuild() {
    // app-misc/A upgrades from SLOT 0/1 to 0/2. Installed app-misc/B was built
    // against A:0/1= and must be rebuilt against the new sub-slot.
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
    // Installed B records the sub-slot it was built against.
    installed.insert(
        "app-misc/B-0",
        pkg_slot("0", &[("RDEPEND", "app-misc/A:0/1=")]),
    );

    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);

    // A-2 is merged, and B-0 is pulled in for a rebuild against the new sub-slot.
    assert!(
        outcome.mergelist.contains(&"app-misc/A-2".to_string()),
        "A-2 merged: {:?}",
        outcome.mergelist
    );
    assert!(
        outcome.mergelist.contains(&"app-misc/B-0".to_string()),
        "B-0 rebuilt: {:?}",
        outcome.mergelist
    );
    // The rebuild merges after the new dependency.
    let pos = |cpv: &str| outcome.mergelist.iter().position(|x| x == cpv).unwrap();
    assert!(pos("app-misc/A-2") < pos("app-misc/B-0"));
}

#[test]
fn unchanged_subslot_does_not_trigger_rebuild() {
    // A stays at SLOT 0/1: no rebuild of B is required.
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
    // Re-request A (already installed at the same sub-slot).
    let outcome = resolver.resolve(&["app-misc/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // A-1 is the only candidate (reinstall target); B must NOT be rebuilt.
    assert!(
        !outcome.mergelist.contains(&"app-misc/B-0".to_string()),
        "B should not rebuild when sub-slot is unchanged: {:?}",
        outcome.mergelist
    );
}
