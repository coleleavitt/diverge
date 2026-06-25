//! Fault-injection coverage of the I/O error arms in merge/unmerge/vardb/
//! session, by making a destination directory read-only so writes fail.
//!
//! Skips gracefully when running as root (where mode bits don't block writes).

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use diverge::dbapi::PackageMetadata;
use diverge::executor::config_protect::ConfigProtect;
use diverge::executor::merge::ContentEntry;
use diverge::executor::{MergeError, MergeTransaction, unmerge};
use diverge::vardb;

/// Probes whether mode bits actually block writes here (false when running as
/// root, where 0o500 dirs are still writable, so those assertions are skipped).
fn readonly_blocks_writes() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let ro = dir.path().join("ro");
    fs::create_dir_all(&ro).unwrap();
    let mut perm = fs::metadata(&ro).unwrap().permissions();
    perm.set_mode(0o500);
    fs::set_permissions(&ro, perm).unwrap();
    let blocked = fs::write(ro.join("probe"), "x").is_err();
    let mut perm = fs::metadata(&ro).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&ro, perm).unwrap();
    blocked
}

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

fn set_readonly(dir: &Path) {
    let mut perm = fs::metadata(dir).unwrap().permissions();
    perm.set_mode(0o500); // r-x: cannot create entries
    fs::set_permissions(dir, perm).unwrap();
}

fn restore(dir: &Path) {
    let mut perm = fs::metadata(dir).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(dir, perm).unwrap();
}

#[test]
fn merge_into_readonly_root_errors() {
    if !readonly_blocks_writes() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("img");
    write(&image.join("usr/bin/tool"), "x\n");
    let root = dir.path().join("root");
    fs::create_dir_all(&root).unwrap();
    set_readonly(&root);

    let protect = ConfigProtect::new(&["/etc"], &[]);
    let result = MergeTransaction::new(&image, &root, &protect).run();
    restore(&root);
    assert!(
        matches!(result, Err(MergeError::Io(_))),
        "expected Io error"
    );
    // Exercise MergeError::Io Display.
    if let Err(e) = result {
        assert!(!format!("{e}").is_empty());
    }
}

#[test]
fn vardb_record_into_readonly_errors() {
    if !readonly_blocks_writes() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let vdb = dir.path().join("pkg");
    fs::create_dir_all(&vdb).unwrap();
    set_readonly(&vdb);

    let meta = PackageMetadata {
        slot: Some("0".to_string()),
        sub_slot: None,
        repo: None,
        eapi: Some("7".to_string()),
        iuse: vec![],
        use_enabled: vec![],
        keywords: vec![],
        deps: Default::default(),
    };
    let res = vardb::record_install(&vdb, "cat/p-1", &meta, &[]);
    restore(&vdb);
    assert!(res.is_err());
    if let Err(e) = res {
        assert!(!format!("{e}").is_empty()); // VardbError::Io Display
    }
}

#[test]
fn vardb_record_invalid_cpv_errors() {
    let dir = tempfile::tempdir().unwrap();
    let meta = PackageMetadata::default();
    // No '/' in cpv -> Io("invalid cpv").
    assert!(vardb::record_install(dir.path(), "nocategory", &meta, &[]).is_err());
    assert!(vardb::remove_install(dir.path(), "nocategory").is_err());
}

#[test]
fn unmerge_from_readonly_dir_surfaces_error() {
    if !readonly_blocks_writes() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    // Create a file then make its parent read-only so removal fails.
    write(&root.join("usr/bin/tool"), "x\n");
    let bindir = root.join("usr/bin");
    set_readonly(&bindir);

    let contents = vec![ContentEntry::File {
        path: "usr/bin/tool".to_string(),
        protected: false,
    }];
    let res = unmerge(&root, &contents);
    restore(&bindir);
    // Removal of a file in a read-only dir fails with an Io error.
    assert!(res.is_err());
}
