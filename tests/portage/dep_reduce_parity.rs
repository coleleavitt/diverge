//! Ported from research/portage/lib/portage/tests/dep/test_paren_reduce.py,
//! test_use_reduce.py (representative subset), and
//! test_check_required_use.py.

use diverge::dep::{Dep, UseReduceOptions, check_required_use, paren_reduce, use_reduce};

fn token(value: &str) -> Dep {
    Dep::Token(value.to_string())
}

fn group(items: Vec<Dep>) -> Dep {
    Dep::Group(items)
}

#[test]
fn paren_reduce_matches_portage_cases() {
    assert_eq!(paren_reduce("A").unwrap(), vec![token("A")]);
    assert_eq!(paren_reduce("( A )").unwrap(), vec![token("A")]);
    assert_eq!(
        paren_reduce("|| ( A B )").unwrap(),
        vec![token("||"), group(vec![token("A"), token("B")])],
    );
    assert_eq!(
        paren_reduce("|| ( A || ( B C ) )").unwrap(),
        vec![
            token("||"),
            group(vec![
                token("A"),
                token("||"),
                group(vec![token("B"), token("C")]),
            ]),
        ],
    );
    assert_eq!(
        paren_reduce("a? ( A )").unwrap(),
        vec![token("a?"), group(vec![token("A")])]
    );
    assert_eq!(
        paren_reduce("( || ( ( ( A ) B ) ) )").unwrap(),
        vec![token("A"), token("B")]
    );
    assert_eq!(
        paren_reduce("( || ( || ( ( A ) B ) ) )").unwrap(),
        vec![token("||"), group(vec![token("A"), token("B")])],
    );
    assert_eq!(paren_reduce("|| ( A )").unwrap(), vec![token("A")]);
    assert_eq!(
        paren_reduce("A || ( ) foo? ( ) B").unwrap(),
        vec![token("A"), token("B")]
    );
    assert_eq!(
        paren_reduce("|| ( A ) || ( B )").unwrap(),
        vec![token("A"), token("B")]
    );
    assert_eq!(
        paren_reduce("foo? ( A ) foo? ( B )").unwrap(),
        vec![
            token("foo?"),
            group(vec![token("A")]),
            token("foo?"),
            group(vec![token("B")]),
        ],
    );
    assert_eq!(
        paren_reduce("|| ( ( A B ) C )").unwrap(),
        vec![
            token("||"),
            group(vec![group(vec![token("A"), token("B")]), token("C")])
        ],
    );
    assert_eq!(
        paren_reduce("|| ( ( A B ) ( C ) )").unwrap(),
        vec![
            token("||"),
            group(vec![group(vec![token("A"), token("B")]), token("C")])
        ],
    );
    assert_eq!(
        paren_reduce(">=dev-lang/php-5.2[pcre(+)]").unwrap(),
        vec![token(">=dev-lang/php-5.2[pcre(+)]")],
    );
}

#[test]
fn paren_reduce_rejects_malformed_cases() {
    for bad in [
        "( A",
        "A )",
        "||( A B )",
        "|| (A B )",
        "|| ( A B)",
        "|| ( A B",
        "|| A B )",
        "|| A B",
        "|| ( A B ) )",
        "|| || B C",
        "|| ( A B || )",
        "a? A",
    ] {
        assert!(paren_reduce(bad).is_err(), "{bad} should be rejected");
    }
}

#[test]
fn use_reduce_evaluates_conditionals() {
    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            uselist: &["a", "b", "c", "d"],
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("A"), token("B")]);

    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            uselist: &["a", "b", "c"],
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("A"), token("B"), token("D")]);

    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            matchall: true,
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("A"), token("B"), token("C"), token("D")]);

    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            masklist: &["a", "c"],
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("C"), token("D")]);

    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            excludeall: &["a", "c"],
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("D")]);
}

#[test]
fn use_reduce_subset_selection() {
    let result = use_reduce(
        "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )",
        &UseReduceOptions {
            uselist: &["a", "b", "c", "d"],
            subset: Some(&["b"]),
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("B")]);

    let result = use_reduce(
        "|| ( foo bar? ( baz ) )",
        &UseReduceOptions {
            uselist: &["bar"],
            subset: Some(&["bar"]),
            ..UseReduceOptions::default()
        },
    )
    .unwrap();
    assert_eq!(result, vec![token("baz")]);
}

#[test]
fn use_reduce_rejects_malformed_cases() {
    for bad in [
        "? ( A )",
        "!? ( A )",
        "( A",
        "A )",
        "||( A B )",
        "|| (A B )",
        "|| ( A B)",
        "|| ( A B",
        "|| A B )",
        "|| A B",
        "|| ( A B ) )",
        "|| || B C",
        "|| ( A B || )",
        "a? A",
        "foo?",
        "1.0? ( A )",
    ] {
        assert!(
            use_reduce(bad, &UseReduceOptions::default()).is_err(),
            "{bad} should be rejected",
        );
    }
}

#[test]
fn check_required_use_matches_portage_cases() {
    let iuse = ["a", "b", "c", "d"];
    let matcher = |flag: &str| iuse.contains(&flag);

    let cases: &[(&str, &[&str], bool)] = &[
        ("|| ( a b )", &[], false),
        ("|| ( a b )", &["a"], true),
        ("^^ ( a b )", &["a", "b"], false),
        ("?? ( a b )", &["a", "b"], false),
        ("?? ( a b )", &["a"], true),
        ("?? ( )", &[], true),
        ("^^ ( || ( a b ) c )", &[], false),
        ("^^ ( || ( a b ) c )", &["a"], true),
        ("a? ( ^^ ( b c ) )", &["a"], false),
        ("a? ( ^^ ( b c ) )", &["a", "b"], true),
        ("^^ ( a? ( !b ) !c? ( d ) )", &[], false),
        ("^^ ( a? ( !b ) !c? ( d ) )", &["a"], true),
        ("^^ ( a? ( !b ) !c? ( d ) )", &["c"], false),
        ("|| ( ^^ ( a b ) ^^ ( b c ) )", &["a", "b", "c"], false),
        ("^^ ( || ( a b ) ^^ ( b c ) )", &["b"], false),
        ("|| ( ( a b ) c )", &["a"], false),
        ("|| ( ( a b ) c )", &["a", "b"], true),
        ("^^ ( ( a b ) c )", &["a", "b", "c"], false),
        ("^^ ( ( a b ) c )", &["c"], true),
    ];

    for (required_use, use_, expected) in cases {
        assert_eq!(
            check_required_use(required_use, use_, matcher, None).unwrap(),
            *expected,
            "REQUIRED_USE={required_use}, USE={use_:?}",
        );
    }
}

#[test]
fn check_required_use_rejects_malformed_and_unknown_flags() {
    let iuse = ["a", "b"];
    let matcher = |flag: &str| iuse.contains(&flag);

    // malformed structure
    assert!(check_required_use("^^ ( || ( a b ) ^^ ( b c )", &[], |_| true, None).is_err());
    assert!(check_required_use("^^( || ( a b ) )", &[], |_| true, None).is_err());
    // unknown IUSE flag c
    assert!(check_required_use("^^ ( || ( a b ) ^^ ( b c ) )", &[], matcher, None).is_err());
    // '??' rejected under EAPI 4 (iuse matcher rejects the '?' pseudo-flag)
    assert!(check_required_use("?? ( a b )", &[], matcher, Some("4")).is_err());
}
