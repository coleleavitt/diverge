use diverge::atom::{Atom, AtomParseOptions, Blocker, Operator, is_valid_atom};

#[test]
fn atom_parts_match_representative_portage_cases() {
    let cases = [
        (
            "=sys-apps/portage-2.1-r1:0[doc,a=,!b=,c?,!d?,-e]",
            Some(Operator::Equal),
            "sys-apps/portage",
            Some("2.1-r1"),
            Some("0"),
            Some("[doc,a=,!b=,c?,!d?,-e]"),
            None,
            false,
            false,
        ),
        (
            "=sys-apps/portage-2.1-r1*:0[doc]",
            Some(Operator::EqualGlob),
            "sys-apps/portage",
            Some("2.1-r1"),
            Some("0"),
            Some("[doc]"),
            None,
            false,
            false,
        ),
        (
            "sys-apps/portage:0[doc]",
            None,
            "sys-apps/portage",
            None,
            Some("0"),
            Some("[doc]"),
            None,
            false,
            false,
        ),
        ("*/*", None, "*/*", None, None, None, None, true, false),
        (
            "=*/*-*9999*:0::repo_name",
            Some(Operator::EqualGlob),
            "*/*",
            Some("*9999"),
            Some("0"),
            None,
            Some("repo_name"),
            true,
            true,
        ),
        (
            "sys-apps/portage:0::repo_name[doc]",
            None,
            "sys-apps/portage",
            None,
            Some("0"),
            Some("[doc]"),
            Some("repo_name"),
            false,
            true,
        ),
        (
            "dev-libs/A[a(+),b(-)=,!c(+)=,d(-)?,!e(+)?,-f(-)]",
            None,
            "dev-libs/A",
            None,
            None,
            Some("[a(+),b(-)=,!c(+)=,d(-)?,!e(+)?,-f(-)]"),
            None,
            true,
            true,
        ),
    ];

    for (input, operator, cp, version, slot, use_deps, repo, allow_wildcard, allow_repo) in cases {
        let atom = Atom::parse_with_options(
            input,
            AtomParseOptions {
                allow_wildcard,
                allow_repo,
            },
        )
        .unwrap_or_else(|err| panic!("{input} should parse: {err}"));
        assert_eq!(atom.operator, operator, "{input}.operator");
        assert_eq!(atom.cp(), cp, "{input}.cp");
        assert_eq!(atom.version.as_deref(), version, "{input}.version");
        assert_eq!(atom.slot(), slot, "{input}.slot");
        assert_eq!(atom.use_deps.as_deref(), use_deps, "{input}.use");
        assert_eq!(atom.repo.as_deref(), repo, "{input}.repo");
    }
}

#[test]
fn atom_rejects_representative_invalid_portage_cases() {
    for (input, allow_wildcard, allow_repo) in [
        ("cat/pkg\n", false, false),
        ("cat/Ҙ", false, false),
        ("cat/pkg:/slot", false, false),
        ("+cat/pkg", false, false),
        ("-cat/pkg", false, false),
        (".cat/pkg", false, false),
        ("cat/+pkg", false, false),
        ("cat/-pkg", false, false),
        ("cat/pkg[a!]", false, false),
        ("cat/pkg[!a]", false, false),
        ("cat/pkg[-a=]", false, false),
        ("cat/pkg[-a?]", false, false),
        ("sys-apps/portage[doc]:0", false, false),
        ("*/*", false, false),
        ("*/**", true, false),
        ("sys-apps/portage[doc]::repo_name", false, false),
        ("sys-apps/portage:0[doc]::repo_name", false, false),
        ("app-doc/php-docs-20071125", false, false),
        ("foo/bar-1", false, false),
    ] {
        assert!(
            Atom::parse_with_options(
                input,
                AtomParseOptions {
                    allow_wildcard,
                    allow_repo,
                },
            )
            .is_err(),
            "{input} should be rejected",
        );
    }
}

#[test]
fn isvalidatom_representative_cases_match_portage() {
    for (input, expected, allow_wildcard, allow_repo) in [
        ("sys-apps/portage", true, false, false),
        ("=sys-apps/portage-2.1", true, false, false),
        ("=sys-apps/portage-2.1*", true, false, false),
        (">=sys-apps/portage-2.1", true, false, false),
        ("~sys-apps/portage-2.1", true, false, false),
        ("sys-apps/portage:foo", true, false, false),
        ("sys-apps/portage-2.1:foo", false, false, false),
        (
            "=sys-apps/portage-2.2*:foo[bar,-baz,doc?,!build?]",
            true,
            false,
            false,
        ),
        ("=sys-apps/portage-2.2*:foo[!doc]", false, false, false),
        ("portage", false, false, false),
        ("=portage-2.1*", false, false, false),
        ("null/portage", true, false, false),
        (">=null/portage-2.1", true, false, false),
        (">=null/portage", false, false, false),
        ("~null/portage-2.1", true, false, false),
        ("games-strategy/ufo2000", true, false, false),
        ("app-text/7plus", true, false, false),
        ("foo/666", true, false, false),
        ("sys-apps/portage::repo_123-name", true, false, true),
        ("sys-apps/portage::repo_123-name", false, false, false),
        ("*/portage-2.1", false, true, false),
        ("*/portage", true, true, false),
    ] {
        assert_eq!(
            is_valid_atom(
                input,
                AtomParseOptions {
                    allow_wildcard,
                    allow_repo,
                },
            ),
            expected,
            "isvalidatom({input})",
        );
    }
}

#[test]
fn slot_abi_parts_match_portage_cases() {
    for (input, slot, sub_slot, slot_operator) in [
        ("virtual/ffmpeg:0/53", Some("0"), Some("53"), None),
        ("virtual/ffmpeg:0/53=", Some("0"), Some("53"), Some("=")),
        ("virtual/ffmpeg:=", None, None, Some("=")),
        ("virtual/ffmpeg:0=", Some("0"), None, Some("=")),
        ("virtual/ffmpeg:*", None, None, Some("*")),
        ("virtual/ffmpeg:0", Some("0"), None, None),
        ("virtual/ffmpeg", None, None, None),
    ] {
        let atom = Atom::parse(input).unwrap();
        assert_eq!(atom.slot(), slot, "{input}.slot");
        assert_eq!(atom.sub_slot(), sub_slot, "{input}.sub_slot");
        assert_eq!(atom.slot_operator(), slot_operator, "{input}.slot_operator");
    }
}

#[test]
fn blockers_and_intersects_match_representative_portage_cases() {
    assert_eq!(
        Atom::parse("!dev-libs/A").unwrap().blocker,
        Some(Blocker::Weak)
    );
    assert_eq!(
        Atom::parse("!!dev-libs/A").unwrap().blocker,
        Some(Blocker::Strong),
    );

    for (left, right, expected) in [
        ("dev-libs/A", "dev-libs/A", true),
        ("dev-libs/A", "dev-libs/B", false),
        ("dev-libs/A", "sci-libs/A", false),
        ("=dev-libs/A-1", "=dev-libs/A-1-r1", false),
        ("=dev-libs/A-1:1", "=dev-libs/A-1", true),
        ("=dev-libs/A-1:1", "=dev-libs/A-1:1", true),
        ("=dev-libs/A-1:1", "=dev-libs/A-1:2", false),
    ] {
        let left = Atom::parse(left).unwrap();
        let right = Atom::parse(right).unwrap();
        assert_eq!(left.intersects(&right), expected);
    }
}
