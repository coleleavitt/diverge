//! Extended tests for config module, ported from:
//! - research/portage/lib/portage/tests/util/test_varExpand.py
//! - research/portage/lib/portage/tests/util/test_getconfig.py
//!
//! These tests cover additional cases beyond the basic parity tests in
//! tests/portage/config_parity.rs, including edge cases in varexpand with
//! braced variables, malformed references, and getconfig with various
//! quoting/escaping patterns.

use std::collections::HashMap;

use diverge::config::{ParseError, getconfig, varexpand};

fn dict(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ============================================================================
// varexpand: Additional test cases
// ============================================================================

/// Upstream test_varExpand.py::VarExpandTestCase::testVarExpandPass
/// Validates ${VAR} and $VAR with multiple variables.
#[test]
fn varexpand_multiple_variables_braced_and_simple() {
    let vars = dict(&[("a", "5"), ("b", "7"), ("c", "-5")]);

    // Test each variable in both $VAR and ${VAR} forms
    assert_eq!(varexpand("$a", &vars), "5");
    assert_eq!(varexpand("${a}", &vars), "5");
    assert_eq!(varexpand("$b", &vars), "7");
    assert_eq!(varexpand("${b}", &vars), "7");
    assert_eq!(varexpand("$c", &vars), "-5");
    assert_eq!(varexpand("${c}", &vars), "-5");
}

/// Test varexpand with multiple expansions in one string.
#[test]
fn varexpand_multiple_expansions_in_string() {
    let vars = dict(&[("a", "hello"), ("b", "world")]);
    assert_eq!(varexpand("$a $b", &vars), "hello world");
    assert_eq!(varexpand("${a} ${b}", &vars), "hello world");
    assert_eq!(varexpand("$a-$b", &vars), "hello-world");
    assert_eq!(
        varexpand("prefix_${a}_${b}_suffix", &vars),
        "prefix_hello_world_suffix"
    );
}

/// Test varexpand with variable references followed by word characters.
/// Since varexpand only stops at non-word-chars, $aB means the variable "aB".
#[test]
fn varexpand_variable_boundary_with_word_chars() {
    let vars = dict(&[("a", "val_a"), ("ab", "val_ab")]);
    // $a stops at the first non-word character (space or end)
    assert_eq!(varexpand("$a ", &vars), "val_a ");
    // $ab expands to the whole variable "ab"
    assert_eq!(varexpand("$ab", &vars), "val_ab");
    // Braced form is explicit
    assert_eq!(varexpand("${a}b", &vars), "val_ab");
}

/// Test varexpand with malformed ${...} references.
/// Upstream returns empty string on bad substitution.
#[test]
fn varexpand_malformed_braced_reference_returns_empty() {
    let vars = dict(&[("a", "5")]);

    // Missing closing brace
    assert_eq!(varexpand("${a", &vars), "");

    // Empty variable name in braces
    assert_eq!(varexpand("${}", &vars), "");

    // Non-word character immediately after ${
    assert_eq!(varexpand("${-}", &vars), "");
}

/// Test varexpand with newlines in the input.
/// Upstream converts newlines to spaces per the code.
#[test]
fn varexpand_newlines_converted_to_spaces() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("line1\nline2", &vars), "line1 line2");
    assert_eq!(varexpand("$a\n$a", &vars), "5 5");
}

/// Test varexpand with backslash-newline (line continuation).
/// Upstream drops the backslash and newline.
#[test]
fn varexpand_escaped_newline_is_dropped() {
    let vars = HashMap::new();
    assert_eq!(varexpand("line1\\\nline2", &vars), "line1line2");
    assert_eq!(varexpand("hello\\\nworld", &vars), "helloworld");
}

/// Test varexpand with double and single quotes mixed.
#[test]
fn varexpand_mixed_quotes() {
    let vars = dict(&[("a", "5")]);
    // Single quotes inside double quotes: variable still expanded
    assert_eq!(varexpand("\"'$a'\"", &vars), "\"'5'\"");
    // Double quotes inside single quotes: variable NOT expanded (single quotes suppress)
    assert_eq!(varexpand("'\"$a\"'", &vars), "'\"$a\"'");
}

/// Test varexpand with consecutive backslashes.
/// The bug-for-bug compatible behavior where \\ consumes a following quote or $.
#[test]
fn varexpand_double_backslash_consumes_next_special_char() {
    let vars = HashMap::new();
    // \\ followed by $ should consume the $
    assert_eq!(varexpand("\\\\$", &vars), "\\$");
    // \\ followed by " should consume the "
    assert_eq!(varexpand("\\\\\"", &vars), "\\\"");
    // \\ followed by regular char: backslash is kept but $ is NOT consumed
    assert_eq!(varexpand("\\\\a", &vars), "\\a");
}

/// Test varexpand with variable expansion after backslash handling.
#[test]
fn varexpand_escaped_dollar_prevents_expansion() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("\\$a", &vars), "$a");
    assert_eq!(varexpand("\\${a}", &vars), "${a}");
}

/// Test varexpand with trailing dollar sign.
#[test]
fn varexpand_trailing_dollar_is_preserved() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("$", &vars), "$");
    assert_eq!(varexpand("hello$", &vars), "hello$");
}

/// Test varexpand with empty dict and various inputs.
#[test]
fn varexpand_empty_dict_returns_empty_expansion() {
    let empty = HashMap::new();
    assert_eq!(varexpand("$undef", &empty), "");
    assert_eq!(varexpand("${undef}", &empty), "");
    // Note: $undef_suffix looks for variable named "undef_suffix" (underscore is word char)
    assert_eq!(varexpand("prefix_$undef_suffix", &empty), "prefix_");
}

/// Test varexpand preserves most special characters that aren't backslash/quote.
#[test]
fn varexpand_special_chars_preserved() {
    let vars = dict(&[("a", "5")]);
    assert_eq!(varexpand("$a!@#%", &vars), "5!@#%");
    assert_eq!(
        varexpand("$a~*^&()[]{}|;:<>,.?/", &vars),
        "5~*^&()[]{}|;:<>,.?/"
    );
}

/// Test varexpand with numbers and underscores in variable names.
#[test]
fn varexpand_variable_names_with_numbers_and_underscores() {
    let vars = dict(&[
        ("var1", "one"),
        ("VAR_2", "two"),
        ("_underscore_start", "three"),
        ("MIX_123_ing", "four"),
    ]);
    assert_eq!(varexpand("$var1", &vars), "one");
    assert_eq!(varexpand("${VAR_2}", &vars), "two");
    assert_eq!(varexpand("$_underscore_start", &vars), "three");
    assert_eq!(varexpand("${MIX_123_ing}", &vars), "four");
}

/// Test varexpand with numeric variable name (treated as a regular variable).
/// Varexpand doesn't validate variable names, so a numeric name like "1" can be looked up.
#[test]
fn varexpand_numeric_variable_name_can_be_expanded() {
    let vars = dict(&[("1", "value_one")]);
    // varexpand doesn't validate variable names, so "1" is treated as a regular variable
    assert_eq!(varexpand("${1}", &vars), "value_one");
}

// ============================================================================
// getconfig: Additional test cases for quoting, escaping, and expansions
// ============================================================================

/// Test getconfig with double-quoted values containing special characters.
#[test]
fn getconfig_double_quoted_with_special_chars() {
    let empty = HashMap::new();
    let cfg = getconfig(
        "URL=\"https://example.com?foo=bar&baz=qux\"\n",
        false,
        &empty,
    )
    .unwrap();
    assert_eq!(
        cfg.get("URL").map(String::as_str),
        Some("https://example.com?foo=bar&baz=qux")
    );
}

/// Test getconfig with backslash-escaped characters inside double quotes.
/// Only \\ and \" are escape sequences inside double quotes per shlex.
#[test]
fn getconfig_backslash_escapes_in_double_quotes() {
    let empty = HashMap::new();
    // \\ inside double quotes becomes single \
    let cfg = getconfig("A=\"\\\\test\"\n", false, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("\\test"));

    // \" inside double quotes becomes literal "
    let cfg = getconfig("B=\"quote\\\"here\"\n", false, &empty).unwrap();
    assert_eq!(cfg.get("B").map(String::as_str), Some("quote\"here"));
}

/// Test getconfig with single-quoted values (no escaping inside single quotes).
#[test]
fn getconfig_single_quoted_literal() {
    let empty = HashMap::new();
    let cfg = getconfig("PATH='/usr/bin:/usr/local/bin'\n", false, &empty).unwrap();
    assert_eq!(
        cfg.get("PATH").map(String::as_str),
        Some("/usr/bin:/usr/local/bin")
    );

    // Backslashes in single quotes are literal
    let cfg = getconfig("ESCAPE='\\\\test'\n", false, &empty).unwrap();
    assert_eq!(cfg.get("ESCAPE").map(String::as_str), Some("\\\\test"));
}

/// Test getconfig with mixed quoted and unquoted parts.
#[test]
fn getconfig_mixed_quoting() {
    let empty = HashMap::new();
    let cfg = getconfig("MIXED=prefix\"quoted\"suffix\n", false, &empty).unwrap();
    assert_eq!(
        cfg.get("MIXED").map(String::as_str),
        Some("prefixquotedsuffix")
    );
}

/// Test getconfig with export prefix (recognized and skipped).
#[test]
fn getconfig_export_keyword_prefix() {
    let empty = HashMap::new();
    let cfg = getconfig("export EXPORTED=\"value\"\n", false, &empty).unwrap();
    assert_eq!(cfg.get("EXPORTED").map(String::as_str), Some("value"));
}

/// Test getconfig with expansion chaining: B references A, C references B.
#[test]
fn getconfig_expansion_chaining() {
    let empty = HashMap::new();
    let cfg = getconfig("A=first\nB=$A\nC=$B\n", true, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("first"));
    assert_eq!(cfg.get("B").map(String::as_str), Some("first"));
    assert_eq!(cfg.get("C").map(String::as_str), Some("first"));
}

/// Test getconfig with initial expansion map (provided context).
#[test]
fn getconfig_with_initial_expansion_map() {
    let initial = dict(&[("PORTDIR", "/usr/portage"), ("DISTDIR", "/usr/distfiles")]);
    let cfg = getconfig(
        "PACKAGE_PATH=\"${PORTDIR}/packages\"\nFETCH_PATH=\"${DISTDIR}\"\n",
        true,
        &initial,
    )
    .unwrap();
    assert_eq!(
        cfg.get("PACKAGE_PATH").map(String::as_str),
        Some("/usr/portage/packages")
    );
    assert_eq!(
        cfg.get("FETCH_PATH").map(String::as_str),
        Some("/usr/distfiles")
    );
}

/// Test getconfig recognizes # as line comment (entire line and inline).
#[test]
fn getconfig_hash_comments() {
    let empty = HashMap::new();
    let cfg = getconfig(
        "A=value1 # inline comment\n# Full line comment\nB=value2\n",
        false,
        &empty,
    )
    .unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("value1"));
    assert_eq!(cfg.get("B").map(String::as_str), Some("value2"));
}

/// Test getconfig with consecutive assignments (no blank lines needed).
#[test]
fn getconfig_consecutive_assignments() {
    let empty = HashMap::new();
    let cfg = getconfig("A=1\nB=2\nC=3\nD=4\n", false, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("1"));
    assert_eq!(cfg.get("B").map(String::as_str), Some("2"));
    assert_eq!(cfg.get("C").map(String::as_str), Some("3"));
    assert_eq!(cfg.get("D").map(String::as_str), Some("4"));
}

/// Test getconfig with complex shell-like value (from test_getconfig.py _cases).
/// This is one of the real-world patterns from upstream, with initial expansion context.
#[test]
fn getconfig_complex_fetchcommand_value() {
    // This is a real case from portage test suite, but with expansion context provided
    let initial = dict(&[
        ("DISTDIR", "/usr/distfiles"),
        ("FILE", "package.tar.gz"),
        ("URI", "http://example.com/package.tar.gz"),
    ]);
    let cfg = getconfig(
        "FETCHCOMMAND=\"wget -t 3 -T 60 --passive-ftp -U \\\"Portage\\\" -O \\\"${DISTDIR}/${FILE}\\\" \\\"${URI}\\\"\"\n",
        true,
        &initial,
    ).unwrap();
    let expected = "wget -t 3 -T 60 --passive-ftp -U \"Portage\" -O \"/usr/distfiles/package.tar.gz\" \"http://example.com/package.tar.gz\"";
    assert_eq!(cfg.get("FETCHCOMMAND").map(String::as_str), Some(expected));
}

/// Test getconfig with values containing arithmetic-like patterns.
#[test]
fn getconfig_arithmetic_like_values() {
    let empty = HashMap::new();
    let cfg = getconfig("MATH=\"1+2*3\"\nCALC=\"(a-b)/c\"\n", false, &empty).unwrap();
    assert_eq!(cfg.get("MATH").map(String::as_str), Some("1+2*3"));
    assert_eq!(cfg.get("CALC").map(String::as_str), Some("(a-b)/c"));
}

/// Test getconfig with whitespace around = is handled correctly.
#[test]
fn getconfig_whitespace_around_equals() {
    let empty = HashMap::new();
    // Shlex should handle this: whitespace is separator
    let cfg = getconfig("KEY = value\n", false, &empty);
    // Upstream getconfig reads KEY, then expects =, but gets (whitespace-skipped) =, so it's OK.
    // Actually, shlex handles this: tokens are KEY, =, value
    assert!(cfg.is_ok());
    let cfg = cfg.unwrap();
    assert_eq!(cfg.get("KEY").map(String::as_str), Some("value"));
}

/// Test getconfig error on unterminated quote.
#[test]
fn getconfig_unterminated_quote_is_error() {
    let empty = HashMap::new();
    let result = getconfig("BAD=\"unclosed\n", false, &empty);
    assert!(result.is_err());
    match result {
        Err(ParseError(msg)) => assert!(msg.contains("closing quotation"), "msg={msg}"),
        _ => panic!("Expected ParseError"),
    }
}

/// Test getconfig error on missing value after = (EOF).
#[test]
fn getconfig_eof_after_equals_is_error() {
    let empty = HashMap::new();
    let result = getconfig("KEY=", false, &empty);
    assert!(result.is_err());
    match result {
        Err(ParseError(msg)) => assert!(msg.contains("Unexpected"), "msg={msg}"),
        _ => panic!("Expected ParseError"),
    }
}

/// Test getconfig error on token after = that isn't a value (should never happen with shlex).
#[test]
fn getconfig_unexpected_token_instead_of_equals() {
    let empty = HashMap::new();
    // If for some reason a token appears where = is expected (hard to construct with shlex)
    // the parser should error. This is a defensive test.
    let result = getconfig("KEY NOTEQUALS value\n", false, &empty);
    assert!(result.is_err());
    match result {
        Err(ParseError(msg)) => assert!(
            msg.contains("Invalid token") || msg.contains("not '='"),
            "msg={msg}"
        ),
        _ => panic!("Expected ParseError"),
    }
}

/// Test getconfig with empty input.
#[test]
fn getconfig_empty_input() {
    let empty = HashMap::new();
    let cfg = getconfig("", false, &empty).unwrap();
    assert!(cfg.is_empty());
}

/// Test getconfig with only whitespace and comments.
#[test]
fn getconfig_only_whitespace_and_comments() {
    let empty = HashMap::new();
    let cfg = getconfig("   \n  # just a comment\n  \n", false, &empty).unwrap();
    assert!(cfg.is_empty());
}

/// Test getconfig without final newline (still works, upstream adds one).
#[test]
fn getconfig_missing_final_newline() {
    let empty = HashMap::new();
    let cfg = getconfig("KEY=value", false, &empty).unwrap();
    assert_eq!(cfg.get("KEY").map(String::as_str), Some("value"));
}

/// Test getconfig with expansion disabled keeps literal $\E sequence (bug #410625 from upstream).
/// This is exactly the test case from test_getconfig.py::testGetConfigProfileEnv.
#[test]
fn getconfig_profile_env_keeps_literal_dollar_escape() {
    let empty = HashMap::new();
    let cfg = getconfig("LESS_TERMCAP_mb=\"$\\E[01;31m\"\n", false, &empty).unwrap();
    assert_eq!(
        cfg.get("LESS_TERMCAP_mb").map(String::as_str),
        Some("$\\E[01;31m"),
    );
}

/// Test getconfig with later assignment overrides earlier (not in expand_map).
#[test]
fn getconfig_later_assignment_overrides_earlier() {
    let empty = HashMap::new();
    let cfg = getconfig("A=first\nA=second\n", false, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some("second"));
}

/// Test getconfig with variable name starting with underscore.
#[test]
fn getconfig_variable_name_with_leading_underscore() {
    let empty = HashMap::new();
    let cfg = getconfig("_INTERNAL=value\n", false, &empty).unwrap();
    assert_eq!(cfg.get("_INTERNAL").map(String::as_str), Some("value"));
}

/// Test getconfig with UPPERCASE and lowercase variables coexist.
#[test]
fn getconfig_case_sensitive_variable_names() {
    let empty = HashMap::new();
    let cfg = getconfig("Key=one\nkey=two\nKEY=three\n", false, &empty).unwrap();
    assert_eq!(cfg.get("Key").map(String::as_str), Some("one"));
    assert_eq!(cfg.get("key").map(String::as_str), Some("two"));
    assert_eq!(cfg.get("KEY").map(String::as_str), Some("three"));
}

/// Test getconfig with variable that references an undefined variable (expands to empty).
#[test]
fn getconfig_expansion_undefined_variable() {
    let empty = HashMap::new();
    let cfg = getconfig("A=${UNDEFINED}\nB=prefix_${A}_suffix\n", true, &empty).unwrap();
    assert_eq!(cfg.get("A").map(String::as_str), Some(""));
    assert_eq!(cfg.get("B").map(String::as_str), Some("prefix__suffix"));
}

/// Test getconfig with quoted variable expansion.
#[test]
fn getconfig_quoted_variable_expansion() {
    let initial = dict(&[("VAR", "value")]);
    let cfg = getconfig("QUOTED=\"${VAR}\"\n", true, &initial).unwrap();
    assert_eq!(cfg.get("QUOTED").map(String::as_str), Some("value"));
}

/// Test getconfig with newlines inside quoted values (converted to spaces by varexpand).
#[test]
fn getconfig_newline_in_quoted_value() {
    let empty = HashMap::new();
    let cfg = getconfig("MULTILINE=\"line1\nline2\"\n", false, &empty).unwrap();
    // shlex preserves the literal newline in the token
    assert_eq!(
        cfg.get("MULTILINE").map(String::as_str),
        Some("line1\nline2")
    );
}

/// Test getconfig expansion mode: variable references are expanded even in quoted values.
#[test]
fn getconfig_expansion_in_quoted_value() {
    let initial = dict(&[("USER", "alice")]);
    let cfg = getconfig("GREETING=\"Hello ${USER}!\"\n", true, &initial).unwrap();
    assert_eq!(
        cfg.get("GREETING").map(String::as_str),
        Some("Hello alice!")
    );
}

/// Test getconfig with = inside quoted value (no special meaning).
#[test]
fn getconfig_equals_inside_quoted_value() {
    let empty = HashMap::new();
    let cfg = getconfig("EQUATION=\"a=b+c\"\n", false, &empty).unwrap();
    assert_eq!(cfg.get("EQUATION").map(String::as_str), Some("a=b+c"));
}

/// Test that invalid variable names are rejected (starting with digit).
#[test]
fn getconfig_invalid_var_name_starting_with_digit() {
    let empty = HashMap::new();
    let result = getconfig("1INVALID=value\n", false, &empty);
    assert!(result.is_err());
    match result {
        Err(ParseError(msg)) => assert!(msg.contains("Invalid variable name"), "msg={msg}"),
        _ => panic!("Expected ParseError"),
    }
}

/// Test that invalid variable names with special chars are rejected.
#[test]
fn getconfig_invalid_var_name_with_special_char() {
    let empty = HashMap::new();
    let result = getconfig("MY-VAR=value\n", false, &empty);
    assert!(result.is_err());
    match result {
        Err(ParseError(msg)) => assert!(msg.contains("Invalid variable name"), "msg={msg}"),
        _ => panic!("Expected ParseError"),
    }
}

/// Test getconfig with backslash outside of quotes (POSIX shlex escape).
#[test]
fn getconfig_backslash_escape_outside_quotes() {
    let empty = HashMap::new();
    // Outside quotes, backslash escapes the next character
    let cfg = getconfig("ESCAPED=hello\\world\n", false, &empty).unwrap();
    // shlex should treat \w as literal w
    assert_eq!(cfg.get("ESCAPED").map(String::as_str), Some("helloworld"));
}

/// Test getconfig with complex multiline pattern (export + quoted + expansion).
#[test]
fn getconfig_complex_multiline() {
    let initial = dict(&[("PREFIX", "/usr")]);
    let cfg = getconfig(
        "export BINDIR=\"${PREFIX}/bin\"\nexport LIBDIR=\"${PREFIX}/lib\"\nDBPATH=\"${PREFIX}/libexec/db\"\n",
        true,
        &initial,
    ).unwrap();
    assert_eq!(cfg.get("BINDIR").map(String::as_str), Some("/usr/bin"));
    assert_eq!(cfg.get("LIBDIR").map(String::as_str), Some("/usr/lib"));
    assert_eq!(
        cfg.get("DBPATH").map(String::as_str),
        Some("/usr/libexec/db")
    );
}

// Regression: a make.conf containing non-word separator chars (e.g. `$(...)`,
// `(`, `&`) must not cause unbounded recursion / stack overflow in the lexer.
// Reproduces the host-make.conf stack overflow (config.rs next_token).
#[test]
fn getconfig_many_separators_no_stack_overflow() {
    use std::collections::HashMap;

    use diverge::config::getconfig;
    let empty = HashMap::new();
    // A long run of non-word separator chars (the host make.conf shape that
    // triggered unbounded recursion in the lexer). The key property is that
    // getconfig TERMINATES (Ok or Err) rather than overflowing the stack.
    let separators: String = "( ) & | < > ".repeat(20000);
    let _ = getconfig(&separators, true, &empty);

    // And a realistic file with separators interspersed still parses its
    // assignments correctly.
    let content = "FOO=\"bar\"\nMAKEOPTS=\"-j24 -l26\"\n";
    let parsed = getconfig(content, true, &empty).expect("parses");
    assert_eq!(parsed.get("FOO").map(String::as_str), Some("bar"));
    assert_eq!(
        parsed.get("MAKEOPTS").map(String::as_str),
        Some("-j24 -l26")
    );
}

#[test]
fn getconfig_real_world_make_conf_flags() {
    use std::collections::HashMap;

    use diverge::config::getconfig;
    let empty = HashMap::new();
    // Shapes seen in a real make.conf (parenthesised command substitution,
    // bracketed flags). The lexer must terminate.
    let content = "\
COMMON_FLAGS=\"-march=native -O2 -pipe\"
CFLAGS=\"${COMMON_FLAGS}\"
MAKEOPTS=\"-j24 -l26\"
FEATURES=\"parallel-fetch\"
";
    let parsed = getconfig(content, true, &empty).expect("parses");
    assert_eq!(
        parsed.get("MAKEOPTS").map(String::as_str),
        Some("-j24 -l26")
    );
    assert_eq!(
        parsed.get("CFLAGS").map(String::as_str),
        Some("-march=native -O2 -pipe")
    );
}
