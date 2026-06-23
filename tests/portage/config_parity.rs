//! Ported from research/portage/lib/portage/tests/util/test_varExpand.py.

use std::collections::HashMap;

use diverge::config::varexpand;

fn dict(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn varexpand_pass_simple_and_braced() {
    let vars = dict(&[("a", "5"), ("b", "7"), ("c", "-5")]);
    for (key, expected) in [("a", "5"), ("b", "7"), ("c", "-5")] {
        assert_eq!(varexpand(&format!("${key}"), &vars), expected);
        assert_eq!(varexpand(&format!("${{{key}}}"), &vars), expected);
    }
}

#[test]
fn varexpand_backslashes_match_bash() {
    // These are the exact upstream cases, including the documented
    // bug-for-bug behavior.
    let empty = HashMap::new();
    let tests = [
        ("\\", "\\"),
        ("\\\\", "\\"),
        ("\\\\\\", "\\\\"),
        ("\\\\\\\\", "\\\\"),
        ("\\$", "$"),
        ("\\\\$", "\\$"),
        ("\\a", "\\a"),
        ("\\b", "\\b"),
        ("\\n", "\\n"),
        ("\\r", "\\r"),
        ("\\t", "\\t"),
        ("\\\n", ""),
        ("\\\"", "\\\""),
        ("\\'", "\\'"),
    ];
    for (input, expected) in tests {
        assert_eq!(varexpand(input, &empty), expected, "input={input:?}");
    }
}

#[test]
fn varexpand_double_quotes_preserved() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("\"${a}\"", &vars), "\"5\"");
}

#[test]
fn varexpand_single_quotes_suppress_expansion() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("'${a}'", &vars), "'${a}'");
}

#[test]
fn varexpand_unset_variable_expands_to_empty() {
    let vars = dict(&[("a", "5"), ("b", "7"), ("c", "15")]);
    assert_eq!(varexpand("$fail", &vars), "");
    assert_eq!(varexpand("${fail}", &vars), "");
}
