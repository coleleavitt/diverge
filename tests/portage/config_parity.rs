//! Ported from research/portage/lib/portage/tests/util/test_varExpand.py and
//! test_getconfig.py.

use std::collections::HashMap;

use diverge::config::{getconfig, varexpand};

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

#[test]
fn getconfig_parses_assignments_and_expands() {
    let empty = HashMap::new();
    let cfg = getconfig(
        "FOO=\"hello world\"\nBAR=2\nBAZ=\"${FOO}!\"\n",
        true,
        &empty,
    )
    .unwrap();
    assert_eq!(cfg.get("FOO").map(String::as_str), Some("hello world"));
    assert_eq!(cfg.get("BAR").map(String::as_str), Some("2"));
    assert_eq!(cfg.get("BAZ").map(String::as_str), Some("hello world!"));
}

#[test]
fn getconfig_export_prefix_and_later_assignment_wins() {
    let empty = HashMap::new();
    let cfg = getconfig("export A=1\nA=2\n", true, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("2"));
}

#[test]
fn getconfig_profile_env_mode_no_expansion() {
    // test_getconfig.py::testGetConfigProfileEnv: expand=False keeps `$\E...`.
    let empty = HashMap::new();
    let cfg = getconfig("LESS_TERMCAP_mb=\"$\\E[01;31m\"\n", false, &empty).unwrap();
    assert_eq!(
        cfg.get("LESS_TERMCAP_mb").map(String::as_str),
        Some("$\\E[01;31m"),
    );
}

#[test]
fn getconfig_invalid_var_name_is_parse_error() {
    let empty = HashMap::new();
    assert!(getconfig("1BAD=x\n", true, &empty).is_err());
}

#[test]
fn getconfig_value_less_assignment_is_parse_error() {
    let empty = HashMap::new();
    assert!(getconfig("A=#c\n", true, &empty).is_err());
}
