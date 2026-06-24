//! Integration tests for the merge/unmerge runtime, against isolated temp
//! roots only (never touches the real filesystem outside the tempdir).
//!
//! Reference:
//! - `research/portage/lib/portage/dbapi/vartree.py` (merge, CONFIG_PROTECT,
//!   collision-protect, unmerge)
//! - `research/portage/lib/portage/util/__init__.py` (ConfigProtect)
//! - `research/portage/lib/portage/tests/emerge/test_config_protect.py`

use std::fs;

use diverge::executor::config_protect::ConfigProtect;
use diverge::executor::{ContentEntry, MergeError, MergeTransaction, unmerge};

use crate::fs_fixture::write;

#[test]
fn config_protect_is_protected_respects_mask() {
    let cp = ConfigProtect::new(&["/etc"], &["/etc/foo"]);
    assert!(cp.is_protected("/etc/bar.conf"));
    // /etc/foo is masked back to unprotected.
    assert!(!cp.is_protected("/etc/foo/baz.conf"));
    // A path outside CONFIG_PROTECT is not protected.
    assert!(!cp.is_protected("/usr/bin/tool"));
    // /etc/foobaz is not under /etc/foo (boundary check).
    assert!(cp.is_protected("/etc/foobaz"));
}

#[test]
fn protect_filename_increments_counter() {
    // No existing dest: returns the plain name.
    assert_eq!(
        ConfigProtect::protect_filename("bar.conf", &[], false),
        "bar.conf"
    );
    // Dest exists, no prior ._cfg: first protected name is 0000.
    assert_eq!(
        ConfigProtect::protect_filename("bar.conf", &["bar.conf".to_string()], true),
        "._cfg0000_bar.conf"
    );
    // Existing ._cfg0000_: next is 0001.
    let siblings = vec!["bar.conf".to_string(), "._cfg0000_bar.conf".to_string()];
    assert_eq!(
        ConfigProtect::protect_filename("bar.conf", &siblings, true),
        "._cfg0001_bar.conf"
    );
}

#[test]
fn merge_installs_image_into_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");
    write(&image.join("usr/bin/tool"), "#!/bin/sh\n");
    write(&image.join("usr/share/doc/readme"), "hello\n");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    let result = MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge succeeds");

    assert!(root.join("usr/bin/tool").exists());
    assert!(root.join("usr/share/doc/readme").exists());
    let mut paths = result.installed_paths();
    paths.sort();
    assert!(paths.contains(&"usr/bin/tool".to_string()));
    assert!(paths.contains(&"usr/share/doc/readme".to_string()));
}

#[test]
fn merge_redirects_protected_existing_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");

    // An admin-edited config already in the root.
    write(&root.join("etc/app.conf"), "admin-edited\n");
    // The package ships a new version of it.
    write(&image.join("etc/app.conf"), "package-default\n");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    let result = MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge succeeds");

    // Original is preserved; new version is written to ._cfg0000_app.conf.
    assert_eq!(
        fs::read_to_string(root.join("etc/app.conf")).unwrap(),
        "admin-edited\n"
    );
    let protected = root.join("etc/._cfg0000_app.conf");
    assert!(protected.exists(), "protected file written");
    assert_eq!(fs::read_to_string(&protected).unwrap(), "package-default\n");

    // CONTENTS records the protected redirect.
    assert!(result.contents.iter().any(|e| matches!(
        e,
        ContentEntry::File { path, protected: true } if path.contains("._cfg0000_app.conf")
    )));
}

#[test]
fn merge_overwrites_unprotected_existing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");
    write(&root.join("usr/bin/tool"), "old\n");
    write(&image.join("usr/bin/tool"), "new\n");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge succeeds");
    // Non-config file is overwritten in place.
    assert_eq!(
        fs::read_to_string(root.join("usr/bin/tool")).unwrap(),
        "new\n"
    );
}

#[test]
fn merge_detects_collision_with_other_package() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");
    write(&image.join("usr/bin/tool"), "mine\n");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    let err = MergeTransaction::new(&image, &root, &protect)
        .with_existing_owner("app-a/other-1", &["usr/bin/tool"])
        .run()
        .expect_err("collision must error");
    match err {
        MergeError::Collision { path, owner } => {
            assert_eq!(path, "usr/bin/tool");
            assert_eq!(owner, "app-a/other-1");
        }
        other => panic!("expected collision, got {other}"),
    }
}

#[test]
fn merge_reproduces_symlinks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");
    write(&image.join("usr/lib/libfoo.so.1"), "lib\n");
    std::os::unix::fs::symlink("libfoo.so.1", image.join("usr/lib/libfoo.so"))
        .expect("create symlink");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge succeeds");

    let link = root.join("usr/lib/libfoo.so");
    assert!(link.is_symlink(), "symlink reproduced");
    assert_eq!(
        fs::read_link(&link).unwrap().to_string_lossy(),
        "libfoo.so.1"
    );
}

#[test]
fn unmerge_removes_files_and_empty_dirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = dir.path().join("image");
    let root = dir.path().join("root");
    write(&image.join("usr/bin/tool"), "x\n");
    write(&image.join("usr/share/app/data"), "y\n");

    let protect = ConfigProtect::new(&["/etc"], &[]);
    let merged = MergeTransaction::new(&image, &root, &protect)
        .run()
        .expect("merge");

    // Drop an unrelated file into a shared dir so it is kept.
    write(&root.join("usr/bin/other"), "keep\n");

    let result = unmerge(&root, &merged.contents).expect("unmerge");
    // Our files are gone.
    assert!(!root.join("usr/bin/tool").exists());
    assert!(!root.join("usr/share/app/data").exists());
    // usr/share/app became empty and was removed; usr/bin kept (has `other`).
    assert!(!root.join("usr/share/app").exists());
    assert!(root.join("usr/bin").exists());
    assert!(result.kept_dirs.iter().any(|d| d == "usr/bin"));
}

#[test]
fn merge_requires_existing_image() {
    let dir = tempfile::tempdir().expect("tempdir");
    let protect = ConfigProtect::new(&[], &[]);
    let err = MergeTransaction::new(dir.path().join("nope"), dir.path().join("root"), &protect)
        .run()
        .expect_err("missing image errors");
    assert!(matches!(err, MergeError::MissingImage(_)));
}
