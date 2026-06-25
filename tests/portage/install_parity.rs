//! End-to-end install/unmerge against an isolated ROOT: merge a built image
//! into the root, record the VDB entry + CONTENTS, update the world file, then
//! unmerge it back out. Uses only a tempdir — never the host `/`.
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/vartree.py` (`dblink.merge`/`unmerge`)
//! - `research/portage/lib/_emerge/Scheduler.py` (merge orchestration)

use diverge::session::Session;

use crate::fs_fixture::write;

fn ebuild(meta: &[(&str, &str)]) -> String {
    meta.iter()
        .map(|(k, v)| format!("{k}=\"{v}\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Builds a config root with one available package and an empty install root.
fn fixture_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        &ebuild(&[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")]),
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    dir
}

#[test]
fn install_image_merges_records_vdb_and_world() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");

    // A built image for app-misc/hello.
    let image = dir.path().join("image");
    write(&image.join("usr/bin/hello"), "#!/bin/sh\necho hello\n");
    write(&image.join("etc/hello.conf"), "greeting=hi\n");

    let result = session
        .install_image("app-misc/hello-1", &image, false)
        .expect("install");

    // Files landed in the root.
    assert!(dir.path().join("usr/bin/hello").exists());
    assert!(dir.path().join("etc/hello.conf").exists());
    assert!(
        result
            .installed_paths()
            .iter()
            .any(|p| p == "usr/bin/hello")
    );

    // VDB entry recorded with CONTENTS + metadata.
    let vdb = dir.path().join("var/db/pkg/app-misc/hello-1");
    assert!(vdb.join("CONTENTS").exists(), "CONTENTS written");
    assert!(vdb.join("SLOT").exists(), "SLOT written");
    let contents = std::fs::read_to_string(vdb.join("CONTENTS")).unwrap();
    assert!(
        contents.contains("obj /usr/bin/hello"),
        "CONTENTS: {contents}"
    );

    // World file updated with the cp (not oneshot).
    let world = std::fs::read_to_string(session.world_path()).unwrap();
    assert!(world.contains("app-misc/hello"), "world: {world}");
}

#[test]
fn oneshot_install_does_not_touch_world() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");
    let image = dir.path().join("image");
    write(&image.join("usr/bin/hello"), "x\n");

    session
        .install_image("app-misc/hello-1", &image, true)
        .expect("oneshot install");

    // Oneshot: no world file written.
    assert!(
        !session.world_path().exists(),
        "world file should not be created for --oneshot"
    );
    // But the VDB entry still exists.
    assert!(
        dir.path()
            .join("var/db/pkg/app-misc/hello-1/CONTENTS")
            .exists()
    );
}

#[test]
fn unmerge_removes_files_and_vdb_entry() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");
    let image = dir.path().join("image");
    write(&image.join("usr/bin/hello"), "x\n");
    write(&image.join("usr/share/hello/data"), "y\n");

    let result = session
        .install_image("app-misc/hello-1", &image, false)
        .expect("install");
    assert!(dir.path().join("usr/bin/hello").exists());

    // Unmerge using the recorded contents.
    session
        .unmerge_package("app-misc/hello-1", &result.contents)
        .expect("unmerge");

    assert!(!dir.path().join("usr/bin/hello").exists(), "file removed");
    assert!(
        !dir.path().join("usr/share/hello").exists(),
        "empty dir removed"
    );
    assert!(
        !dir.path().join("var/db/pkg/app-misc/hello-1").exists(),
        "VDB entry removed"
    );
}

#[test]
fn config_protect_preserves_config_on_install() {
    let dir = fixture_root();
    // Default CONFIG_PROTECT is /etc.
    let session = Session::load(dir.path(), dir.path()).expect("session");

    // Pre-existing admin-edited config in the root.
    write(&dir.path().join("etc/hello.conf"), "admin-edited\n");

    let image = dir.path().join("image");
    write(&image.join("etc/hello.conf"), "package-default\n");

    session
        .install_image("app-misc/hello-1", &image, true)
        .expect("install");

    // Admin edit preserved; the new version goes to ._cfg0000_.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("etc/hello.conf")).unwrap(),
        "admin-edited\n"
    );
    assert!(
        dir.path().join("etc/._cfg0000_hello.conf").exists(),
        "protected config written to ._cfg name"
    );
}

#[test]
fn reload_session_sees_installed_package() {
    let dir = fixture_root();
    let session = Session::load(dir.path(), dir.path()).expect("session");
    let image = dir.path().join("image");
    write(&image.join("usr/bin/hello"), "x\n");
    session
        .install_image("app-misc/hello-1", &image, false)
        .expect("install");

    // A freshly-loaded session reads the just-recorded VDB entry as installed.
    let reloaded = Session::load(dir.path(), dir.path()).expect("reload");
    assert!(
        !reloaded
            .installed
            .match_str("app-misc/hello")
            .unwrap()
            .is_empty(),
        "installed package visible after reload"
    );
}
