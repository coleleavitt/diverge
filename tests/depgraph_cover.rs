//! Coverage for resolver depgraph branches: failure Display, masks,
//! REQUIRED_USE, depclean, backtracking, slot-operator, blockers.

use diverge::dbapi::{PackageDb, PackageMetadata};
use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

fn pkg(deps: &[(&str, &str)]) -> PackageMetadata {
    let mut m = PackageMetadata {
        slot: Some("0".to_string()),
        sub_slot: None,
        repo: Some("test".to_string()),
        eapi: Some("7".to_string()),
        iuse: Vec::new(),
        use_enabled: Vec::new(),
        keywords: vec!["x86".to_string()],
        deps: Default::default(),
    };
    for (k, v) in deps {
        m.deps.insert((*k).to_string(), (*v).to_string());
    }
    m
}

fn pkg_slot(slot: &str, deps: &[(&str, &str)]) -> PackageMetadata {
    let mut m = pkg(deps);
    match slot.split_once('/') {
        Some((s, sub)) => {
            m.slot = Some(s.to_string());
            m.sub_slot = Some(sub.to_string());
        }
        None => m.slot = Some(slot.to_string()),
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
fn failure_display_for_every_variant() {
    assert!(format!("{}", ResolveFailure::Unsatisfied("a/b".into())).contains("no visible"));
    assert!(
        format!(
            "{}",
            ResolveFailure::Blocked {
                blocker: "!a/b".into(),
                blocked: "a/c".into()
            }
        )
        .contains("blocker")
    );
    assert!(
        format!(
            "{}",
            ResolveFailure::CircularDependency(vec!["a".into(), "b".into()])
        )
        .contains("circular")
    );
    assert!(format!("{}", ResolveFailure::InvalidDependency("x".into())).contains("invalid"));
    assert!(format!("{}", ResolveFailure::AutounmaskRequired).contains("autounmask"));
    assert!(
        format!(
            "{}",
            ResolveFailure::RequiredUseUnsatisfied {
                cpv: "a/b-1".into(),
                required_use: "^^ ( x y )".into()
            }
        )
        .contains("REQUIRED_USE")
    );
}

#[test]
fn mask_fallback_and_full_mask() {
    let available = db(&[("d/A-1", pkg(&[])), ("d/A-2", pkg(&[]))]);
    let installed = PackageDb::new();
    let p = ResolveParams::default().with_masks([">=d/A-2"]);
    let o = Resolver::new(&available, &installed, p).resolve(&["d/A"]);
    assert_eq!(o.mergelist, vec!["d/A-1"]);
    let p = ResolveParams::default().with_masks(["d/A"]);
    let o = Resolver::new(&available, &installed, p).resolve(&["d/A"]);
    assert!(matches!(o.error, Some(ResolveFailure::Unsatisfied(_))));
}

#[test]
fn required_use_enforced() {
    let mut m = pkg(&[]);
    m.iuse = vec!["a".into(), "b".into()];
    m.deps.insert("REQUIRED_USE".into(), "|| ( a b )".into());
    let available = db(&[("d/A-1", m)]);
    let installed = PackageDb::new();
    // neither enabled -> || fails
    let o = Resolver::new(&available, &installed, ResolveParams::default()).resolve(&["d/A"]);
    assert!(matches!(
        o.error,
        Some(ResolveFailure::RequiredUseUnsatisfied { .. })
    ));
    // a enabled -> ok
    let p = ResolveParams::default().with_use(["a"]);
    let o = Resolver::new(&available, &installed, p).resolve(&["d/A"]);
    assert!(o.is_success());
}

#[test]
fn depclean_transitive_and_or_choice() {
    let available = PackageDb::new();
    let mut installed = PackageDb::new();
    installed.insert("d/A-1", pkg(&[("RDEPEND", "d/C")]));
    installed.insert("d/B-1", pkg(&[("RDEPEND", "|| ( d/X d/C )")]));
    installed.insert("d/C-1", pkg(&[]));
    installed.insert("d/orphan-1", pkg(&[]));
    let r = Resolver::new(&available, &installed, ResolveParams::default());
    let clean = r.depclean(&["d/A", "d/B"]);
    assert!(clean.contains(&"d/orphan-1".to_string()));
    assert!(!clean.contains(&"d/C-1".to_string())); // kept via A and B's choice
}

#[test]
fn backtracking_exact_and_unsat() {
    let available = db(&[
        ("d/A-1", pkg(&[])),
        ("d/A-2", pkg(&[])),
        ("d/C-1", pkg(&[("DEPEND", "d/A")])),
        ("d/D-1", pkg(&[("DEPEND", "=d/A-1")])),
    ]);
    let installed = PackageDb::new();
    let o =
        Resolver::new(&available, &installed, ResolveParams::default()).resolve(&["d/C", "d/D"]);
    assert!(o.is_success());
    assert!(o.mergelist.contains(&"d/A-1".to_string()));
    assert!(!o.mergelist.contains(&"d/A-2".to_string()));

    let avail2 = db(&[
        ("d/E-1", pkg(&[("DEPEND", "=d/Z-1")])),
        ("d/F-1", pkg(&[("DEPEND", "=d/Z-2")])),
        ("d/Z-1", pkg(&[])),
        ("d/Z-2", pkg(&[])),
    ]);
    let o = Resolver::new(&avail2, &installed, ResolveParams::default()).resolve(&["d/E", "d/F"]);
    assert!(!o.is_success());
}

#[test]
fn slot_operator_rebuild_and_weak_blocker() {
    let available = db(&[
        ("d/A-1", pkg_slot("0/1", &[])),
        ("d/A-2", pkg_slot("0/2", &[])),
        ("d/B-0", pkg_slot("0", &[("RDEPEND", "d/A:=")])),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("d/A-1", pkg_slot("0/1", &[]));
    installed.insert("d/B-0", pkg_slot("0", &[("RDEPEND", "d/A:0/1=")]));
    let o = Resolver::new(&available, &installed, ResolveParams::default()).resolve(&["d/A"]);
    assert!(o.is_success());
    assert!(o.mergelist.contains(&"d/B-0".to_string()));

    // Weak blocker (!): a non-installed conflicting pkg is fine if not selected.
    let available = db(&[("d/X-1", pkg(&[("RDEPEND", "!d/Y")]))]);
    let o =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["d/X"]);
    assert!(o.is_success(), "{:?}", o.error);
}

#[test]
fn invalid_target_atom_errors() {
    let available = db(&[("d/A-1", pkg(&[]))]);
    let o = Resolver::new(&available, &PackageDb::new(), ResolveParams::default())
        .resolve(&["d/A[bad"]);
    assert!(matches!(
        o.error,
        Some(ResolveFailure::InvalidDependency(_))
    ));
}
