//! Deep depgraph coverage: OR-choice branches (committed/overlap/first/
//! fallback), group-nested ||, and matching helper arms.

use diverge::dbapi::{PackageDb, PackageMetadata};
use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

fn pkg(deps: &[(&str, &str)]) -> PackageMetadata {
    let mut m = PackageMetadata {
        slot: Some("0".into()),
        sub_slot: None,
        repo: Some("r".into()),
        eapi: Some("7".into()),
        iuse: vec![],
        use_enabled: vec![],
        keywords: vec!["x86".into()],
        deps: Default::default(),
    };
    for (k, v) in deps {
        m.deps.insert((*k).to_string(), (*v).to_string());
    }
    m
}

fn db(entries: &[(&str, PackageMetadata)]) -> PackageDb {
    let mut d = PackageDb::new();
    for (cpv, m) in entries {
        d.insert(*cpv, m.clone());
    }
    d
}

#[test]
fn or_choice_group_branch_pulls_all_atoms() {
    // A grouped branch `( X Y )` inside || -> both X and Y selected.
    let available = db(&[
        ("p/main-1", pkg(&[("RDEPEND", "|| ( ( p/x p/y ) p/z )")])),
        ("p/x-1", pkg(&[])),
        ("p/y-1", pkg(&[])),
        ("p/z-1", pkg(&[])),
    ]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // First branch (the group X Y) is taken since both are available.
    assert!(outcome.mergelist.contains(&"p/x-1".to_string()));
    assert!(outcome.mergelist.contains(&"p/y-1".to_string()));
    assert!(!outcome.mergelist.contains(&"p/z-1".to_string()));
}

#[test]
fn or_choice_falls_back_to_first_when_none_available() {
    // No branch is fully available -> fall back to the first branch, which
    // then surfaces an unsatisfied error for its missing atom.
    let available = db(&[(
        "p/main-1",
        pkg(&[("RDEPEND", "|| ( p/missingA p/missingB )")]),
    )]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::Unsatisfied(_))
    ));
}

#[test]
fn or_choice_second_branch_when_first_unavailable() {
    // First branch's atom is missing; second branch is available -> chosen.
    let available = db(&[
        ("p/main-1", pkg(&[("RDEPEND", "|| ( p/absent p/present )")])),
        ("p/present-1", pkg(&[])),
    ]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert!(outcome.mergelist.contains(&"p/present-1".to_string()));
}

#[test]
fn or_choice_prefers_already_committed_provider() {
    // Two || groups share provider B; the second reuses the first's commit.
    let available = db(&[
        (
            "p/main-1",
            pkg(&[("RDEPEND", "|| ( p/a p/b ) || ( p/b p/c )")]),
        ),
        ("p/a-1", pkg(&[])),
        ("p/b-1", pkg(&[])),
        ("p/c-1", pkg(&[])),
    ]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // B is shared across both choices -> only B pulled in (minimize children).
    assert!(outcome.mergelist.contains(&"p/b-1".to_string()));
    assert!(!outcome.mergelist.contains(&"p/a-1".to_string()));
    assert!(!outcome.mergelist.contains(&"p/c-1".to_string()));
}

#[test]
fn pdepend_and_bdepend_followed() {
    // PDEPEND and BDEPEND are in the default dep_keys -> followed.
    let available = db(&[
        (
            "p/main-1",
            pkg(&[("PDEPEND", "p/post"), ("BDEPEND", "p/build")]),
        ),
        ("p/post-1", pkg(&[])),
        ("p/build-1", pkg(&[])),
    ]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert!(outcome.mergelist.contains(&"p/post-1".to_string()));
    assert!(outcome.mergelist.contains(&"p/build-1".to_string()));
}

#[test]
fn already_selected_dependency_not_duplicated() {
    // Diamond: main -> a, main -> b, a -> shared, b -> shared.
    let available = db(&[
        ("p/main-1", pkg(&[("RDEPEND", "p/a p/b")])),
        ("p/a-1", pkg(&[("RDEPEND", "p/shared")])),
        ("p/b-1", pkg(&[("RDEPEND", "p/shared")])),
        ("p/shared-1", pkg(&[])),
    ]);
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // shared appears exactly once.
    let n = outcome
        .mergelist
        .iter()
        .filter(|m| *m == "p/shared-1")
        .count();
    assert_eq!(n, 1);
}
