//! Real ebuild-build integration: run an actual ebuild's `src_install` through
//! the bundled `EbuildSpawner` + install helpers, producing a real `$D` image,
//! then merge it into an isolated ROOT. Requires only `bash` + coreutils — no
//! compiler, no host Portage, and never touches the host filesystem.
//!
//! Reference: `research/portage/bin/phase-helpers.sh` (the install helpers).

#![cfg(unix)]

use std::collections::BTreeMap;

use diverge::cli::EmergeRequest;
use diverge::executor::ebuild_sh::EbuildSpawner;
use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseSpawner};
use diverge::session::Session;

use crate::fs_fixture::write;

/// Runs `func` of `ebuild` against a fresh build dir and returns the image dir.
fn run_phase(
    ebuild: &std::path::Path,
    build: &std::path::Path,
    phase: Phase,
) -> std::path::PathBuf {
    let dirs = BuildDirs::new(build.to_path_buf(), build);
    dirs.create().unwrap();
    let ctx = PhaseContext {
        ebuild: ebuild.to_path_buf(),
        cpv: "app-misc/hello-1".to_string(),
        eapi: "7".to_string(),
        root: build.join("root"),
        dirs: dirs.clone(),
        use_flags: vec![],
    };
    let mut env: BTreeMap<String, String> = ctx.environment(phase);
    // The helpers write under $D; make sure it exists.
    std::fs::create_dir_all(&dirs.image_dir).unwrap();
    env.insert(
        "D".to_string(),
        dirs.image_dir.to_string_lossy().into_owned(),
    );
    let mut spawner = EbuildSpawner::new();
    let outcome = spawner.run_phase(phase, &env);
    assert!(outcome.success, "src_install failed: {:?}", outcome.message);
    dirs.image_dir
}

#[test]
fn ebuild_src_install_populates_image_via_helpers() {
    let dir = tempfile::tempdir().unwrap();
    // A real ebuild whose src_install uses the bundled install helpers.
    let ebuild = dir.path().join("hello-1.ebuild");
    write(
        &ebuild,
        r#"EAPI=7
SLOT="0"
KEYWORDS="amd64"
src_install() {
    dobin "${T}/hello"
    insinto /etc
    doins "${T}/hello.conf"
    dosym hello /usr/bin/hi
    keepdir /var/lib/hello
}
"#,
    );
    // Source files the ebuild installs (placed in $T, the temp dir).
    let build = dir.path().join("build");
    let t = build.join("temp");
    write(&t.join("hello"), "#!/bin/sh\necho hi\n");
    write(&t.join("hello.conf"), "greeting=hi\n");

    let image = run_phase(&ebuild, &build, Phase::SrcInstall);

    // The helpers installed everything under $D.
    assert!(image.join("usr/bin/hello").exists(), "dobin");
    assert!(image.join("etc/hello.conf").exists(), "doins into /etc");
    assert!(image.join("usr/bin/hi").is_symlink(), "dosym");
    assert!(image.join("var/lib/hello/.keep").exists(), "keepdir");
    assert_eq!(
        std::fs::read_to_string(image.join("etc/hello.conf")).unwrap(),
        "greeting=hi\n"
    );
}

#[test]
fn merge_with_ebuild_spawner_builds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    // A repo ebuild that installs a binary from $T during src_install.
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        r#"EAPI=7
SLOT="0"
KEYWORDS="amd64"
src_install() {
    dodir /usr/bin
    printf '#!/bin/sh\necho hi\n' > "${D}/usr/bin/hello"
    chmod +x "${D}/usr/bin/hello"
}
"#,
    );
    // A real synced repo ships an md5-cache entry (the metadata source for
    // ebuilds whose KEYWORDS/etc. come via eclasses or function bodies the
    // direct parser can't read). The loader prefers it.
    write(
        &repo.join("metadata/md5-cache/app-misc/hello-1"),
        "EAPI=7\nSLOT=0\nKEYWORDS=amd64\n",
    );
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );

    let session = Session::load(root, root).unwrap();
    let request = EmergeRequest::parse(["app-misc/hello"]).unwrap();
    // The phase context's EBUILD must point at the repo ebuild so the spawner
    // can source it. merge_action builds <build>/ebuild as EBUILD by default,
    // so supply the real image via image_for is NOT used here — instead we
    // point the spawner at the repo by symlinking the build's ebuild path.
    // Simplest: use image_for=None and let the spawner build D; but merge_action
    // sets EBUILD=<build>/ebuild. So we run the build ourselves through a custom
    // spawner that resolves the repo ebuild from the cpv.
    struct RepoSpawner {
        repo: std::path::PathBuf,
    }
    impl PhaseSpawner for RepoSpawner {
        fn run_phase(
            &mut self,
            phase: Phase,
            env: &BTreeMap<String, String>,
        ) -> diverge::executor::PhaseOutcome {
            // Rewrite EBUILD to the real repo ebuild for this cpv.
            let cpv = env.get("CATEGORY").cloned().unwrap_or_default();
            let pf = env.get("PF").cloned().unwrap_or_default();
            let ebuild = self
                .repo
                .join(&cpv)
                .join(pf.split('-').next().unwrap_or(&pf))
                .join(format!("{pf}.ebuild"));
            let mut env2 = env.clone();
            env2.insert("EBUILD".to_string(), ebuild.to_string_lossy().into_owned());
            std::fs::create_dir_all(env2.get("D").cloned().unwrap_or_default()).ok();
            EbuildSpawner::new().run_phase(phase, &env2)
        }
    }
    let mut spawner = RepoSpawner { repo: repo.clone() };
    let report = session
        .merge_action(&request, &mut spawner, |_| None)
        .expect("merge");

    assert_eq!(report.merged, vec!["app-misc/hello-1"]);
    assert!(
        root.join("usr/bin/hello").exists(),
        "built binary merged into ROOT"
    );
    assert!(root.join("var/db/pkg/app-misc/hello-1/CONTENTS").exists());
}
