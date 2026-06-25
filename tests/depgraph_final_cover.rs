//! Final depgraph coverage: slot-op binding edge cases, newuse, empty
//! REQUIRED_USE skip, dep DNF group-disjunction paths.

use diverge::dbapi::{PackageDb, PackageMetadata};
use diverge::depgraph::{ResolveParams, Resolver};

fn meta(slot: &str, deps: &[(&str, &str)], iuse: &[&str], use_on: &[&str]) -> PackageMetadata {
    let (s, sub) = match slot.split_once('/') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (slot.to_string(), None),
    };
    let mut m = PackageMetadata {
        slot: Some(s),
        sub_slot: sub,
        repo: Some("r".into()),
        eapi: Some("7".into()),
        iuse: iuse.iter().map(|x| x.to_string()).collect(),
        use_enabled: use_on.iter().map(|x| x.to_string()).collect(),
        keywords: vec!["x86".into()],
        deps: Default::default(),
    };
    for (k, v) in deps {
        m.deps.insert((*k).to_string(), (*v).to_string());
    }
    m
}

#[test]
fn slot_op_binding_bare_and_mismatched_no_rebuild() {
    // Installed B has a bare `:=` (no recorded sub-slot) -> no rebuild trigger.
    let available = PackageDb::new();
    let mut av = PackageDb::new();
    av.insert("a/lib-1", meta("0/1", &[], &[], &[]));
    av.insert("a/lib-2", meta("0/2", &[], &[], &[]));
    av.insert("a/cons-1", meta("0", &[("RDEPEND", "a/lib:=")], &[], &[]));
    let mut installed = PackageDb::new();
    installed.insert("a/lib-1", meta("0/1", &[], &[], &[]));
    // Bare `a/lib:=` with no bound sub-slot in the installed record.
    installed.insert("a/cons-1", meta("0", &[("RDEPEND", "a/lib:=")], &[], &[]));
    let _ = available;
    let outcome = Resolver::new(&av, &installed, ResolveParams::default()).resolve(&["a/lib"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // No recorded sub-slot binding -> the consumer is not force-rebuilt.
    assert!(!outcome.mergelist.contains(&"a/cons-1".to_string()));
}

#[test]
fn slot_op_binding_unrelated_cp_ignored() {
    // Installed consumer binds a DIFFERENT cp's slot-operator; upgrading a/lib
    // must not rebuild it.
    let mut av = PackageDb::new();
    av.insert("a/lib-1", meta("0/1", &[], &[], &[]));
    av.insert("a/lib-2", meta("0/2", &[], &[], &[]));
    av.insert("a/cons-1", meta("0", &[("RDEPEND", "a/lib")], &[], &[]));
    let mut installed = PackageDb::new();
    installed.insert("a/lib-1", meta("0/1", &[], &[], &[]));
    installed.insert(
        "a/cons-1",
        meta("0", &[("RDEPEND", "other/thing:0/9=")], &[], &[]),
    );
    let outcome = Resolver::new(&av, &installed, ResolveParams::default()).resolve(&["a/lib"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    assert!(!outcome.mergelist.contains(&"a/cons-1".to_string()));
}

#[test]
fn newuse_reinstall_on_flag_change() {
    // Installed B has USE={}, available B has USE={foo} -> --newuse reinstalls.
    let mut av = PackageDb::new();
    av.insert("a/main-1", meta("0", &[("RDEPEND", "a/b")], &[], &[]));
    av.insert("a/b-1", meta("0", &[], &["foo"], &["foo"]));
    let mut installed = PackageDb::new();
    installed.insert("a/b-1", meta("0", &[], &["foo"], &[])); // foo declared, off
    let params = ResolveParams::default().with_newuse(true).with_deep(true);
    let outcome = Resolver::new(&av, &installed, params).resolve(&["a/main"]);
    assert!(
        outcome.mergelist.contains(&"a/b-1".to_string()),
        "newuse should reinstall b: {:?}",
        outcome.mergelist
    );
}

#[test]
fn empty_required_use_is_skipped() {
    // A REQUIRED_USE that is whitespace-only is skipped (line 963-964).
    let mut av = PackageDb::new();
    let mut m = meta("0", &[], &[], &[]);
    m.deps.insert("REQUIRED_USE".into(), "   ".into());
    av.insert("a/A-1", m);
    let outcome = Resolver::new(&av, &PackageDb::new(), ResolveParams::default()).resolve(&["a/A"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
}

#[test]
fn use_conditional_in_or_choice() {
    // foo? guarding inside an || group.
    let mut av = PackageDb::new();
    av.insert(
        "a/main-1",
        meta(
            "0",
            &[("RDEPEND", "|| ( foo? ( a/x ) a/y )")],
            &["foo"],
            &["foo"],
        ),
    );
    av.insert("a/x-1", meta("0", &[], &[], &[]));
    av.insert("a/y-1", meta("0", &[], &[], &[]));
    let params = ResolveParams::default().with_use(["foo"]);
    let outcome = Resolver::new(&av, &PackageDb::new(), params).resolve(&["a/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
}
