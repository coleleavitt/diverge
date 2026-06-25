//! Final targeted coverage for specific reachable branch arms.

use diverge::atom::{Atom, AtomError, AtomParseOptions};
use diverge::matching::{Candidate, best_match_to_list, match_from_list};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};
const NOREPO: AtomParseOptions = AtomParseOptions {
    allow_wildcard: false,
    allow_repo: false,
};

fn atom(s: &str) -> Atom {
    Atom::parse_with_options(s, WILD).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn atom_slot_operators_and_errors() {
    // `:*` slot operator (line 437).
    let a = atom("dev-libs/A:*");
    assert_eq!(a.to_string(), "dev-libs/A:*");
    // `:=` operator.
    let a = atom("dev-libs/A:=");
    assert_eq!(a.to_string(), "dev-libs/A:=");
    // `:slot=` operator.
    let a = atom("dev-libs/A:2=");
    assert!(a.to_string().contains(":2"));
    // sub-slot with `*` operator -> invalid.
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A:2/3*", WILD),
        Err(AtomError::InvalidSlot)
    ));
    // empty slot -> invalid.
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A:", WILD),
        Err(AtomError::InvalidSlot)
    ));
    // double colon slot.
    assert!(Atom::parse_with_options("dev-libs/A:1:2", WILD).is_err());
}

#[test]
fn atom_repo_errors() {
    // repo not allowed (line 403).
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A::gentoo", NOREPO),
        Err(AtomError::RepositoryNotAllowed)
    ));
    // empty repo qualifier -> invalid (line 408).
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A::", WILD),
        Err(AtomError::InvalidRepository)
    ));
}

#[test]
fn matching_tilde_and_none_operator() {
    // None operator: bare cp matches any version.
    let pool = [
        Candidate::new("dev-libs/A-1"),
        Candidate::new("dev-libs/A-2"),
    ];
    assert_eq!(match_from_list(&atom("dev-libs/A"), &pool).len(), 2);
    // ~ matches base version ignoring revision.
    let pool = [Candidate::new("dev-libs/A-1.0-r5")];
    assert_eq!(match_from_list(&atom("~dev-libs/A-1.0"), &pool).len(), 1);
    // Equal on exact cpv.
    let pool = [Candidate::new("dev-libs/A-1.0")];
    assert_eq!(match_from_list(&atom("=dev-libs/A-1.0"), &pool).len(), 1);
    assert!(match_from_list(&atom("=dev-libs/A-9.9"), &pool).is_empty());
}

#[test]
fn matching_slot_without_candidate_slot_is_kept() {
    // Candidate carries no slot data -> slot atom cannot disprove (line 305).
    let pool = [Candidate::new("dev-libs/A-1")];
    assert_eq!(match_from_list(&atom("dev-libs/A:5"), &pool).len(), 1);
    // Repo atom but candidate has no repo -> kept (line 402).
    assert_eq!(match_from_list(&atom("dev-libs/A::gentoo"), &pool).len(), 1);
}

#[test]
fn best_match_ordering_tiebreak() {
    // Two ordering operators (value 2 tie); the closer (equal) version wins.
    let cand = Candidate::new("dev-libs/A-2");
    let list = vec![atom(">=dev-libs/A-1"), atom(">=dev-libs/A-2")];
    let best = best_match_to_list(&cand, &list).unwrap();
    // Both are >= (value 2); the tie-break prefers the one equal to candidate.
    assert_eq!(best.to_string(), ">=dev-libs/A-2");
    // Slot-qualified atom gets precedence value 3.
    let cand = Candidate::new("dev-libs/A-1").with_slot("0");
    let list = vec![atom("dev-libs/A"), atom("dev-libs/A:0")];
    let best = best_match_to_list(&cand, &list).unwrap();
    assert_eq!(best.to_string(), "dev-libs/A:0");
}

#[test]
fn matching_extended_wildcard_recursion() {
    // `*foo*`-style category wildcard exercises extended_cp_match recursion.
    let pool = [
        Candidate::new("dev-libs/A-1"),
        Candidate::new("dev-python/B-1"),
    ];
    let got: Vec<&str> = match_from_list(&atom("dev-*/*"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got.len(), 2);
    // `*/A` matches only A.
    let got: Vec<&str> = match_from_list(&atom("*/A"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1"]);
}
