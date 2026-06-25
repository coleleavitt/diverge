//! Edge-arm coverage: empty/`**` keywords, glob normalization, matching
//! from_atom_str error, best-match no-match, depgraph satisfies-other-choice.

use diverge::atom::{Atom, AtomParseOptions};
use diverge::dbapi::{PackageDb, PackageMetadata};
use diverge::depgraph::{ResolveParams, Resolver};
use diverge::matching::{Candidate, best_match_to_list, match_from_list};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn atom(s: &str) -> Atom {
    Atom::parse_with_options(s, WILD).unwrap()
}

fn meta(keywords: &[&str]) -> PackageMetadata {
    PackageMetadata {
        slot: Some("0".into()),
        sub_slot: None,
        repo: Some("r".into()),
        eapi: Some("7".into()),
        iuse: vec![],
        use_enabled: vec![],
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        deps: Default::default(),
    }
}

#[test]
fn keyword_visibility_empty_and_double_star() {
    // Empty KEYWORDS -> visible (depgraph 149-150).
    let mut av = PackageDb::new();
    av.insert("d/A-1", meta(&[]));
    let outcome = Resolver::new(&av, &PackageDb::new(), ResolveParams::default()).resolve(&["d/A"]);
    assert!(
        outcome.is_success(),
        "empty keywords visible: {:?}",
        outcome.error
    );

    // `**` keyword -> visible regardless of arch (line 155).
    let mut av = PackageDb::new();
    av.insert("d/B-1", meta(&["**"]));
    let outcome = Resolver::new(
        &av,
        &PackageDb::new(),
        ResolveParams::default().with_arch("amd64"),
    )
    .resolve(&["d/B"]);
    assert!(
        outcome.is_success(),
        "** keyword visible: {:?}",
        outcome.error
    );
}

#[test]
fn matching_from_atom_str_rejects_invalid() {
    // Candidate::from_atom_str surfaces the parse error (matching.rs 52-59).
    assert!(Candidate::from_atom_str("not a valid atom!!!").is_err());
    assert!(Candidate::from_atom_str("=dev-libs/A-1").is_ok());
}

#[test]
fn matching_glob_leading_zero_normalization() {
    // normalize_glob_version prepends 0 when stripped is empty/non-digit
    // (matching.rs 168-170): `=cat/pkg-0*` against `cat/pkg-0.1`.
    let pool = [Candidate::new("cat/pkg-0.1")];
    assert_eq!(match_from_list(&atom("=cat/pkg-0*"), &pool).len(), 1);
    // A version that is all zeros.
    let pool = [Candidate::new("cat/pkg-0")];
    assert_eq!(match_from_list(&atom("=cat/pkg-0*"), &pool).len(), 1);
}

#[test]
fn matching_glob_no_version_candidate() {
    // glob_matches returns false when the candidate has no version (line 189).
    let pool = [Candidate::new("cat/pkg")]; // bare cp, no version
    assert!(match_from_list(&atom("=cat/pkg-1*"), &pool).is_empty());
}

#[test]
fn best_match_empty_list_is_none() {
    let cand = Candidate::new("dev-libs/A-1");
    assert!(best_match_to_list(&cand, &[]).is_none());
    // A list where nothing matches -> None (line 483 keeps best=None).
    let list = vec![atom("dev-libs/Z")];
    assert!(best_match_to_list(&cand, &list).is_none());
}

#[test]
fn cpv_equal_no_version_both_sides() {
    // match Equal with a candidate that has no version vs an atom cpv that is a
    // bare cp -> the (None,None) arm (matching.rs 214).
    let pool = [Candidate::new("cat/pkg")];
    // `=cat/pkg` is invalid (operator needs version), so use the matcher via a
    // candidate equality through a bare-cp atom which uses None operator.
    assert_eq!(match_from_list(&atom("cat/pkg"), &pool).len(), 1);
}

#[test]
fn depgraph_satisfies_other_choice_overlap() {
    // Overlapping || choices where the shared provider satisfies the *other*
    // choice (depgraph satisfies_other_choice true path).
    let mut av = PackageDb::new();
    av.insert("p/main-1", {
        let mut m = meta(&["x86"]);
        m.deps.insert(
            "RDEPEND".into(),
            "|| ( p/a p/shared ) || ( p/shared p/b )".into(),
        );
        m
    });
    av.insert("p/a-1", meta(&["x86"]));
    av.insert("p/b-1", meta(&["x86"]));
    av.insert("p/shared-1", meta(&["x86"]));
    let outcome =
        Resolver::new(&av, &PackageDb::new(), ResolveParams::default()).resolve(&["p/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // shared satisfies both choices -> chosen once, a and b not pulled in.
    assert!(outcome.mergelist.contains(&"p/shared-1".to_string()));
    assert!(!outcome.mergelist.contains(&"p/a-1".to_string()));
    assert!(!outcome.mergelist.contains(&"p/b-1".to_string()));
}
