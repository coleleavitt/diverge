//! Residual coverage: every CLI short flag, phase helpers, resolver generic
//! dependency path, and remaining atom/version accessor branches.

use diverge::cli::EmergeRequest;
use diverge::executor::phase::{BuildDirs, Phase, build_phases};

#[test]
fn every_short_flag_parses() {
    // Each short flag maps to a long option; parse them so the map arms run.
    // Action flags are parsed alone; option flags with a dummy target.
    for f in [
        "-1", "-b", "-B", "-d", "-e", "-f", "-g", "-G", "-k", "-K", "-n", "-N", "-o", "-O", "-p",
        "-q", "-r", "-t", "-u", "-U", "-a", "-v", "-w", "-D",
    ] {
        let r = EmergeRequest::parse([f, "dev-libs/A"]);
        assert!(r.is_ok(), "{f} failed: {r:?}");
    }
    // Action short flags.
    for f in ["-c", "-P"] {
        assert!(EmergeRequest::parse([f]).is_ok(), "{f}");
    }
    assert!(EmergeRequest::parse(["-C", "dev-libs/A"]).is_ok());
    assert!(EmergeRequest::parse(["-s", "term"]).is_ok());
    assert!(EmergeRequest::parse(["-h"]).is_ok());
    assert!(EmergeRequest::parse(["-V"]).is_ok());
    // Unknown short flag -> error (covers the `_ => return None` arm).
    assert!(EmergeRequest::parse(["-Z"]).is_err());
}

#[test]
fn phase_func_names_all() {
    for p in [
        Phase::PkgSetup,
        Phase::SrcUnpack,
        Phase::SrcPrepare,
        Phase::SrcConfigure,
        Phase::SrcCompile,
        Phase::SrcTest,
        Phase::SrcInstall,
        Phase::PkgPreinst,
        Phase::PkgPostinst,
    ] {
        assert!(!p.func_name().is_empty());
    }
    // EAPI 0 omits prepare/configure (the gated branch).
    assert!(!build_phases("0").contains(&Phase::SrcPrepare));
}

#[test]
fn build_dirs_create_makes_all() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = BuildDirs::new(
        dir.path().join("build/cat/p-1"),
        dir.path().join("repo/cat/p"),
    );
    dirs.create().unwrap();
    assert!(dirs.build_dir.is_dir());
    assert!(dirs.work_dir.is_dir());
    assert!(dirs.temp_dir.is_dir());
    assert!(dirs.image_dir.is_dir());
    assert!(dirs.files_dir.ends_with("files"));
}

#[test]
fn resolver_generic_dependency_path() {
    use diverge::cli::EmergeOptions;
    use diverge::resolver::{PackageRecord, ResolverFixture};
    // A DEPEND with two whitespace-separated atoms exercises the generic
    // split_whitespace().filter().map() path (not the hard-coded OR string).
    let fixture = ResolverFixture {
        ebuilds: vec![
            PackageRecord::new("app/main-1")
                .with_keywords(["x86"])
                .with_depend("app/liba app/libb"),
            PackageRecord::new("app/liba-1").with_keywords(["x86"]),
            PackageRecord::new("app/libb-1").with_keywords(["x86"]),
        ],
        binpkgs: vec![],
        installed: vec![],
    };
    let r = fixture.resolve("app/main", &EmergeOptions::default());
    assert!(r.success, "{:?}", r.error);
    assert!(r.mergelist.iter().any(|m| m.contains("app/liba")));
    assert!(r.mergelist.iter().any(|m| m.contains("app/libb")));

    // A dep with an unsatisfiable token -> failure (covers the ok_or_else arm).
    let fixture = ResolverFixture {
        ebuilds: vec![
            PackageRecord::new("app/x-1")
                .with_keywords(["x86"])
                .with_depend("app/missing"),
        ],
        binpkgs: vec![],
        installed: vec![],
    };
    let r = fixture.resolve("app/x", &EmergeOptions::default());
    assert!(!r.success);
}

#[test]
fn resolver_or_choice_picks_installed_first_branch() {
    use diverge::cli::EmergeOptions;
    use diverge::resolver::simple_portage_fixture;
    // app-misc/Z's OR-choice: Y is ~x86 so the (X W) branch is taken; this
    // drives lines 154-162 of the hard-coded OR path.
    let fixture = simple_portage_fixture();
    let r = fixture.resolve("app-misc/Z", &EmergeOptions::default());
    assert!(r.success, "{:?}", r.error);
    assert!(r.mergelist.iter().any(|m| m.contains("app-misc/W")));
}
