//! Resolver tests for depclean reverse-dep computation and --update/--newuse.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/resolver/test_depclean.py`
//! - `research/portage/lib/portage/tests/resolver/test_depclean_order.py`
//! - `research/portage/lib/portage/tests/resolver/test_update.py`

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};

use crate::resolver_fixture::{db, pkg};

#[test]
fn depclean_removes_unreferenced_package() {
    // world = {A}. A and B installed; B is not required by A -> B is cleaned.
    let available = PackageDb::new();
    let mut installed = PackageDb::new();
    installed.insert("dev-libs/A-1", pkg(&[]));
    installed.insert("dev-libs/B-1", pkg(&[]));
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let cleanlist = resolver.depclean(&["dev-libs/A"]);
    assert_eq!(cleanlist, vec!["dev-libs/B-1"]);
}

#[test]
fn depclean_keeps_transitive_dependencies() {
    // world = {A}. A->C; B->D->E->F. Only A's closure (A, C) is kept;
    // B and its chain (B, D, E, F) are cleaned.
    let available = PackageDb::new();
    let mut installed = PackageDb::new();
    installed.insert("dev-libs/A-1", pkg(&[("RDEPEND", "dev-libs/C")]));
    installed.insert("dev-libs/B-1", pkg(&[("RDEPEND", "dev-libs/D")]));
    installed.insert("dev-libs/C-1", pkg(&[]));
    installed.insert("dev-libs/D-1", pkg(&[("RDEPEND", "dev-libs/E")]));
    installed.insert("dev-libs/E-1", pkg(&[("RDEPEND", "dev-libs/F")]));
    installed.insert("dev-libs/F-1", pkg(&[]));
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let cleanlist = resolver.depclean(&["dev-libs/A"]);
    assert_eq!(
        cleanlist,
        vec![
            "dev-libs/B-1",
            "dev-libs/D-1",
            "dev-libs/E-1",
            "dev-libs/F-1"
        ]
    );
}

#[test]
fn depclean_keeps_installed_or_choice_provider() {
    // A->|| ( B C ); B installed and provides the choice -> B kept, not cleaned.
    let available = PackageDb::new();
    let mut installed = PackageDb::new();
    installed.insert(
        "dev-libs/A-1",
        pkg(&[("RDEPEND", "|| ( dev-libs/B dev-libs/C )")]),
    );
    installed.insert("dev-libs/B-1", pkg(&[]));
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let cleanlist = resolver.depclean(&["dev-libs/A"]);
    assert!(cleanlist.is_empty(), "B is a live provider: {cleanlist:?}");
}

#[test]
fn update_pulls_higher_version_of_installed_dep() {
    // A->dev-libs/B; B-1 installed, B-2 available. --update reinstalls B-2.
    let available = db(&[
        ("dev-libs/A-1", pkg(&[("RDEPEND", "dev-libs/B")])),
        ("dev-libs/B-1", pkg(&[])),
        ("dev-libs/B-2", pkg(&[])),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("dev-libs/B-1", pkg(&[]));

    // Without --update: B already satisfied, not upgraded.
    let resolver = Resolver::new(&available, &installed, ResolveParams::default());
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(!outcome.mergelist.contains(&"dev-libs/B-2".to_string()));

    // With --update --deep: B-2 is pulled in.
    let params = ResolveParams::default().with_update(true).with_deep(true);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(
        outcome.mergelist.contains(&"dev-libs/B-2".to_string()),
        "B-2 should be upgraded: {:?}",
        outcome.mergelist
    );
}

#[test]
fn newuse_reinstalls_on_use_change() {
    // B installed with USE={}, available B with USE={foo}. --newuse reinstalls.
    let mut b_avail = pkg(&[]);
    b_avail.use_enabled = vec!["foo".to_string()];
    b_avail.iuse = vec!["foo".to_string()];
    let available = db(&[
        ("dev-libs/A-1", pkg(&[("RDEPEND", "dev-libs/B")])),
        ("dev-libs/B-1", b_avail),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("dev-libs/B-1", pkg(&[])); // USE empty

    let params = ResolveParams::default().with_newuse(true).with_deep(true);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["dev-libs/A"]);
    assert!(
        outcome.mergelist.contains(&"dev-libs/B-1".to_string()),
        "B-1 should be reinstalled for USE change: {:?}",
        outcome.mergelist
    );
}
