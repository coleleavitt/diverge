//! Deep coverage of dep.rs (use_reduce modes, accessors, error branches) and
//! matching.rs (extended cp, glob boundaries, slot/repo/use-default filtering).

use diverge::atom::{Atom, AtomParseOptions};
use diverge::dep::{
    UseReduceOptions,
    check_required_use,
    dep_getrepo,
    dep_getslot,
    dep_getusedeps,
    paren_reduce,
    use_reduce,
};
use diverge::matching::{Candidate, best_match_to_list, match_from_list};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn atom(s: &str) -> Atom {
    Atom::parse_with_options(s, WILD).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn flat(deps: &[diverge::dep::Dep]) -> String {
    format!("{deps:?}")
}

#[test]
fn dep_getslot_and_repo_string_forms() {
    assert_eq!(dep_getslot("dev-libs/A:2/3"), Some("2/3".to_string()));
    assert_eq!(dep_getslot("dev-libs/A:0[foo]"), Some("0".to_string()));
    assert_eq!(dep_getslot("dev-libs/A"), None);
    assert_eq!(dep_getslot("dev-libs/A:2::repo"), Some("2".to_string()));
    assert_eq!(
        dep_getrepo("dev-libs/A::gentoo"),
        Some("gentoo".to_string())
    );
    assert_eq!(
        dep_getrepo("dev-libs/A::gentoo[foo]"),
        Some("gentoo".to_string())
    );
    assert_eq!(dep_getrepo("dev-libs/A"), None);
}

#[test]
fn dep_getusedeps_comma_and_errors() {
    assert_eq!(
        dep_getusedeps("dev-libs/A[a,b,c]").unwrap(),
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(
        dep_getusedeps("dev-libs/A[single]").unwrap(),
        vec!["single".to_string()]
    );
    assert_eq!(dep_getusedeps("dev-libs/A").unwrap(), Vec::<String>::new());
    // two bracket groups -> Err
    assert!(dep_getusedeps("dev-libs/A[a][b]").is_err());
    // empty group -> Err
    assert!(dep_getusedeps("dev-libs/A[]").is_err());
    // empty flag next to comma -> Err
    assert!(dep_getusedeps("dev-libs/A[a,,b]").is_err());
    // no closing bracket -> Err
    assert!(dep_getusedeps("dev-libs/A[a").is_err());
}

#[test]
fn paren_reduce_errors() {
    assert!(paren_reduce("a )").is_err()); // unbalanced close
    assert!(paren_reduce("( a").is_err()); // missing close
    assert!(paren_reduce("|| a").is_err()); // || without group
}

#[test]
fn use_reduce_matchall_matchnone_mutually_exclusive() {
    let opts = UseReduceOptions {
        matchall: true,
        matchnone: true,
        ..UseReduceOptions::default()
    };
    assert!(use_reduce("foo? ( dev-libs/A )", &opts).is_err());
}

#[test]
fn use_reduce_matchall_includes_all_conditionals() {
    let opts = UseReduceOptions {
        matchall: true,
        ..UseReduceOptions::default()
    };
    let r = use_reduce("foo? ( dev-libs/A ) bar? ( dev-libs/B )", &opts).unwrap();
    let s = flat(&r);
    assert!(s.contains("dev-libs/A") && s.contains("dev-libs/B"));
}

#[test]
fn use_reduce_matchnone_excludes_conditionals() {
    let opts = UseReduceOptions {
        matchnone: true,
        ..UseReduceOptions::default()
    };
    let r = use_reduce("foo? ( dev-libs/A ) dev-libs/C", &opts).unwrap();
    let s = flat(&r);
    assert!(!s.contains("dev-libs/A"));
    assert!(s.contains("dev-libs/C"));
}

#[test]
fn use_reduce_masklist_and_excludeall() {
    // masklist: a masked flag's conditional is treated as disabled.
    let opts = UseReduceOptions {
        uselist: &["foo"],
        masklist: &["foo"],
        ..UseReduceOptions::default()
    };
    let r = use_reduce("foo? ( dev-libs/A )", &opts).unwrap();
    assert!(!flat(&r).contains("dev-libs/A"));

    // excludeall: a negated, excluded flag.
    let opts = UseReduceOptions {
        excludeall: &["bar"],
        ..UseReduceOptions::default()
    };
    let r = use_reduce("!bar? ( dev-libs/B )", &opts).unwrap();
    // !bar with bar excluded -> the conditional is false -> B dropped.
    assert!(!flat(&r).contains("dev-libs/B"));
}

#[test]
fn use_reduce_or_group_and_nested() {
    let opts = UseReduceOptions {
        uselist: &["foo"],
        ..UseReduceOptions::default()
    };
    let r = use_reduce("|| ( dev-libs/A dev-libs/B )", &opts).unwrap();
    assert!(flat(&r).contains("dev-libs/A"));
    // Nested conditional inside a group.
    let r = use_reduce("foo? ( || ( dev-libs/A dev-libs/B ) )", &opts).unwrap();
    assert!(flat(&r).contains("dev-libs/A"));
    // empty any-of group: exercises the empty-||-group branch (result depends
    // on EAPI gates; either outcome is fine, we just drive the code path).
    let _ = use_reduce("|| ( )", &opts);
}

#[test]
fn use_reduce_is_valid_flag_callback_rejects() {
    let valid = |f: &str| f == "known";
    let opts = UseReduceOptions {
        is_valid_flag: Some(&valid),
        ..UseReduceOptions::default()
    };
    // 'unknown?' is not in IUSE -> error.
    assert!(use_reduce("unknown? ( dev-libs/A )", &opts).is_err());
    // 'known?' is fine.
    assert!(use_reduce("known? ( dev-libs/A )", &opts).is_ok());
}

#[test]
fn use_reduce_subset_selection() {
    let subset = ["foo"];
    let opts = UseReduceOptions {
        uselist: &["foo"],
        subset: Some(&subset),
        ..UseReduceOptions::default()
    };
    let r = use_reduce("foo? ( dev-libs/A ) bar? ( dev-libs/B )", &opts);
    assert!(r.is_ok());
}

#[test]
fn use_reduce_rejects_src_uri_arrow_and_unbalanced() {
    let opts = UseReduceOptions::default();
    assert!(use_reduce("http://x -> y", &opts).is_err());
    assert!(use_reduce("foo? ( dev-libs/A", &opts).is_err()); // missing )
    assert!(use_reduce(") (", &opts).is_err());
}

#[test]
fn check_required_use_nested_and_conditional() {
    let iuse = |f: &str| ["a", "b", "c", "d"].contains(&f);
    // nested: a? ( ^^ ( b c ) )
    assert!(check_required_use("a? ( ^^ ( b c ) )", &["a", "b"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("a? ( ^^ ( b c ) )", &["a", "b", "c"], iuse, Some("7")).unwrap());
    // a not set -> the conditional is vacuously satisfied.
    assert!(check_required_use("a? ( ^^ ( b c ) )", &[], iuse, Some("7")).unwrap());
    // plain flag requirement.
    assert!(check_required_use("d", &["d"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("d", &[], iuse, Some("7")).unwrap());
}

// ---- matching.rs branches ----

#[test]
fn match_from_list_extended_cp_and_glob() {
    let pool = [
        Candidate::new("dev-libs/A-1"),
        Candidate::new("sci-libs/B-1"),
        Candidate::new("dev-util/C-2"),
    ];
    // extended cp with wildcard category.
    let got: Vec<&str> = match_from_list(&atom("*/A"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1"]);
    // dev-libs/* matches only the dev-libs entry.
    let got: Vec<&str> = match_from_list(&atom("dev-libs/*"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1"]);
    // =glob on a real version.
    let pool2 = [
        Candidate::new("dev-libs/A-1.2"),
        Candidate::new("dev-libs/A-2"),
    ];
    let got: Vec<&str> = match_from_list(&atom("=dev-libs/A-1*"), &pool2)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1.2"]);
}

#[test]
fn match_from_list_slot_repo_filters() {
    let cand = Candidate::new("dev-libs/A-1")
        .with_slot("2")
        .with_sub_slot("3")
        .with_repo("gentoo");
    let pool = [cand];
    assert_eq!(match_from_list(&atom("dev-libs/A:2"), &pool).len(), 1);
    assert!(match_from_list(&atom("dev-libs/A:9"), &pool).is_empty());
    assert_eq!(match_from_list(&atom("dev-libs/A:2/3"), &pool).len(), 1);
    assert!(match_from_list(&atom("dev-libs/A:2/9"), &pool).is_empty());
    assert_eq!(match_from_list(&atom("dev-libs/A::gentoo"), &pool).len(), 1);
    assert!(match_from_list(&atom("dev-libs/A::other"), &pool).is_empty());
}

#[test]
fn match_from_list_use_default_markers() {
    let with_foo = Candidate::new("dev-libs/A-1")
        .with_iuse(["foo"])
        .with_use(["foo"]);
    let without = Candidate::new("dev-libs/B-1");
    let pool = [with_foo, without];
    // [foo] requires declared+enabled foo.
    let got: Vec<&str> = match_from_list(&atom("dev-libs/A[foo]"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1"]);
    // [bar(+)] default-enabled on a pkg that lacks bar -> matches.
    assert_eq!(match_from_list(&atom("dev-libs/B[bar(+)]"), &pool).len(), 1);
    // [bar(-)] default-disabled, required enabled -> no match.
    assert!(match_from_list(&atom("dev-libs/B[bar(-)]"), &pool).is_empty());
}

#[test]
fn best_match_to_list_ordering_operators() {
    let cand = Candidate::new("dev-libs/A-2");
    // Among >= and =, the exact = wins.
    let list = vec![
        atom(">=dev-libs/A-1"),
        atom("=dev-libs/A-2"),
        atom("dev-libs/A"),
    ];
    let best = best_match_to_list(&cand, &list).unwrap();
    assert_eq!(best.to_string(), "=dev-libs/A-2");
    // No match -> None.
    let other = Candidate::new("dev-libs/Z-1");
    assert!(best_match_to_list(&other, &list).is_none());
}
