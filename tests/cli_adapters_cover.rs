//! Coverage for cli / sync / news / update branches.

use std::fs;
use std::path::Path;

use diverge::cli::{CliError, EmergeAction, EmergeRequest, YesNo};
use diverge::news::{NewsEnvironment, NewsItem, ReadTracker};
use diverge::sync::{LocalSync, SyncBackend, SyncConfig, SyncError, SyncType};
use diverge::update::{UpdateCommand, parse_updates, update_dbentries, update_dbentry};

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn cli_short_flags_map() {
    let req = EmergeRequest::parse(["-pvuDN1", "d/A"]).unwrap();
    assert!(req.options.pretend);
    assert!(req.options.verbose);
    assert!(req.options.update);
    assert!(req.options.deep);
    assert!(req.options.newuse);
    assert!(req.options.oneshot);
    assert_eq!(
        EmergeRequest::parse(["-C", "d/A"]).unwrap().action,
        EmergeAction::Unmerge
    );
    assert_eq!(
        EmergeRequest::parse(["-c"]).unwrap().action,
        EmergeAction::Depclean
    );
}

#[test]
fn cli_valued_options_and_errors() {
    let req = EmergeRequest::parse(["--ask=y", "--quiet=n", "--jobs=4", "d/A"]).unwrap();
    assert_eq!(req.options.ask, YesNo::Yes);
    assert_eq!(req.options.quiet, YesNo::No);
    assert_eq!(req.options.jobs, Some(4));
    assert!(matches!(
        EmergeRequest::parse(["--ask=maybe", "d/A"]),
        Err(CliError::InvalidOptionValue { .. })
    ));
    assert!(matches!(
        EmergeRequest::parse(["--jobs=x", "d/A"]),
        Err(CliError::InvalidOptionValue { .. })
    ));
    assert!(matches!(
        EmergeRequest::parse(["--frobnicate"]),
        Err(CliError::UnknownOption(_))
    ));
    assert!(matches!(
        EmergeRequest::parse(["d/A[bad"]),
        Err(CliError::InvalidTarget { .. })
    ));
    assert!(matches!(
        EmergeRequest::parse(["--depclean", "--unmerge", "d/A"]),
        Err(CliError::MultipleActions { .. })
    ));
    assert!(matches!(
        EmergeRequest::parse(["--search"]),
        Err(CliError::MissingSearchTerm)
    ));
}

#[test]
fn cli_usepkgonly_implies_usepkg_and_sets() {
    let req = EmergeRequest::parse(["-G", "d/A"]).unwrap();
    assert!(req.options.usepkg && req.options.usepkgonly);
    let req = EmergeRequest::parse(["-uD", "@world"]).unwrap();
    assert_eq!(req.sets, vec!["world".to_string()]);
    assert!(req.raw_targets.is_empty());
}

#[test]
fn local_sync_copies_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");
    write(&src.join("a/b.txt"), "hi\n");
    write(&src.join("c.txt"), "yo\n");
    let cfg = SyncConfig {
        name: "r".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };
    let mut backend = LocalSync;
    let o = backend.sync(&cfg).unwrap();
    assert!(o.updated);
    assert!(dest.join("a/b.txt").exists());
    // re-sync no changes.
    let o2 = backend.sync(&cfg).unwrap();
    assert!(!o2.updated);
    // missing source.
    let bad = SyncConfig {
        name: "r".to_string(),
        location: dest,
        uri: dir.path().join("nope").to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };
    assert!(matches!(
        backend.sync(&bad),
        Err(SyncError::SourceMissing(_))
    ));
}

#[test]
fn news_relevance_and_read_tracker() {
    let text = "Title: T\nAuthor: a\nPosted: 2024\nRevision: 1\nNews-Item-Format: 2.0\n\
        Display-If-Installed: dev-libs/A\nDisplay-If-Keyword: amd64\n\nbody\n";
    let item = NewsItem::parse(text);
    assert!(item.is_valid());
    let yes = NewsEnvironment {
        installed: vec!["dev-libs/A-1".to_string()],
        keyword: "amd64".to_string(),
        profile: None,
    };
    assert!(item.is_relevant(&yes));
    let no = NewsEnvironment {
        installed: vec!["dev-libs/A-1".to_string()],
        keyword: "x86".to_string(),
        profile: None,
    };
    assert!(!item.is_relevant(&no)); // keyword AND fails

    let mut rt = ReadTracker::parse("a\nb\n");
    assert!(rt.is_read("a"));
    assert!(!rt.is_read("c"));
    let names = vec!["a".to_string(), "c".to_string()];
    assert_eq!(rt.unread(&names), vec![&"c".to_string()]);
    assert!(rt.mark_read("c"));
    assert!(rt.render().contains("c"));
}

#[test]
fn update_move_and_slotmove() {
    let cmds = parse_updates("move dev-libs/A dev-libs/A2\nslotmove dev-libs/B 0 1\n").unwrap();
    assert_eq!(cmds.len(), 2);
    let mv = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/A2".to_string(),
    };
    assert_eq!(update_dbentry(&mv, ">=dev-libs/A-1.2"), ">=dev-libs/A2-1.2");
    assert_eq!(
        update_dbentry(&mv, "dev-libs/A   dev-libs/C"),
        "dev-libs/A2   dev-libs/C"
    );
    let sm = UpdateCommand::SlotMove {
        cp: "dev-libs/B".to_string(),
        from_slot: "0".to_string(),
        to_slot: "1".to_string(),
    };
    assert_eq!(update_dbentry(&sm, "dev-libs/B:0"), "dev-libs/B:1");
    assert_eq!(update_dbentry(&sm, "dev-libs/B"), "dev-libs/B");
    // sequence.
    let seq = vec![
        UpdateCommand::Move {
            from: "d/A".into(),
            to: "d/A2".into(),
        },
        UpdateCommand::Move {
            from: "d/A2".into(),
            to: "d/A3".into(),
        },
    ];
    assert_eq!(update_dbentries(&seq, "d/A"), "d/A3");
    // parse error.
    assert!(parse_updates("bogus line").is_err());
}
