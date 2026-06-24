//! End-to-end integration: a fixture repository + profile drive resolution and
//! an isolated-root merge, proving the layers compose on one shared model.
//!
//! Flow: load a repo into a PackageDb (repository), stack a profile
//! (profile), resolve a request into a merge plan (depgraph), then merge a
//! built image into a temp root with CONFIG_PROTECT (executor) and update the
//! world file (sets). Nothing outside the tempdir is touched.

use std::fs;
use std::path::Path;

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};
use diverge::executor::config_protect::ConfigProtect;
use diverge::executor::{MergeTransaction, unmerge};
use diverge::profile::StackedProfile;
use diverge::repository::Repository;
use diverge::sets::WorldFile;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, content).expect("write");
}

fn ebuild(meta: &[(&str, &str)]) -> String {
    meta.iter()
        .map(|(k, v)| format!("{k}=\"{v}\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn fixture_repo_profile_resolve_and_merge() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path();

    // 1. A fixture ebuild repository: app-editor/nano depends on sys-libs/ncurses.
    let repo = base.join("repo");
    write(&repo.join("profiles/repo_name"), "test_repo\n");
    write(
        &repo.join("app-editor/nano/nano-7.2.ebuild"),
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

    // 2. A profile selecting the amd64 arch.
    let profile = base.join("profiles/default");
    write(
        &profile.join("make.defaults"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    let stacked = StackedProfile::from_dir(&profile).expect("load profile");
    let arch = stacked.variables.get("ARCH").cloned().unwrap_or_default();
    assert_eq!(arch, "amd64");

    // 3. Load the repo and resolve `app-editor/nano` against an empty install.
    let repository = Repository::load(&repo).expect("load repo");
    let installed = PackageDb::new();
    let params = ResolveParams::default().with_arch(arch);
    let resolver = Resolver::new(&repository.db, &installed, params);
    let outcome = resolver.resolve(&["app-editor/nano"]);
    assert!(outcome.is_success(), "resolve failed: {:?}", outcome.error);
    // ncurses (dependency) merges before nano.
    assert_eq!(
        outcome.mergelist,
        vec!["sys-libs/ncurses-6.4", "app-editor/nano-7.2"]
    );

    // 4. Merge a built image for nano into an isolated root.
    let image = base.join("image");
    write(&image.join("usr/bin/nano"), "#!/bin/sh\n");
    write(&image.join("etc/nanorc"), "set tabsize 4\n");
    let root = base.join("root");
    let protect = ConfigProtect::new(&["/etc"], &[]);
    let merged = MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge nano image");
    assert!(root.join("usr/bin/nano").exists());
    assert!(root.join("etc/nanorc").exists());

    // 5. Record nano in the world file.
    let mut world = WorldFile::default();
    assert!(world.add("app-editor/nano"));
    assert_eq!(world.render(), "app-editor/nano\n");

    // 6. Unmerge cleans the installed files back out.
    let result = unmerge(&root, &merged.contents).expect("unmerge nano");
    assert!(!root.join("usr/bin/nano").exists());
    assert!(result.removed.iter().any(|p| p == "usr/bin/nano"));
}

#[test]
fn config_protect_preserves_edits_on_reinstall() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path();
    let root = base.join("root");
    let protect = ConfigProtect::new(&["/etc"], &[]);

    // First install writes the config.
    let image1 = base.join("image1");
    write(&image1.join("etc/app.conf"), "default v1\n");
    MergeTransaction::new(&image1, &root, &protect)
        .run()
        .expect("first merge");
    assert_eq!(
        fs::read_to_string(root.join("etc/app.conf")).unwrap(),
        "default v1\n"
    );

    // Admin edits the live config.
    write(&root.join("etc/app.conf"), "admin tuned\n");

    // Reinstall ships a new default: admin edit preserved, new goes to ._cfg.
    let image2 = base.join("image2");
    write(&image2.join("etc/app.conf"), "default v2\n");
    MergeTransaction::new(&image2, &root, &protect)
        .run()
        .expect("second merge");
    assert_eq!(
        fs::read_to_string(root.join("etc/app.conf")).unwrap(),
        "admin tuned\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("etc/._cfg0000_app.conf")).unwrap(),
        "default v2\n"
    );
}
