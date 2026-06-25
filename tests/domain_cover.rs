//! Coverage for atom / dep / matching branches.

use diverge::atom::{Atom, AtomParseOptions, is_valid_atom};
use diverge::dep::{
    UseReduceOptions,
    check_required_use,
    dep_getcpv,
    dep_getrepo,
    dep_getslot,
    dep_getusedeps,
    get_operator,
    isjustname,
    paren_reduce,
    use_reduce,
};
use diverge::matching::{Candidate, best_match_to_list, get_required_use_flags, match_from_list};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn atom(s: &str) -> Atom {
    Atom::parse_with_options(s, WILD).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn atom_parses_blockers_slots_repo_usedeps() {
    let a = atom("!!=dev-libs/A-1.2:3/4::gentoo[foo,-bar,baz(+)]");
    assert_eq!(a.cp(), "dev-libs/A");
    assert_eq!(a.slot(), Some("3"));
    assert_eq!(a.sub_slot(), Some("4"));
    assert_eq!(a.repo.as_deref(), Some("gentoo"));
    // Display round-trips back through the parser.
    let rendered = a.to_string();
    let reparsed = atom(&rendered);
    assert_eq!(reparsed.cp(), "dev-libs/A");
    assert_eq!(reparsed.slot(), Some("3"));
}

#[test]
fn atom_operators_round_trip() {
    for s in [
        "=dev-libs/A-1",
        ">=dev-libs/A-1",
        ">dev-libs/A-1",
        "<=dev-libs/A-1",
        "<dev-libs/A-1",
        "~dev-libs/A-1",
        "=dev-libs/A-1*",
        "dev-libs/A",
        "dev-libs/A:0",
        "dev-libs/A:=",
    ] {
        let a = atom(s);
        let rendered = a.to_string();
        assert!(!rendered.is_empty(), "{s} -> empty");
        // Reparse must keep the same cp.
        assert_eq!(atom(&rendered).cp(), a.cp(), "round-trip {s} -> {rendered}");
    }
}

#[test]
fn is_valid_atom_true_and_false() {
    assert!(is_valid_atom("dev-libs/A", WILD));
    assert!(is_valid_atom(">=dev-libs/A-1.2", WILD));
    assert!(!is_valid_atom("dev-libs/A[doc", WILD));
    assert!(!is_valid_atom("=dev-libs/A", WILD)); // = without version
    assert!(!is_valid_atom("", WILD));
}

#[test]
fn dep_accessors_tricky() {
    assert_eq!(get_operator(">=dev-libs/A-1"), Some(">=".to_string()));
    assert_eq!(get_operator("dev-libs/A"), None);
    assert_eq!(
        dep_getcpv(">=dev-libs/A-1:2"),
        Some("dev-libs/A-1".to_string())
    );
    assert_eq!(dep_getslot("dev-libs/A:2/3"), Some("2/3".to_string()));
    assert_eq!(dep_getslot("dev-libs/A"), None);
    assert_eq!(dep_getrepo("dev-libs/A::r"), Some("r".to_string()));
    assert!(isjustname("dev-libs/A"));
    assert!(!isjustname("dev-libs/A-1"));
    let uses = dep_getusedeps("dev-libs/A[a,-b,c?]").unwrap();
    assert!(uses.contains(&"a".to_string()));
}

#[test]
fn paren_reduce_nested_groups() {
    let r = paren_reduce("|| ( a ( b c ) )").unwrap();
    // The top-level should contain the || token and a nested group.
    assert!(!r.is_empty());
}

#[test]
fn use_reduce_conditionals_and_flags() {
    let enabled = ["foo"];
    let opts = UseReduceOptions {
        uselist: &enabled,
        ..UseReduceOptions::default()
    };
    let r = use_reduce("foo? ( dev-libs/A ) bar? ( dev-libs/B )", &opts).unwrap();
    let flat = format!("{r:?}");
    assert!(flat.contains("dev-libs/A"));
    assert!(!flat.contains("dev-libs/B"));
}

#[test]
fn check_required_use_variants() {
    let iuse = |f: &str| ["foo", "bar", "baz"].contains(&f);
    // ^^ exactly one
    assert!(check_required_use("^^ ( foo bar )", &["foo"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("^^ ( foo bar )", &["foo", "bar"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("^^ ( foo bar )", &[], iuse, Some("7")).unwrap());
    // || at least one
    assert!(check_required_use("|| ( foo bar )", &["bar"], iuse, Some("7")).unwrap());
    // ?? at most one
    assert!(check_required_use("?? ( foo bar )", &["foo"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("?? ( foo bar )", &["foo", "bar"], iuse, Some("7")).unwrap());
    // conditional foo? ( bar )
    assert!(!check_required_use("foo? ( bar )", &["foo"], iuse, Some("7")).unwrap());
    assert!(check_required_use("foo? ( bar )", &["foo", "bar"], iuse, Some("7")).unwrap());
    // malformed -> Err
    assert!(check_required_use("|| (", &[], iuse, Some("7")).is_err());
}

#[test]
fn match_from_list_default_use_modifiers() {
    // [foo(+)] : missing flag defaults to enabled -> matches a pkg without it.
    let pool = [Candidate::new("dev-libs/A-1")];
    let a = atom("dev-libs/A[foo(+)]");
    assert_eq!(match_from_list(&a, &pool).len(), 1);
    // [foo(-)] : missing flag defaults to disabled -> [foo(-)] enabled-required fails.
    let a = atom("dev-libs/A[foo(-)]");
    assert!(match_from_list(&a, &pool).is_empty());
}

#[test]
fn best_match_precedence_and_required_use_flags() {
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![atom("dev-libs/A"), atom("=dev-libs/A-1")];
    assert_eq!(
        best_match_to_list(&cand, &list).map(|a| a.to_string()),
        Some("=dev-libs/A-1".to_string())
    );
    let mut flags: Vec<String> = get_required_use_flags("|| ( a b ) c? ( d )")
        .unwrap()
        .into_iter()
        .collect();
    flags.sort();
    assert_eq!(flags, vec!["a", "b", "c", "d"]);
    assert!(get_required_use_flags("( (").is_err());
}
