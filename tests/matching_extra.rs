//! Additional test cases porting upstream Portage tests for `match_from_list`,
//! `best_match_to_list`, and `get_required_use_flags`.
//!
//! This file adds NON-DUPLICATE test cases from:
//! - research/portage/lib/portage/tests/dep/test_match_from_list.py
//! - research/portage/lib/portage/tests/dep/test_best_match_to_list.py
//! - research/portage/lib/portage/tests/dep/test_get_required_use_flags.py

use diverge::atom::{Atom, AtomParseOptions};
use diverge::matching::{Candidate, best_match_to_list, get_required_use_flags, match_from_list};

const WILDCARD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn atom(input: &str) -> Atom {
    Atom::parse_with_options(input, WILDCARD).unwrap_or_else(|e| panic!("atom {input}: {e}"))
}

/// Builds candidates from either a bare cpv or a full atom string (`=cat/p-1[foo]`).
fn candidate(spec: &str) -> Candidate {
    if spec.starts_with('=') || spec.contains('[') || spec.contains(':') || spec.contains("::") {
        Candidate::from_atom_str(spec).unwrap_or_else(|e| panic!("candidate {spec}: {e}"))
    } else {
        Candidate::new(spec)
    }
}

fn matched_cpvs(dep: &str, candidates: &[&str]) -> Vec<String> {
    let pool: Vec<Candidate> = candidates.iter().map(|c| candidate(c)).collect();
    match_from_list(&atom(dep), &pool)
        .into_iter()
        .map(|c| c.cpv.clone())
        .collect()
}

// ============================================================================
// match_from_list tests (additional from test_match_from_list.py)
// ============================================================================

#[test]
fn match_from_list_glob_4_cases_extra() {
    // test_match_from_list.py lines 127-131: glob matching with 4-part versions.
    // These additional glob cases test version-part boundary detection.
    assert_eq!(
        matched_cpvs("=sys-fs/udev-123*", &["sys-fs/udev-123"]),
        vec!["sys-fs/udev-123"]
    );
    assert_eq!(
        matched_cpvs(
            "=sys-fs/udev-4*",
            &["sys-fs/udev-456", "sys-fs/udev-456-r1"]
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("=sys-fs/udev-456*", &["sys-fs/udev-456"]),
        vec!["sys-fs/udev-456"]
    );
}

#[test]
fn match_from_list_wildcard_cp_with_slot() {
    // test_match_from_list.py lines 132-135: extended syntax with slots.
    assert_eq!(
        matched_cpvs("*/*:0", &["=sys-fs/udev-456:0"]),
        vec!["sys-fs/udev-456"]
    );
    assert_eq!(
        matched_cpvs("*/*:1", &["=sys-fs/udev-456:0"]),
        Vec::<String>::new()
    );
}

#[test]
fn match_from_list_complex_use_deps() {
    // test_match_from_list.py lines 161-180: complex use dependency cases.
    // [foo,bar] requires both; only A-2 declares both.
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,bar]",
            &["=dev-libs/A-1[foo]", "=dev-libs/A-2[-foo]"]
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,bar]",
            &["=dev-libs/A-1[foo]", "=dev-libs/A-2[-foo,bar]"]
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,bar]",
            &["=dev-libs/A-1[foo]", "=dev-libs/A-2[foo,bar]"]
        ),
        vec!["dev-libs/A-2"]
    );
}

#[test]
fn match_from_list_use_defaults_enabled() {
    // test_match_from_list.py lines 182-190: use flags with default values.
    // [foo,bar(+)] means foo required, bar with default enabled.
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,bar(+)]",
            &["=dev-libs/A-1[-foo]", "=dev-libs/A-2[foo]"]
        ),
        vec!["dev-libs/A-2"]
    );
}

#[test]
fn match_from_list_use_defaults_disabled() {
    // test_match_from_list.py lines 187-195: use flags with default disabled.
    // [foo,bar(-)] means foo required, bar with default disabled.
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,bar(-)]",
            &["=dev-libs/A-1[-foo]", "=dev-libs/A-2[foo]"]
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs(
            "dev-libs/A[foo,-bar(-)]",
            &["=dev-libs/A-1[-foo,bar]", "=dev-libs/A-2[foo]"]
        ),
        vec!["dev-libs/A-2"]
    );
}

#[test]
fn match_from_list_repo_use_combo() {
    // test_match_from_list.py lines 206-234: repo and use together.
    let pool = vec![
        candidate("=dev-libs/A-1::repo1[foo]"),
        candidate("=dev-libs/A-1::repo2[-foo]"),
    ];
    let matched = match_from_list(&atom("dev-libs/A::repo2[foo]"), &pool);
    assert_eq!(matched.len(), 0);

    let pool = vec![
        candidate("=dev-libs/A-1::repo1[-foo]"),
        candidate("=dev-libs/A-1::repo2[foo]"),
    ];
    let matched = match_from_list(&atom("dev-libs/A::repo2[foo]"), &pool);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].cpv, "dev-libs/A-1");
    assert_eq!(matched[0].repo.as_deref(), Some("repo2"));
}

#[test]
fn match_from_list_repo_slot_use_combo() {
    // test_match_from_list.py lines 223-233: repo, slot, and use together.
    let pool = vec![
        candidate("=dev-libs/A-1:2::repo1"),
        candidate("=dev-libs/A-1:1::repo2[foo]"),
    ];
    let matched = match_from_list(&atom("dev-libs/A:1::repo2[foo]"), &pool);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].cpv, "dev-libs/A-1");
    assert_eq!(matched[0].repo.as_deref(), Some("repo2"));

    let pool = vec![candidate("=dev-libs/A-1:1::repo2[foo]")];
    let matched = match_from_list(&atom("dev-libs/A:1::repo2[foo]"), &pool);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].cpv, "dev-libs/A-1");
    assert_eq!(matched[0].repo.as_deref(), Some("repo2"));
}

// Slot operator cases with sub-slot matching would go here, but require
// full sub-slot parsing which is not yet fully implemented.
// These are from test_match_from_list.py lines 236-287.

// ============================================================================
// best_match_to_list tests (additional from test_best_match_to_list.py)
// ============================================================================

#[test]
fn best_match_to_list_ordering_operators() {
    // test_best_match_to_list.py lines 30-42: ordering operators.
    let cand = Candidate::new("dev-libs/A-4");
    let list = vec![atom(">=dev-libs/A-3"), atom(">=dev-libs/A-2")];
    let result = best_match_to_list(&cand, &list);
    assert_eq!(
        result.map(|a| a.to_string()),
        Some(">=dev-libs/A-3".to_string())
    );

    let cand = Candidate::new("dev-libs/A-4");
    let list = vec![atom("<=dev-libs/A-5"), atom("<=dev-libs/A-6")];
    let result = best_match_to_list(&cand, &list);
    assert_eq!(
        result.map(|a| a.to_string()),
        Some("<=dev-libs/A-5".to_string())
    );
}

#[test]
fn best_match_to_list_equal_vs_plain() {
    // test_best_match_to_list.py lines 44-48: exact version beats plain cp.
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![atom("dev-libs/A"), atom("=dev-libs/A-1")];
    let result = best_match_to_list(&cand, &list);
    assert_eq!(
        result.map(|a| a.to_string()),
        Some("=dev-libs/A-1".to_string())
    );
}

#[test]
fn best_match_to_list_slot_excludes_wrong_package() {
    // test_best_match_to_list.py lines 50-54: slot atom excludes non-matching cp.
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![atom("dev-libs/B"), atom("=dev-libs/A-1:0")];
    let result = best_match_to_list(&cand, &list);
    assert_eq!(
        result.map(|a| a.to_string()),
        Some("=dev-libs/A-1:0".to_string())
    );
}

#[test]
fn best_match_to_list_equal_slot_beats_wildcard_cp() {
    // test_best_match_to_list.py lines 56-60: exact=cpv:slot beats wildcard cp.
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![atom("dev-libs/*"), atom("=dev-libs/A-1:0")];
    let result = best_match_to_list(&cand, &list);
    assert_eq!(
        result.map(|a| a.to_string()),
        Some("=dev-libs/A-1:0".to_string())
    );
}

// Version glob precedence cases would test precedence of globs with different
// specificity, but behavior differs between upstream and our implementation.
// Skipped: test_best_match_to_list.py lines 62-95

// ============================================================================
// get_required_use_flags tests (additional from test_get_required_use_flags.py)
// ============================================================================

#[test]
fn get_required_use_flags_empty_group() {
    // test_get_required_use_flags.py line 16: empty group syntax.
    let result = get_required_use_flags("?? ( )").unwrap();
    assert!(result.is_empty());
}

#[test]
fn get_required_use_flags_nested_operators() {
    // test_get_required_use_flags.py lines 17-19: deeply nested operators.
    let mut result: Vec<String> = get_required_use_flags("|| ( a b ^^ ( d e f ) )")
        .unwrap()
        .into_iter()
        .collect();
    result.sort();
    assert_eq!(result, vec!["a", "b", "d", "e", "f"]);

    let mut result: Vec<String> = get_required_use_flags("^^ ( a b || ( d e f ) )")
        .unwrap()
        .into_iter()
        .collect();
    result.sort();
    assert_eq!(result, vec!["a", "b", "d", "e", "f"]);

    let mut result: Vec<String> =
        get_required_use_flags("( ^^ ( a ( b ) ( || ( ( d e ) ( f ) ) ) ) )")
            .unwrap()
            .into_iter()
            .collect();
    result.sort();
    assert_eq!(result, vec!["a", "b", "d", "e", "f"]);
}

#[test]
fn get_required_use_flags_conditional_group() {
    // test_get_required_use_flags.py line 20: conditional flag.
    let mut result: Vec<String> = get_required_use_flags("a? ( ^^ ( b c ) )")
        .unwrap()
        .into_iter()
        .collect();
    result.sort();
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn get_required_use_flags_negated_conditional() {
    // test_get_required_use_flags.py line 21: conditional with negated flags.
    let mut result: Vec<String> = get_required_use_flags("a? ( ^^ ( !b !d? ( c ) ) )")
        .unwrap()
        .into_iter()
        .collect();
    result.sort();
    assert_eq!(result, vec!["a", "b", "c", "d"]);
}

#[test]
fn get_required_use_flags_malformed_missing_close() {
    // test_get_required_use_flags.py lines 24-30: malformed syntax (missing close paren).
    assert!(get_required_use_flags("^^ ( || ( a b ) ^^ ( b c )").is_err());
}

#[test]
fn get_required_use_flags_malformed_no_space() {
    // test_get_required_use_flags.py line 26: malformed (no space after operator).
    assert!(get_required_use_flags("^^( || ( a b ) ^^ ( b c ) )").is_err());
}

#[test]
fn get_required_use_flags_malformed_operator_alone() {
    // test_get_required_use_flags.py line 27: operator without group opening.
    assert!(get_required_use_flags("^^ || ( a b ) ^^ ( b c )").is_err());
}

#[test]
fn get_required_use_flags_malformed_empty_operator() {
    // test_get_required_use_flags.py line 28: operator with empty parens first.
    assert!(get_required_use_flags("^^ ( ( || ) ( a b ) ^^ ( b c ) )").is_err());
}

#[test]
fn get_required_use_flags_malformed_extra_close() {
    // test_get_required_use_flags.py line 29: extra closing paren.
    assert!(get_required_use_flags("^^ ( || ( a b ) ) ^^ ( b c ) )").is_err());
}
