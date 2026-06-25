//! End-to-end session integration: load a real on-disk config root (make.conf,
//! repos.conf, profile symlink, vardb) and drive resolution + pretend output.
//!
//! Proves the binary's actual flow works against a real filesystem layout,
//! using only an isolated tempdir (never the host `/`).
//!
//! Reference:
//! - `research/portage/lib/_emerge/main.py`
//! - `research/portage/lib/portage/tests/emerge/test_baseline.py`

use diverge::cli::EmergeRequest;
use diverge::session::Session;

use crate::fs_fixture::write;

fn ebuild(meta: &[(&str, &str)]) -> String {
    meta.iter()
        .map(|(k, v)| format!("{k}=\"{v}\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Builds a minimal but realistic config root: a repo, a profile, make.conf,
/// repos.conf, and an installed package in the vardb.
fn fixture_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    // Repository tree at <root>/var/db/repos/gentoo.
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-editors/nano/nano-7.2.ebuild"),
        &ebuild(&[
            ("EAPI", "7"),
            ("SLOT", "0"),
            ("KEYWORDS", "amd64"),
            ("RDEPEND", "sys-libs/ncurses"),
        ]),
    );
    write(
        &repo.join("sys-libs/ncurses/ncurses-6.4.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );

    // A profile selecting amd64.
    let profile = root.join("var/db/repos/gentoo/profiles/default/linux/amd64");
    write(
        &profile.join("make.defaults"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );

    // make.profile symlink -> the profile dir.
    let make_profile = root.join("etc/portage/make.profile");
    std::fs::create_dir_all(make_profile.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&profile, &make_profile).expect("symlink make.profile");

    // make.conf and repos.conf.
    write(
        &root.join("etc/portage/make.conf"),
        "USE=\"ncurses\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );

    // An installed package in the vardb: ncurses already present.
    let vdb = root.join("var/db/pkg/sys-libs/ncurses-6.4");
    write(&vdb.join("SLOT"), "0\n");
    write(&vdb.join("KEYWORDS"), "amd64\n");
    write(&vdb.join("EAPI"), "7\n");
    write(&vdb.join("repository"), "gentoo\n");

    dir
}

#[test]
fn session_loads_real_config_root() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("load session");

    // Repositories were discovered via repos.conf and loaded.
    assert!(!session.available.is_empty(), "available packages loaded");
    assert!(
        !session
            .available
            .match_str("app-editors/nano")
            .unwrap()
            .is_empty(),
        "nano present in available db"
    );
    // The installed ncurses was read from the vardb.
    assert!(
        !session
            .installed
            .match_str("sys-libs/ncurses")
            .unwrap()
            .is_empty(),
        "installed ncurses present"
    );
    // Profile + make.conf supplied arch/keywords/use.
    assert_eq!(session.arch(), "amd64");
    assert!(session.accept_keywords().contains(&"amd64".to_string()));
}

#[test]
fn session_resolves_against_real_config() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("load session");
    let request = EmergeRequest::parse(["app-editors/nano"]).unwrap();

    let outcome = session.resolve(&request);
    assert!(outcome.is_success(), "resolve failed: {:?}", outcome.error);
    // nano is merged; ncurses is already installed so it is not re-merged.
    assert!(
        outcome
            .mergelist
            .contains(&"app-editors/nano-7.2".to_string())
    );
    assert!(
        !outcome
            .mergelist
            .contains(&"sys-libs/ncurses-6.4".to_string())
    );
}

#[test]
fn session_pretend_renders_plan() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("load session");
    let request = EmergeRequest::parse(["-p", "app-misc/hello"]).unwrap();

    let report = session.pretend(&request);
    // The pretend report lists the package as a new (N) ebuild merge.
    assert!(report.contains("app-misc/hello-1"), "report: {report}");
    assert!(report.contains("[ebuild"), "emerge-style line: {report}");
    assert!(report.contains("Total: 1 package"), "total line: {report}");
}

#[test]
fn session_missing_config_root_is_empty_not_error() {
    // Pointing at an empty dir yields an empty (but valid) session.
    let dir = tempfile::tempdir().expect("tempdir");
    let session = Session::load(dir.path(), dir.path()).expect("empty session loads");
    assert!(session.available.is_empty());
    assert!(session.installed.is_empty());
}

/// Confirms the loader never requires (or touches) anything outside the root.
#[test]
fn session_is_isolated_to_its_root() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("load");
    // The eroot is exactly what we passed; no host paths leak in.
    assert_eq!(session.eroot, dir.path());
    assert_eq!(session.config_root, dir.path());
}
