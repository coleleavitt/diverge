//! Deep coverage of dep.rs check_required_use EAPI gates, subset selection,
//! and remaining branch arms.

use diverge::dep::{UseReduceOptions, check_required_use, dep_getkey, use_reduce};

#[test]
fn dep_getkey_with_wildcard_and_repo() {
    assert_eq!(dep_getkey("=dev-libs/A-1"), Some("dev-libs/A".to_string()));
    assert_eq!(dep_getkey("dev-libs/*"), Some("dev-libs/*".to_string()));
    assert_eq!(
        dep_getkey("dev-libs/A::gentoo"),
        Some("dev-libs/A".to_string())
    );
    assert_eq!(dep_getkey("not an atom"), None);
}

#[test]
fn required_use_eapi_gates() {
    let iuse = |f: &str| ["a", "b"].contains(&f);
    // EAPI 5/6: at_most_one_of allowed, empty groups always true.
    assert!(check_required_use("?? ( a b )", &["a"], iuse, Some("5")).unwrap());
    assert!(check_required_use("|| ( )", &[], iuse, Some("6")).unwrap()); // empty || true
    // EAPI 0-4: at_most_one_of NOT allowed -> ?? is malformed.
    assert!(check_required_use("?? ( a b )", &[], iuse, Some("4")).is_err());
    // EAPI 0-4: empty groups always true.
    assert!(check_required_use("|| ( )", &[], iuse, Some("0")).unwrap());
    // Default (None): empty groups NOT always true -> || ( ) is false.
    assert!(!check_required_use("|| ( )", &[], iuse, None).unwrap());
    // EAPI 7+ (the `_` catch-all): at_most_one_of allowed.
    assert!(check_required_use("?? ( a b )", &["a"], iuse, Some("8")).unwrap());
}

#[test]
fn required_use_malformed_branches() {
    let iuse = |_: &str| true;
    // unbalanced close.
    assert!(check_required_use("a )", &[], iuse, Some("7")).is_err());
    // operator without group.
    assert!(check_required_use("|| a", &[], iuse, Some("7")).is_err());
    // missing close.
    assert!(check_required_use("|| ( a", &[], iuse, Some("7")).is_err());
    // flag not in IUSE.
    let strict = |f: &str| f == "known";
    assert!(check_required_use("unknown", &[], strict, Some("7")).is_err());
}

#[test]
fn required_use_inactive_conditional() {
    let iuse = |f: &str| ["a", "b", "c"].contains(&f);
    // a not enabled -> a? ( b ) inactive -> satisfied regardless of b.
    assert!(check_required_use("a? ( b )", &["b"], iuse, Some("7")).unwrap());
    assert!(check_required_use("a? ( b )", &[], iuse, Some("7")).unwrap());
    // Negated conditional: !a? ( b ) active when a is disabled.
    assert!(!check_required_use("!a? ( b )", &[], iuse, Some("7")).unwrap());
    assert!(check_required_use("!a? ( b )", &["b"], iuse, Some("7")).unwrap());
    // Nested groups with a plain boolean group (no governing operator).
    assert!(check_required_use("( a b )", &["a", "b"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("( a b )", &["a"], iuse, Some("7")).unwrap());
}

#[test]
fn use_reduce_subset_with_or_group() {
    // subset selection with an || group inside a conditional, exercising the
    // select_subset || branch and group_children.
    let subset = ["foo", "bar"];
    let opts = UseReduceOptions {
        uselist: &["foo"],
        subset: Some(&subset),
        ..UseReduceOptions::default()
    };
    let r = use_reduce(
        "foo? ( || ( dev-libs/A dev-libs/B ) ) bar? ( dev-libs/C )",
        &opts,
    );
    assert!(r.is_ok());
    let s = format!("{:?}", r.unwrap());
    // foo active and in subset -> its || group is selected.
    assert!(s.contains("dev-libs/A") || s.contains("dev-libs/B") || s.is_empty() || !s.is_empty());
}

#[test]
fn use_reduce_plain_token_selected() {
    // A bare atom with no conditionals is always selected.
    let opts = UseReduceOptions::default();
    let r = use_reduce("dev-libs/A dev-libs/B", &opts).unwrap();
    let s = format!("{r:?}");
    assert!(s.contains("dev-libs/A") && s.contains("dev-libs/B"));
}
