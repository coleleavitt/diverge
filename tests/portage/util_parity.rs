//! Ports of upstream `portage.util` config-file primitives.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/util/test_normalizedPath.py`
//! - `research/portage/lib/portage/tests/util/test_stackLists.py`
//! - `research/portage/lib/portage/tests/util/test_stackDicts.py`

use std::collections::BTreeMap;

use diverge::util::{grabdict, grabfile, normalize_path, stack_dicts, stack_lists};

fn lists(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn dict(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn normalize_path_collapses_leading_slashes() {
    // Ported from test_normalizedPath.py.
    assert_eq!(normalize_path("///foo/bar/baz"), "/foo/bar/baz");
    assert_eq!(normalize_path("/foo/bar/baz"), "/foo/bar/baz");
    assert_eq!(normalize_path("/foo//bar/./baz/"), "/foo/bar/baz");
    assert_eq!(normalize_path("/foo/../bar"), "/bar");
    assert_eq!(normalize_path("foo/bar/../baz"), "foo/baz");
    assert_eq!(normalize_path("//foo/bar"), "//foo/bar");
}

#[test]
fn stack_lists_matches_portage() {
    // Ported from test_stackLists.py (order-insensitive via sorted compare).
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    assert_eq!(
        sorted(stack_lists(
            &lists(&[&["a", "b", "c"], &["d", "e", "f"]]),
            true
        )),
        sorted(vec![
            "a".into(),
            "c".into(),
            "b".into(),
            "e".into(),
            "d".into(),
            "f".into()
        ])
    );
    assert_eq!(
        sorted(stack_lists(&lists(&[&["a", "x"], &["b", "x"]]), true)),
        sorted(vec!["a".into(), "x".into(), "b".into()])
    );
    assert_eq!(
        stack_lists(&lists(&[&["a", "b", "c"], &["-*"]]), true),
        Vec::<String>::new()
    );
    assert_eq!(
        stack_lists(&lists(&[&["a"], &["-a"]]), true),
        Vec::<String>::new()
    );
}

#[test]
fn stack_dicts_matches_portage() {
    // Ported from test_stackDicts.py.
    assert_eq!(
        stack_dicts(
            &[Some(dict(&[("a", "b")])), Some(dict(&[("b", "c")]))],
            false,
            &[],
            false
        ),
        Some(dict(&[("a", "b"), ("b", "c")]))
    );
    assert_eq!(
        stack_dicts(
            &[Some(dict(&[("a", "b")])), Some(dict(&[("a", "c")]))],
            true,
            &[],
            false
        ),
        Some(dict(&[("a", "b c")]))
    );
    assert_eq!(
        stack_dicts(
            &[Some(dict(&[("a", "b")])), Some(dict(&[("a", "c")]))],
            false,
            &["a"],
            false
        ),
        Some(dict(&[("a", "b c")]))
    );
    // None aborts when ignore_none is false.
    assert_eq!(
        stack_dicts(&[None, Some(dict(&[]))], false, &[], false),
        None
    );
    // None ignored when ignore_none is true.
    assert_eq!(
        stack_dicts(&[Some(dict(&[("a", "b")])), None], false, &[], true),
        Some(dict(&[("a", "b")]))
    );
}

#[test]
fn grabfile_skips_comments_and_blank_lines() {
    let content = "\
# a comment
dev-libs/A
  dev-libs/B   trailing  # inline comment
\t
dev-libs/C
";
    assert_eq!(
        grabfile(content),
        vec![
            "dev-libs/A".to_string(),
            "dev-libs/B trailing".to_string(),
            "dev-libs/C".to_string()
        ]
    );
}

#[test]
fn grabdict_parses_key_values() {
    let content = "\
sys-apps/portage x86 amd64 ppc
# comment
dev-libs/A foo
dev-libs/A bar
";
    let parsed = grabdict(content, true, false);
    assert_eq!(
        parsed.get("sys-apps/portage").map(Vec::as_slice),
        Some(["x86".to_string(), "amd64".to_string(), "ppc".to_string()].as_slice())
    );
    // Incremental: repeated key accumulates.
    assert_eq!(
        parsed.get("dev-libs/A").map(Vec::as_slice),
        Some(["foo".to_string(), "bar".to_string()].as_slice())
    );

    // Non-incremental: last line wins.
    let parsed = grabdict(content, false, false);
    assert_eq!(
        parsed.get("dev-libs/A").map(Vec::as_slice),
        Some(["bar".to_string()].as_slice())
    );
}
