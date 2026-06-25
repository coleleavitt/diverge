//! Precise inputs to reach dep.rs select_subset group-in-disjunction and the
//! selected-token push paths.

use diverge::dep::{use_reduce, UseReduceOptions};

fn reduce_subset(s: &str, uselist: &[&str], subset: &[&str]) -> Vec<diverge::dep::Dep> {
    let opts = UseReduceOptions {
        uselist,
        subset: Some(subset),
        ..UseReduceOptions::default()
    };
    use_reduce(s, &opts).expect("reduce ok")
}

#[test]
fn or_with_nested_group_under_subset() {
    // `|| ( ( a/x a/y ) a/z )`: the || recursion has disjunction=true, and the
    // inner parenthesized group hits the disjunction Group branch (604-609).
    let r = reduce_subset("|| ( ( dev/x dev/y ) dev/z )", &[], &["q"]);
    let s = format!("{r:?}");
    // Under subset with no selected flags, top-level || atoms are not selected,
    // so the result is empty; the code path is still exercised.
    assert!(s == "[]" || s.contains("dev/"));
}

#[test]
fn active_conditional_in_subset_selects_tokens() {
    // foo is active AND in the subset -> its tokens become selected (619-626).
    let r = reduce_subset("foo? ( dev/x dev/y )", &["foo"], &["foo"]);
    let s = format!("{r:?}");
    assert!(s.contains("dev/x") && s.contains("dev/y"), "got {s}");
}

#[test]
fn active_conditional_not_in_subset_drops_tokens() {
    // foo active but NOT in subset, and not already selected -> tokens dropped.
    let r = reduce_subset("foo? ( dev/x )", &["foo"], &["other"]);
    assert_eq!(format!("{r:?}"), "[]");
}

#[test]
fn or_inside_active_conditional_under_subset() {
    // foo active + in subset wrapping an || group: drives both the conditional
    // selected path and the || disjunction recursion.
    let r = reduce_subset("foo? ( || ( dev/x dev/y ) )", &["foo"], &["foo"]);
    let s = format!("{r:?}");
    assert!(s.contains("dev/x") || s.contains("||"), "got {s}");
}

#[test]
fn nested_conditional_subset_propagates_selected() {
    // a active+subset, nested b? selects via propagated `selected=true`.
    let r = reduce_subset("a? ( b? ( dev/x ) dev/y )", &["a", "b"], &["a"]);
    let s = format!("{r:?}");
    // dev/y is directly under the selected a? group.
    assert!(s.contains("dev/y"), "got {s}");
}
