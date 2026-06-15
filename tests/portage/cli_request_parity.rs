use diverge::cli::{CliError, EmergeAction, EmergeRequest};

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
