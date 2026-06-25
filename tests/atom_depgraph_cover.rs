//! Targeted coverage for atom blocker/use-dep validation branches, depgraph
//! depclean-with-use, and session edge paths.

use diverge::atom::{Atom, AtomError, AtomParseOptions};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

#[test]
fn atom_invalid_blocker_triple_bang() {
    // `!!!` -> InvalidBlocker (strip "!!" then rest starts with '!').
    assert_eq!(
        Atom::parse_with_options("!!!dev-libs/A", WILD),
        Err(AtomError::InvalidBlocker)
    );
    // A lone weak blocker followed by another '!'.
    assert_eq!(
        Atom::parse_with_options("!!dev-libs/A", WILD).map(|a| a.to_string()),
        Ok("!!dev-libs/A".to_string())
    );
}

#[test]
fn atom_invalid_use_dep_forms() {
    for bad in [
        "dev-libs/A[]",       // empty
        "dev-libs/A[,foo]",   // leading comma
        "dev-libs/A[foo,]",   // trailing comma
        "dev-libs/A[foo,,b]", // empty token
        "dev-libs/A[!]",      // bang with nothing
        "dev-libs/A[!-foo]",  // bang then minus
        "dev-libs/A[-]",      // minus with nothing
        "dev-libs/A][",       // mismatched brackets
    ] {
        assert!(
            Atom::parse_with_options(bad, WILD).is_err(),
            "{bad} should be invalid"
        );
    }
}

#[test]
fn atom_valid_use_dep_forms() {
    for ok in [
        "dev-libs/A[foo]",
        "dev-libs/A[foo,bar]",
        "dev-libs/A[-foo]",
        "dev-libs/A[!foo?]",
        "dev-libs/A[foo=]",
        "dev-libs/A[foo(+)]",
        "dev-libs/A[foo(-),bar]",
    ] {
        assert!(
            Atom::parse_with_options(ok, WILD).is_ok(),
            "{ok} should be valid"
        );
    }
}

#[test]
fn atom_all_operators_parse() {
    for (s, _) in [
        (">=dev-libs/A-1", ">="),
        ("<=dev-libs/A-1", "<="),
        ("=dev-libs/A-1", "="),
        (">dev-libs/A-1", ">"),
        ("<dev-libs/A-1", "<"),
        ("~dev-libs/A-1", "~"),
    ] {
        let a = Atom::parse_with_options(s, WILD).unwrap();
        assert!(a.operator.is_some(), "{s}");
    }
}

#[test]
fn depclean_with_use_conditional_deps() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveParams, Resolver};

    fn meta(deps: &[(&str, &str)], iuse: &[&str], use_on: &[&str]) -> PackageMetadata {
        let mut m = PackageMetadata {
            slot: Some("0".into()),
            sub_slot: None,
            repo: Some("r".into()),
            eapi: Some("7".into()),
            iuse: iuse.iter().map(|s| s.to_string()).collect(),
            use_enabled: use_on.iter().map(|s| s.to_string()).collect(),
            keywords: vec!["x86".into()],
            deps: Default::default(),
        };
        for (k, v) in deps {
            m.deps.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    let available = PackageDb::new();
    let mut installed = PackageDb::new();
    // A has foo enabled -> foo? ( B ) keeps B; C only via disabled bar -> cleaned.
    installed.insert(
        "app/A-1",
        meta(
            &[("RDEPEND", "foo? ( app/B ) bar? ( app/C )")],
            &["foo", "bar"],
            &["foo"],
        ),
    );
    installed.insert("app/B-1", meta(&[], &[], &[]));
    installed.insert("app/C-1", meta(&[], &[], &[]));

    let params = ResolveParams::default().with_use(["foo"]);
    let resolver = Resolver::new(&available, &installed, params);
    let clean = resolver.depclean(&["app/A"]);
    assert!(
        clean.contains(&"app/C-1".to_string()),
        "C unreferenced: {clean:?}"
    );
    assert!(
        !clean.contains(&"app/B-1".to_string()),
        "B kept via foo: {clean:?}"
    );
}
