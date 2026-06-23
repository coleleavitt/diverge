use diverge::cli::{CliError, EmergeAction, EmergeRequest, YesNo};

#[test]
fn parses_merge_options_and_targets() {
    let request = EmergeRequest::parse([
        "--pretend",
        "--verbose",
        "--update",
        "--usepkg",
        "dev-libs/A",
        "=dev-libs/B-1.2",
    ])
    .unwrap();

    assert_eq!(request.action, EmergeAction::Merge);
    assert!(request.options.pretend);
    assert!(request.options.verbose);
    assert!(request.options.update);
    assert!(request.options.usepkg);
    assert_eq!(request.targets.len(), 2);
    assert_eq!(request.targets[0].cp(), "dev-libs/A");
    assert_eq!(request.targets[1].cpv(), "dev-libs/B-1.2");
}

#[test]
fn usepkgonly_implies_usepkg_like_emerge_selection_flags() {
    let request = EmergeRequest::parse(["--usepkgonly", "dev-libs/B"]).unwrap();
    assert!(request.options.usepkgonly);
    assert!(request.options.usepkg);
}

#[test]
fn search_requires_a_search_term() {
    assert_eq!(
        EmergeRequest::parse(["--search"]),
        Err(CliError::MissingSearchTerm),
    );

    let request = EmergeRequest::parse(["--search", "portage"]).unwrap();
    assert_eq!(request.action, EmergeAction::Search);
    assert_eq!(request.raw_targets, ["portage"]);
    assert!(request.targets.is_empty());
}

#[test]
fn non_merge_actions_reject_package_targets() {
    assert!(matches!(
        EmergeRequest::parse(["--sync", "dev-libs/A"]),
        Err(CliError::UnexpectedTarget { .. }),
    ));
    assert!(matches!(
        EmergeRequest::parse(["--info", "dev-libs/A"]),
        Err(CliError::UnexpectedTarget { .. }),
    ));
    assert!(matches!(
        EmergeRequest::parse(["--depclean", "dev-libs/A"]),
        Err(CliError::UnexpectedTarget { .. }),
    ));
}

#[test]
fn merge_rejects_invalid_atoms() {
    assert!(matches!(
        EmergeRequest::parse(["sys-apps/portage[doc]:0"]),
        Err(CliError::InvalidTarget { .. }),
    ));
}

#[test]
fn bundled_short_flags_expand_like_emerge() {
    // -pv == --pretend --verbose; -uD == --update --deep.
    let request = EmergeRequest::parse(["-pvuD", "dev-libs/A"]).unwrap();
    assert!(request.options.pretend);
    assert!(request.options.verbose);
    assert!(request.options.update);
    assert!(request.options.deep);
    assert_eq!(request.action, EmergeAction::Merge);

    // -1 is --oneshot.
    let request = EmergeRequest::parse(["-1", "dev-libs/A"]).unwrap();
    assert!(request.options.oneshot);
}

#[test]
fn short_action_flags_select_actions() {
    assert_eq!(
        EmergeRequest::parse(["-C", "dev-libs/A"]).unwrap().action,
        EmergeAction::Unmerge
    );
    assert_eq!(
        EmergeRequest::parse(["-c"]).unwrap().action,
        EmergeAction::Depclean
    );
    assert_eq!(
        EmergeRequest::parse(["-s", "portage"]).unwrap().action,
        EmergeAction::Search
    );
}

#[test]
fn yes_no_valued_options_parse_equals_form() {
    let request = EmergeRequest::parse(["--ask=y", "--quiet=n", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.ask, YesNo::Yes);
    assert_eq!(request.options.quiet, YesNo::No);

    // Bare --ask defaults to yes.
    let request = EmergeRequest::parse(["--ask", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.ask, YesNo::Yes);

    // Invalid value is rejected.
    assert!(matches!(
        EmergeRequest::parse(["--ask=maybe", "dev-libs/A"]),
        Err(CliError::InvalidOptionValue { .. })
    ));
}

#[test]
fn integer_valued_options_parse() {
    let request = EmergeRequest::parse(["--jobs=4", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.jobs, Some(4));
    assert!(matches!(
        EmergeRequest::parse(["--jobs=x", "dev-libs/A"]),
        Err(CliError::InvalidOptionValue { .. })
    ));
}

#[test]
fn package_sets_are_collected() {
    let request = EmergeRequest::parse(["-uD", "@world"]).unwrap();
    assert_eq!(request.sets, vec!["world".to_string()]);
    assert!(request.raw_targets.is_empty());

    // depclean accepts a set target.
    let request = EmergeRequest::parse(["--depclean", "@world"]).unwrap();
    assert_eq!(request.action, EmergeAction::Depclean);
    assert_eq!(request.sets, vec!["world".to_string()]);
}

#[test]
fn conflicting_actions_are_rejected() {
    assert!(matches!(
        EmergeRequest::parse(["--depclean", "--unmerge", "dev-libs/A"]),
        Err(CliError::MultipleActions { .. })
    ));
}

#[test]
fn unmerge_accepts_atom_targets() {
    let request = EmergeRequest::parse(["--unmerge", "dev-libs/A"]).unwrap();
    assert_eq!(request.action, EmergeAction::Unmerge);
    assert_eq!(request.targets.len(), 1);
    assert_eq!(request.targets[0].cp(), "dev-libs/A");
}
