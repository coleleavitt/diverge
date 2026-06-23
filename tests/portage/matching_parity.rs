//! Ports of upstream `match_from_list` / `best_match_to_list` / required-use
//! flag extraction behavior.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/dep/test_match_from_list.py`
//! - `research/portage/lib/portage/tests/dep/test_best_match_to_list.py`
//! - `research/portage/lib/portage/tests/dep/test_get_required_use_flags.py`

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

#[test]
fn match_from_list_operator_cases() {
    // Ported from test_match_from_list.py (operator-bearing rows).
    assert_eq!(
        matched_cpvs("=sys-apps/portage-45*", &[]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("=sys-apps/portage-45*", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("!=sys-apps/portage-45*", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("!!=sys-apps/portage-45*", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("=sys-apps/portage-045", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("=sys-apps/portage-045", &["sys-apps/portage-046"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("~sys-apps/portage-045", &["sys-apps/portage-045-r1"]),
        vec!["sys-apps/portage-045-r1"]
    );
    assert_eq!(
        matched_cpvs("~sys-apps/portage-045", &["sys-apps/portage-046-r1"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("<=sys-apps/portage-045", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("<=sys-apps/portage-045", &["sys-apps/portage-046"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("<sys-apps/portage-046", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs("<sys-apps/portage-046", &["sys-apps/portage-046"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs(">=sys-apps/portage-045", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs(">=sys-apps/portage-047", &["sys-apps/portage-046-r1"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs(">sys-apps/portage-044", &["sys-apps/portage-045"]),
        vec!["sys-apps/portage-045"]
    );
    assert_eq!(
        matched_cpvs(">sys-apps/portage-047", &["sys-apps/portage-046-r1"]),
        Vec::<String>::new()
    );
}

#[test]
fn match_from_list_glob_boundary_cases() {
    // bug 560466: =* matches only on boundaries between version parts.
    assert_eq!(
        matched_cpvs("=cat/pkg-1-r1*", &["cat/pkg-1_alpha1"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-1.1*", &["cat/pkg-1.1-r1", "cat/pkg-1.10-r1"]),
        vec!["cat/pkg-1.1-r1"]
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-1-r1*", &["cat/pkg-1-r11"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-1_pre*", &["cat/pkg-1_pre1"]),
        vec!["cat/pkg-1_pre1"]
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-1-r1*", &["cat/pkg-1-r1"]),
        vec!["cat/pkg-1-r1"]
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-1-r11*", &["cat/pkg-1-r11"]),
        vec!["cat/pkg-1-r11"]
    );
    // Leading-zero normalization.
    assert_eq!(
        matched_cpvs("=cat/pkg-1-r11*", &["cat/pkg-01-r11"]),
        vec!["cat/pkg-01-r11"]
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-01-r11*", &["cat/pkg-1-r11"]),
        vec!["cat/pkg-1-r11"]
    );
    assert_eq!(
        matched_cpvs("=cat/pkg-01-r11*", &["cat/pkg-001-r11"]),
        vec!["cat/pkg-001-r11"]
    );
    assert_eq!(
        matched_cpvs(
            "=sys-fs/udev-1*",
            &["sys-fs/udev-123", "sys-fs/udev-123-r1"]
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("=sys-fs/udev-123*", &["sys-fs/udev-123-r1"]),
        vec!["sys-fs/udev-123-r1"]
    );
    assert_eq!(
        matched_cpvs("=sys-apps/portage-2*", &["sys-apps/portage-2.1"]),
        vec!["sys-apps/portage-2.1"]
    );
    assert_eq!(
        matched_cpvs("=sys-apps/portage-2.1*", &["sys-apps/portage-2.1.2"]),
        vec!["sys-apps/portage-2.1.2"]
    );
}

#[test]
fn match_from_list_extended_cp_cases() {
    assert_eq!(
        matched_cpvs("*/*", &["sys-fs/udev-456"]),
        vec!["sys-fs/udev-456"]
    );
    assert_eq!(
        matched_cpvs("sys-fs/*", &["sys-fs/udev-456"]),
        vec!["sys-fs/udev-456"]
    );
    assert_eq!(
        matched_cpvs("*/udev", &["sys-fs/udev-456"]),
        vec!["sys-fs/udev-456"]
    );
    assert_eq!(
        matched_cpvs("dev-libs/*", &["sys-apps/portage-2.1.2"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("*/tar", &["sys-apps/portage-2.1.2"]),
        Vec::<String>::new()
    );
    assert_eq!(
        matched_cpvs("*/*", &["dev-libs/A-1", "dev-libs/B-1"]),
        vec!["dev-libs/A-1", "dev-libs/B-1"]
    );
    assert_eq!(
        matched_cpvs("dev-libs/*", &["dev-libs/A-1", "sci-libs/B-1"]),
        vec!["dev-libs/A-1"]
    );
}

#[test]
fn match_from_list_slot_and_repo_cases() {
    let pool = vec![
        candidate("=sys-apps/portage-045:0"),
        candidate("=sys-apps/portage-045:1"),
    ];
    let got: Vec<&str> = match_from_list(&atom("sys-apps/portage:0"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["sys-apps/portage-045"]);

    let repo_pool = vec![
        candidate("=dev-libs/A-1::repo1"),
        candidate("=dev-libs/A-1::repo2"),
    ];
    let got: Vec<Option<&str>> = match_from_list(&atom("dev-libs/A::repo2"), &repo_pool)
        .iter()
        .map(|c| c.repo.as_deref())
        .collect();
    assert_eq!(got, vec![Some("repo2")]);
}

#[test]
fn match_from_list_use_dep_cases() {
    let pool = vec![
        candidate("=dev-libs/A-1[foo]"),
        candidate("=dev-libs/A-2[-foo]"),
    ];
    let got: Vec<&str> = match_from_list(&atom("dev-libs/A[foo]"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1"]);

    let got: Vec<&str> = match_from_list(&atom("dev-libs/A[-foo]"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-2"]);

    // [foo,bar] requires both; only A-2 declares both.
    let pool = vec![
        candidate("=dev-libs/A-1[foo]"),
        candidate("=dev-libs/A-2[foo,bar]"),
    ];
    let got: Vec<&str> = match_from_list(&atom("dev-libs/A[foo,bar]"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-2"]);
}

#[test]
fn best_match_to_list_precedence() {
    // =cpv (6) beats plain cp (1).
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![atom("dev-libs/A"), atom("=dev-libs/A-1")];
    assert_eq!(
        best_match_to_list(&cand, &list).map(|a| a.to_string()),
        Some("=dev-libs/A-1".to_string())
    );

    // cp:slot (3) beats a non-matching cp.
    let cand = Candidate::new("dev-libs/A-1").with_slot("0");
    let list = vec![atom("dev-libs/B"), atom("=dev-libs/A-1:0")];
    assert_eq!(
        best_match_to_list(&cand, &list).map(|a| a.to_string()),
        Some("=dev-libs/A-1:0".to_string())
    );

    // =cpv:slot (6) beats an extended-syntax wildcard (-1).
    let cand = Candidate::new("dev-libs/A-1").with_slot("0");
    let list = vec![atom("dev-libs/*"), atom("=dev-libs/A-1:0")];
    assert_eq!(
        best_match_to_list(&cand, &list).map(|a| a.to_string()),
        Some("=dev-libs/A-1:0".to_string())
    );
}

#[test]
fn required_use_flag_extraction() {
    // Ported from test_get_required_use_flags.py.
    let flags = |s: &str| {
        let mut v: Vec<String> = get_required_use_flags(s).unwrap().into_iter().collect();
        v.sort();
        v
    };
    assert_eq!(flags("a b c"), vec!["a", "b", "c"]);
    assert_eq!(flags("|| ( a b c )"), vec!["a", "b", "c"]);
    assert_eq!(flags("^^ ( a b c )"), vec!["a", "b", "c"]);
    assert_eq!(flags("?? ( a b c )"), vec!["a", "b", "c"]);
    assert_eq!(flags("a? ( b )"), vec!["a", "b"]);
    assert_eq!(flags("!a? ( b )"), vec!["a", "b"]);
    assert_eq!(
        flags("|| ( a b ) c ^^ ( d e )"),
        vec!["a", "b", "c", "d", "e"]
    );
    assert!(get_required_use_flags("|| ( a b ) (").is_err());
}
