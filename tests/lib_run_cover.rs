//! Coverage for the top-level `diverge::{parse, run}` entry points and the
//! sync backend, driven against an isolated config root via env vars.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use diverge::sync::{LocalSync, SyncBackend, SyncConfig, SyncType};

// `run` reads process-global env (PORTAGE_CONFIGROOT/ROOT); serialize the tests
// that mutate it so they don't race.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

fn fixture(root: &Path) {
    let repo = root.join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
}

#[test]
fn parse_returns_request() {
    let req = diverge::parse(["-p", "app-misc/hello"]).unwrap();
    assert_eq!(req.action, diverge::cli::EmergeAction::Merge);
    assert!(req.options.pretend);
    // A bad option surfaces a CLI error.
    assert!(diverge::parse(["--nonsense-flag"]).is_err());
}

#[test]
fn run_pretend_against_isolated_root() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    // SAFETY: serialized by ENV_LOCK; we restore by removing afterwards.
    unsafe {
        std::env::set_var("PORTAGE_CONFIGROOT", dir.path());
        std::env::set_var("ROOT", dir.path());
    }
    // -p hello resolves and prints a plan; run returns Ok.
    let r = diverge::run(["-p", "app-misc/hello"]);
    assert!(r.is_ok(), "run failed: {:?}", r.err());

    // --version path.
    assert!(diverge::run(["--version"]).is_ok());
    // A search.
    assert!(diverge::run(["-s", "hello"]).is_ok());

    unsafe {
        std::env::remove_var("PORTAGE_CONFIGROOT");
        std::env::remove_var("ROOT");
    }
}

#[test]
fn run_surfaces_cli_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    let err = diverge::run(["--bogus-option-xyz"]);
    assert!(err.is_err());
    // RunError Display covers the Cli arm.
    let msg = format!("{}", err.unwrap_err());
    assert!(!msg.is_empty());
}

#[test]
fn sync_nested_tree_and_changes_reported() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");
    write(&src.join("a/b/c.txt"), "deep\n");
    write(&src.join("top.txt"), "t\n");
    let cfg = SyncConfig {
        name: "gentoo".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Rsync,
    };
    let mut backend = LocalSync;
    let out = backend.sync(&cfg).unwrap();
    assert!(out.updated);
    assert_eq!(out.changed_files.len(), 2);
    assert!(dest.join("a/b/c.txt").exists());

    // Changing a file is detected on the next sync.
    write(&src.join("top.txt"), "changed\n");
    let out2 = backend.sync(&cfg).unwrap();
    assert!(out2.updated);
    assert!(out2.changed_files.iter().any(|f| f.contains("top.txt")));
}
