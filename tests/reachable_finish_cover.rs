//! Final reachable-arm coverage: run_merge_phases, UnmergeError Display,
//! config tokenizer edges, depgraph build-cycle, required_use operator-in-group.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn run_merge_phases_runs_preinst_postinst() {
    use diverge::executor::phase::{
        BuildDirs,
        Phase,
        PhaseContext,
        PhaseOutcome,
        PhaseSpawner,
        run_merge_phases,
    };
    struct Rec {
        ran: Vec<Phase>,
    }
    impl PhaseSpawner for Rec {
        fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
            self.ran.push(phase);
            PhaseOutcome {
                phase,
                success: true,
                message: None,
            }
        }
    }
    let ctx = PhaseContext {
        ebuild: PathBuf::from("/e"),
        cpv: "c/p-1".into(),
        eapi: "7".into(),
        root: PathBuf::from("/r"),
        dirs: BuildDirs::new(PathBuf::from("/b"), PathBuf::from("/r")),
        use_flags: vec![],
    };
    let mut sp = Rec { ran: vec![] };
    let outcomes = run_merge_phases(&ctx, &mut sp);
    assert_eq!(outcomes.len(), 2);
    assert_eq!(sp.ran, vec![Phase::PkgPreinst, Phase::PkgPostinst]);
}

#[test]
fn unmerge_error_display() {
    use diverge::executor::UnmergeError;
    assert_eq!(format!("{}", UnmergeError::Io("boom".into())), "boom");
    let _: &dyn std::error::Error = &UnmergeError::Io("x".into());
}

#[test]
fn config_tokenizer_comment_and_empty_key() {
    use diverge::config::getconfig;
    let empty = std::collections::HashMap::new();
    // A `#` mid-stream terminates the token (config 288 break).
    let c = getconfig("A=\"1\"#c\nB=\"2\"\n", true, &empty).unwrap();
    assert_eq!(c.get("A").map(String::as_str), Some("1"));
    assert_eq!(c.get("B").map(String::as_str), Some("2"));
    // Leading whitespace/separator then a real token (303 recurse path).
    let c = getconfig("   A=\"1\"\n", true, &empty).unwrap();
    assert_eq!(c.get("A").map(String::as_str), Some("1"));
    // An empty key (invalid_var_name None arm, config 347): a value-less `=`.
    assert!(getconfig("=\"x\"\n", true, &empty).is_err());
}

#[test]
fn depgraph_build_cycle_records_path() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};
    fn pkg(dep: &str) -> PackageMetadata {
        let mut m = PackageMetadata {
            slot: Some("0".into()),
            sub_slot: None,
            repo: Some("r".into()),
            eapi: Some("7".into()),
            iuse: vec![],
            use_enabled: vec![],
            keywords: vec!["x86".into()],
            deps: Default::default(),
        };
        if !dep.is_empty() {
            m.deps.insert("DEPEND".into(), dep.into());
        }
        m
    }
    // A -> B -> C -> A build-time cycle records the in_progress path (728-730).
    let mut av = PackageDb::new();
    av.insert("c/A-1", pkg("c/B"));
    av.insert("c/B-1", pkg("c/C"));
    av.insert("c/C-1", pkg("c/A"));
    let outcome = Resolver::new(&av, &PackageDb::new(), ResolveParams::default()).resolve(&["c/A"]);
    match outcome.error {
        Some(ResolveFailure::CircularDependency(cycle)) => {
            assert!(cycle.len() >= 2, "cycle path recorded: {cycle:?}");
        }
        other => panic!("expected circular dependency, got {other:?}"),
    }
}

#[test]
fn required_use_operator_inside_group_evaluated() {
    use diverge::dep::check_required_use;
    let iuse = |f: &str| ["a", "b", "c"].contains(&f);
    // An operator nested inside a plain group: ( ^^ ( a b ) c ).
    assert!(check_required_use("( ^^ ( a b ) c )", &["a", "c"], iuse, Some("7")).unwrap());
    assert!(!check_required_use("( ^^ ( a b ) c )", &["a", "b", "c"], iuse, Some("7")).unwrap());
    // unmatched close inside (required_use 760 malformed).
    assert!(check_required_use("a ) b", &[], iuse, Some("7")).is_err());
}

#[test]
fn session_skips_unreadable_repos_conf_fragment() {
    use std::fs;

    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("tree");
    fs::create_dir_all(repo.join("profiles")).unwrap();
    fs::write(repo.join("profiles/repo_name"), "gentoo\n").unwrap();
    fs::create_dir_all(repo.join("app/x")).unwrap();
    fs::write(
        repo.join("app/x/x-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    )
    .unwrap();
    // repos.conf directory with a real fragment plus a subdirectory (which is
    // not a readable file -> the read_to_string skip at session 431/497).
    let conf = dir.path().join("etc/portage/repos.conf");
    fs::create_dir_all(conf.join("subdir")).unwrap();
    fs::write(
        conf.join("gentoo.conf"),
        format!("[gentoo]\nlocation = {}\n", repo.display()),
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("etc/portage")).unwrap();
    fs::write(dir.path().join("etc/portage/make.conf"), "ARCH=\"amd64\"\n").unwrap();
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(!s.available.match_str("app/x").unwrap().is_empty());
}
