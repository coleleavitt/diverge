//! Targeted branch coverage for atom rendering, version/util/matching/config/
//! profile helpers, and depgraph edge arms.

use std::collections::HashMap;

use diverge::atom::{Atom, AtomParseOptions};
use diverge::config::{getconfig, varexpand};
use diverge::matching::{Candidate, match_from_list};
use diverge::util::normalize_path;
use diverge::version::vercmp;

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn atom(s: &str) -> Atom {
    Atom::parse_with_options(s, WILD).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn atom_display_weak_blocker_and_slot_operator() {
    // Weak blocker prefix (line 259).
    let a = atom("!dev-libs/A");
    assert_eq!(a.to_string(), "!dev-libs/A");
    // Strong blocker + slot-operator + repo round-trips.
    let a = atom("!!=dev-libs/A-1:2/3=::r");
    let rendered = a.to_string();
    assert!(rendered.starts_with("!!="));
    assert!(rendered.contains(":2/3"));
    assert!(rendered.contains("::r"));
    // EqualGlob renders with trailing `*`.
    let a = atom("=dev-libs/A-1*");
    assert!(a.to_string().ends_with('*'));
}

#[test]
fn atom_use_dep_minus_prefix_negates() {
    // The `-flag` form (line 205) marks the token negated.
    let a = atom("dev-libs/A[-foo,bar]");
    let parsed = a.parsed_use_deps().unwrap();
    let foo = parsed.tokens.iter().find(|t| t.name == "foo").unwrap();
    assert!(foo.negated);
    let bar = parsed.tokens.iter().find(|t| t.name == "bar").unwrap();
    assert!(!bar.negated);
    // `=` and `?` conditional suffixes are stripped.
    let a = atom("dev-libs/A[foo=,bar?]");
    let parsed = a.parsed_use_deps().unwrap();
    let names: Vec<&str> = parsed.tokens.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"foo") && names.contains(&"bar"));
}

#[test]
fn atom_intersects_version_and_slot_branches() {
    // Same cp, different explicit versions -> no intersect (line 242-244).
    let a = atom("=dev-libs/A-1");
    let b = atom("=dev-libs/A-2");
    assert!(!a.intersects(&b));
    // Same cp, different slots -> no intersect.
    let a = atom("dev-libs/A:1");
    let b = atom("dev-libs/A:2");
    assert!(!a.intersects(&b));
    // Same cp, same slot -> intersect.
    let a = atom("dev-libs/A:1");
    let b = atom("dev-libs/A:1");
    assert!(a.intersects(&b));
}

#[test]
fn normalize_path_relative_dotdot_branches() {
    // Relative path: leading `..` is preserved (apply_parent_component else-if).
    assert_eq!(normalize_path("../a"), "../a");
    assert_eq!(normalize_path("../../a"), "../../a");
    // `..` cancels a prior component.
    assert_eq!(normalize_path("a/../b"), "b");
    // Absolute `..` at root is dropped.
    assert_eq!(normalize_path("/../a"), "/a");
    // All-dots collapse to ".".
    assert_eq!(normalize_path("a/.."), ".");
}

#[test]
fn vercmp_partial_ord_via_sort() {
    // Drives Version::partial_cmp through comparisons.
    assert!(vercmp("1.0", "2.0").is_lt());
    assert!(vercmp("2.0", "1.0").is_gt());
    assert!(vercmp("1.0", "1.0").is_eq());
}

#[test]
fn matching_glob_boundary_and_ordering() {
    // glob boundary: =A-1* must not match A-10 (digit-boundary branch).
    let pool = [
        Candidate::new("dev-libs/A-10"),
        Candidate::new("dev-libs/A-1.2"),
    ];
    let got: Vec<&str> = match_from_list(&atom("=dev-libs/A-1*"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert_eq!(got, vec!["dev-libs/A-1.2"]);
    // leading-zero normalization: =A-01* matches A-1.x.
    let pool = [Candidate::new("dev-libs/A-1.5")];
    assert_eq!(match_from_list(&atom("=dev-libs/A-01*"), &pool).len(), 1);
    // tilde operator strips revision (version_only).
    let pool = [Candidate::new("dev-libs/A-1.2-r3")];
    assert_eq!(match_from_list(&atom("~dev-libs/A-1.2"), &pool).len(), 1);
    // ordering operators.
    let pool = [Candidate::new("dev-libs/A-2")];
    assert_eq!(match_from_list(&atom(">dev-libs/A-1"), &pool).len(), 1);
    assert!(match_from_list(&atom("<dev-libs/A-1"), &pool).is_empty());
    assert_eq!(match_from_list(&atom("<=dev-libs/A-2"), &pool).len(), 1);
}

#[test]
fn matching_use_disabled_constraints() {
    // [-foo] on a pkg that has foo enabled -> no match (disabled_constraints).
    let cand = Candidate::new("dev-libs/A-1")
        .with_iuse(["foo"])
        .with_use(["foo"]);
    let pool = [cand];
    assert!(match_from_list(&atom("dev-libs/A[-foo]"), &pool).is_empty());
    // [-foo] on a pkg with foo declared but disabled -> match.
    let cand = Candidate::new("dev-libs/B-1").with_iuse(["foo"]);
    let pool = [cand];
    assert_eq!(match_from_list(&atom("dev-libs/B[-foo]"), &pool).len(), 1);
}

#[test]
fn getconfig_source_and_braces() {
    let empty = HashMap::new();
    // ${VAR} brace expansion (config line 133-139 path).
    let mut seed = HashMap::new();
    seed.insert("BASE".to_string(), "/usr".to_string());
    let c = getconfig("LIB=\"${BASE}/lib\"\n", true, &seed).unwrap();
    assert_eq!(c.get("LIB").map(String::as_str), Some("/usr/lib"));
    // Bad substitution `${` with nothing after -> still parses (varexpand path).
    assert_eq!(varexpand("${", &empty), "");
    // export with no expand.
    let c = getconfig("export X=\"1\"\n", false, &empty).unwrap();
    assert_eq!(c.get("X").map(String::as_str), Some("1"));
}
