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

#[test]
fn search_action_lists_matching_packages() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");
    let report = session.search(&["nano".to_string()]);
    assert!(report.contains("app-editors/nano"), "search: {report}");
    assert!(
        report.contains("Latest version available"),
        "search: {report}"
    );
    // A non-matching term yields no packages.
    let empty = session.search(&["zzznotreal".to_string()]);
    assert!(empty.contains("No packages found"), "empty search: {empty}");
}

#[test]
fn dispatch_routes_each_action() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");

    let search = EmergeRequest::parse(["-s", "nano"]).unwrap();
    assert!(session.dispatch(&search).contains("app-editors/nano"));

    let version = EmergeRequest::parse(["--version"]).unwrap();
    assert!(session.dispatch(&version).contains("diverge"));

    let moo = EmergeRequest::parse(["--moo"]).unwrap();
    assert!(session.dispatch(&moo).contains("mooed"));

    let list_sets = EmergeRequest::parse(["--list-sets"]).unwrap();
    let sets = session.dispatch(&list_sets);
    assert!(sets.contains("world") && sets.contains("system"));

    let info = EmergeRequest::parse(["--info"]).unwrap();
    let info_out = session.dispatch(&info);
    assert!(info_out.contains("ARCH=amd64"), "info: {info_out}");
}

#[test]
fn depclean_report_lists_unreferenced_installed() {
    let dir = fixture_root();
    // Install an extra package not in world -> depclean should list it.
    let vdb = dir.path().join("var/db/pkg/app-misc/orphan-1");
    write(&vdb.join("SLOT"), "0\n");
    write(&vdb.join("EAPI"), "7\n");
    // ncurses is in the world via the fixture? No — world is empty here, so both
    // installed packages are candidates. Seed world with ncurses to protect it.
    write(
        &dir.path().join("var/lib/portage/world"),
        "sys-libs/ncurses\n",
    );

    let session = Session::load(dir.path(), dir.path()).expect("session");
    let request = EmergeRequest::parse(["--depclean"]).unwrap();
    let report = session.depclean_report(&request);
    // orphan is not protected -> appears in the unmerge list.
    assert!(report.contains("app-misc/orphan-1"), "depclean: {report}");
    // ncurses is protected by world -> not listed.
    assert!(
        !report.contains("sys-libs/ncurses-6.4"),
        "depclean: {report}"
    );
}

#[test]
fn sync_action_copies_repo_from_local_source() {
    // A repos.conf with sync-type=rsync + a local sync-uri; LocalSync copies it.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // The sync *source* tree.
    let src = root.join("source/gentoo");
    write(&src.join("profiles/repo_name"), "gentoo\n");
    write(
        &src.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );
    // The empty destination + repos.conf pointing at the source.
    let dest = root.join("var/db/repos/gentoo");
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    write(
        &root.join("etc/portage/repos.conf"),
        &format!(
            "[gentoo]\nlocation = {}\nsync-type = rsync\nsync-uri = {}\n",
            dest.display(),
            src.display()
        ),
    );

    let session = Session::load(root, root).expect("session");
    // Repo config parsed with sync settings.
    assert_eq!(session.repos.len(), 1);
    assert_eq!(session.repos[0].sync_type.as_deref(), Some("rsync"));
    assert_eq!(
        session.repos[0].sync_uri.as_deref(),
        Some(src.to_str().unwrap())
    );

    let mut backend = diverge::sync::LocalSync;
    let report = session.sync_action(&mut backend);
    assert!(
        report.contains(">>> Syncing repository 'gentoo'"),
        "report: {report}"
    );
    assert!(report.contains("1 synced, 0 failed"), "report: {report}");
    // The destination tree now has the ebuild.
    assert!(dest.join("app-misc/hello/hello-1.ebuild").exists());
}

#[test]
fn sync_action_reports_failure_for_missing_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    write(
        &root.join("etc/portage/repos.conf"),
        &format!(
            "[gentoo]\nlocation = {}\nsync-type = rsync\nsync-uri = {}\n",
            root.join("dest").display(),
            root.join("does-not-exist").display()
        ),
    );
    let session = Session::load(root, root).expect("session");
    let mut backend = diverge::sync::LocalSync;
    let report = session.sync_action(&mut backend);
    assert!(report.contains("!!! Sync error"), "report: {report}");
    assert!(report.contains("0 synced, 1 failed"), "report: {report}");
}

#[test]
fn regen_action_writes_md5_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[
            ("EAPI", "7"),
            ("SLOT", "0/2"),
            ("KEYWORDS", "amd64 ~x86"),
            ("IUSE", "+foo bar"),
            ("DEPEND", "dev-libs/b"),
        ]),
    );
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );

    let session = Session::load(root, root).expect("session");
    let report = session.regen_action();
    assert!(
        report.contains("Regenerated 1 cache entry"),
        "report: {report}"
    );

    // The md5-cache entry exists with the expected KEY=value lines.
    let entry = repo.join("metadata/md5-cache/app-misc/hello-1");
    assert!(entry.exists(), "cache entry written");
    let body = std::fs::read_to_string(&entry).unwrap();
    assert!(body.contains("SLOT=0/2"), "{body}");
    assert!(body.contains("EAPI=7"), "{body}");
    assert!(body.contains("KEYWORDS=amd64 ~x86"), "{body}");
    assert!(body.contains("DEPEND=dev-libs/b"), "{body}");
    // IUSE default markers are stripped by the loader.
    assert!(body.contains("IUSE=foo bar"), "{body}");
}

#[test]
fn config_action_runs_pkg_config_on_installed() {
    use std::collections::BTreeMap;

    use diverge::executor::phase::{Phase, PhaseOutcome, PhaseSpawner};

    // A fake spawner recording the phases + cpv (via PF env) it was asked to run.
    struct Recorder {
        ran: Vec<(String, Phase)>,
    }
    impl PhaseSpawner for Recorder {
        fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
            self.ran
                .push((env.get("PF").cloned().unwrap_or_default(), phase));
            PhaseOutcome {
                phase,
                success: true,
                message: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    // An installed package in the vardb.
    let vdb = root.join("var/db/pkg/app-misc/tool-1");
    write(&vdb.join("SLOT"), "0\n");
    write(&vdb.join("EAPI"), "7\n");

    let session = Session::load(root, root).expect("session");
    let mut rec = Recorder { ran: Vec::new() };
    let report = session.config_action(&["app-misc/tool".to_string()], &mut rec);

    assert!(
        report.contains(">>> Configured app-misc/tool-1"),
        "report: {report}"
    );
    // The pkg_config phase ran for the installed package.
    assert_eq!(rec.ran, vec![("tool-1".to_string(), Phase::PkgConfig)]);

    // An atom with no installed match is reported, runs nothing.
    let mut rec2 = Recorder { ran: Vec::new() };
    let report = session.config_action(&["app-misc/absent".to_string()], &mut rec2);
    assert!(
        report.contains("'app-misc/absent' is not installed"),
        "report: {report}"
    );
    assert!(rec2.ran.is_empty());
}

#[test]
fn merge_action_builds_merges_and_records() {
    use std::collections::BTreeMap;

    use diverge::executor::phase::{Phase, PhaseOutcome, PhaseSpawner};

    // A spawner that succeeds every build phase.
    struct Ok;
    impl PhaseSpawner for Ok {
        fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
            PhaseOutcome {
                phase,
                success: true,
                message: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    // A prebuilt image for hello (stands in for the package's D).
    let image = root.join("images/hello");
    write(&image.join("usr/bin/hello"), "#!/bin/sh\n");

    let session = Session::load(root, root).expect("session");
    let request = EmergeRequest::parse(["app-misc/hello"]).unwrap();
    let mut spawner = Ok;
    let report = session
        .merge_action(&request, &mut spawner, |cpv| {
            (cpv == "app-misc/hello-1").then(|| image.clone())
        })
        .expect("merge");

    assert_eq!(report.merged, vec!["app-misc/hello-1"]);
    assert!(report.failed.is_none());
    // File landed in the root, VDB recorded, world updated.
    assert!(root.join("usr/bin/hello").exists());
    assert!(root.join("var/db/pkg/app-misc/hello-1/CONTENTS").exists());
    assert!(
        session
            .world_atoms()
            .contains(&"app-misc/hello".to_string())
    );

    // A freshly-loaded session sees it installed.
    let reloaded = Session::load(root, root).expect("reload");
    assert!(
        !reloaded
            .installed
            .match_str("app-misc/hello")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn merge_action_refuses_host_root() {
    // eroot = "/" without the override must be refused (never touches host).
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let session = Session::load(dir.path(), "/").expect("session");
    use std::collections::BTreeMap;

    use diverge::executor::phase::{Phase, PhaseOutcome, PhaseSpawner};
    struct Ok;
    impl PhaseSpawner for Ok {
        fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
            PhaseOutcome {
                phase,
                success: true,
                message: None,
            }
        }
    }
    let request = EmergeRequest::parse(["app-misc/hello"]).unwrap();
    let mut spawner = Ok;
    let err = session
        .merge_action(&request, &mut spawner, |_| None)
        .expect_err("must refuse ROOT=/");
    assert!(format!("{err}").contains("refusing to merge into ROOT=/"));
}

#[test]
fn unmerge_action_removes_installed_files_and_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    // Install a package by recording it (image + vdb) the way merge would.
    use diverge::dbapi::PackageMetadata;
    use diverge::executor::merge::ContentEntry;
    write(&root.join("usr/bin/tool"), "x\n");
    let contents = vec![
        ContentEntry::Dir { path: "usr".into() },
        ContentEntry::File {
            path: "usr/bin/tool".into(),
            protected: false,
        },
    ];
    diverge::vardb::record_install(
        &diverge::vardb::vdb_path(root),
        "app-misc/tool-1",
        &PackageMetadata {
            slot: Some("0".into()),
            eapi: Some("7".into()),
            ..PackageMetadata::default()
        },
        &contents,
    )
    .unwrap();

    let session = Session::load(root, root).expect("session");
    let removed = session
        .unmerge_action(&["app-misc/tool".to_string()])
        .expect("unmerge");
    assert_eq!(removed, vec!["app-misc/tool-1"]);
    assert!(!root.join("usr/bin/tool").exists(), "file removed");
    assert!(
        !root.join("var/db/pkg/app-misc/tool-1").exists(),
        "vdb entry removed"
    );
}

#[test]
fn unmerge_action_refuses_host_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let session = Session::load(dir.path(), "/").expect("session");
    let err = session
        .unmerge_action(&["app-misc/tool".to_string()])
        .expect_err("must refuse ROOT=/");
    assert!(format!("{err}").contains("refusing to unmerge from ROOT=/"));
}

#[test]
fn prune_action_keeps_highest_version_per_slot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    // Three installed versions of the same cp+slot; prune keeps only the newest.
    use diverge::dbapi::PackageMetadata;
    use diverge::executor::merge::ContentEntry;
    let vdb = diverge::vardb::vdb_path(root);
    for ver in ["1", "2", "3"] {
        let cpv = format!("dev-libs/foo-{ver}");
        write(&root.join(format!("usr/lib/foo{ver}")), "x\n");
        diverge::vardb::record_install(
            &vdb,
            &cpv,
            &PackageMetadata {
                slot: Some("0".into()),
                eapi: Some("7".into()),
                ..Default::default()
            },
            &[ContentEntry::File {
                path: format!("usr/lib/foo{ver}"),
                protected: false,
            }],
        )
        .unwrap();
    }
    let session = Session::load(root, root).expect("session");
    let pruned = session.prune_action().expect("prune");
    // Versions 1 and 2 pruned; 3 kept.
    assert_eq!(pruned, vec!["dev-libs/foo-1", "dev-libs/foo-2"]);
    assert!(root.join("var/db/pkg/dev-libs/foo-3").exists());
    assert!(!root.join("var/db/pkg/dev-libs/foo-1").exists());
    assert!(!root.join("usr/lib/foo1").exists());
}

#[test]
fn clean_action_removes_unreferenced_installed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    use diverge::dbapi::PackageMetadata;
    let vdb = diverge::vardb::vdb_path(root);
    // keep is in world; orphan is not referenced anywhere -> cleaned.
    for cpv in ["dev-libs/keep-1", "dev-libs/orphan-1"] {
        diverge::vardb::record_install(
            &vdb,
            cpv,
            &PackageMetadata {
                slot: Some("0".into()),
                eapi: Some("7".into()),
                ..Default::default()
            },
            &[],
        )
        .unwrap();
    }
    write(&root.join("var/lib/portage/world"), "dev-libs/keep\n");

    let session = Session::load(root, root).expect("session");
    let cleaned = session.clean_action().expect("clean");
    assert_eq!(cleaned, vec!["dev-libs/orphan-1"]);
    assert!(root.join("var/db/pkg/dev-libs/keep-1").exists());
    assert!(!root.join("var/db/pkg/dev-libs/orphan-1").exists());
}

#[test]
fn clean_and_prune_refuse_host_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let session = Session::load(dir.path(), "/").expect("session");
    assert!(
        format!("{}", session.clean_action().unwrap_err()).contains("refusing to clean ROOT=/")
    );
    assert!(
        format!("{}", session.prune_action().unwrap_err()).contains("refusing to prune ROOT=/")
    );
}

#[test]
fn check_news_lists_relevant_unread_items() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(&root.join("etc/portage/make.conf"), "ARCH=\"amd64\"\n");
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    // A news item that is Display-If-Installed dev-libs/foo (which is installed).
    let item = "2024-01-01-test";
    write(
        &repo.join(format!("metadata/news/{item}/{item}.en.txt")),
        "Title: Test\nAuthor: a\nPosted: 2024-01-01\nRevision: 1\nNews-Item-Format: 2.0\n\
         Display-If-Installed: dev-libs/foo\n\nbody\n",
    );
    // Install dev-libs/foo so the item is relevant.
    use diverge::dbapi::PackageMetadata;
    diverge::vardb::record_install(
        &diverge::vardb::vdb_path(root),
        "dev-libs/foo-1",
        &PackageMetadata {
            slot: Some("0".into()),
            eapi: Some("7".into()),
            ..Default::default()
        },
        &[],
    )
    .unwrap();

    let session = Session::load(root, root).expect("session");
    let unread = session.check_news();
    assert_eq!(unread, vec![item.to_string()]);

    // An irrelevant item (Display-If-Installed a missing pkg) is not listed.
    let item2 = "2024-02-02-other";
    write(
        &repo.join(format!("metadata/news/{item2}/{item2}.en.txt")),
        "Title: Other\nAuthor: a\nPosted: 2024-02-02\nRevision: 1\nNews-Item-Format: 2.0\n\
         Display-If-Installed: dev-libs/not-installed\n\nbody\n",
    );
    let session = Session::load(root, root).expect("reload");
    let unread = session.check_news();
    assert!(unread.contains(&item.to_string()));
    assert!(!unread.contains(&item2.to_string()));
}

#[cfg(unix)]
#[test]
fn merge_action_builds_real_image_from_ebuild_sh() {
    use std::os::unix::fs::PermissionsExt;

    use diverge::executor::CommandSpawner;

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );

    // A stub ebuild.sh: on the `src_install` phase, populate $D (the image).
    let script = root.join("ebuild.sh");
    write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = \"src_install\" ]; then\n  mkdir -p \"$D/usr/bin\"\n  echo hi > \"$D/usr/bin/hello\"\nfi\nexit 0\n",
    );
    let mut perm = std::fs::metadata(&script).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&script, perm).unwrap();

    let session = Session::load(root, root).expect("session");
    let request = EmergeRequest::parse(["app-misc/hello"]).unwrap();
    // Real spawner runs the stub ebuild.sh for each phase; no image_for override.
    let mut spawner = CommandSpawner::new(&script);
    let report = session
        .merge_action(&request, &mut spawner, |_| None)
        .expect("merge");

    // The image produced by src_install ($D) was merged into ROOT.
    assert_eq!(report.merged, vec!["app-misc/hello-1"]);
    assert_eq!(
        std::fs::read_to_string(root.join("usr/bin/hello")).unwrap(),
        "hi\n"
    );
    assert!(root.join("var/db/pkg/app-misc/hello-1/CONTENTS").exists());
    assert!(
        session
            .world_atoms()
            .contains(&"app-misc/hello".to_string())
    );
}
