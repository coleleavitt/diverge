//! Coverage for the top-level `diverge::{parse, run}` entry points and the
//! sync backend, driven against an isolated config root via env vars.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use diverge::sync::{LocalSync, SyncBackend, SyncConfig, SyncType};

// `run` reads process-global env (PORTAGE_CONFIGROOT/ROOT); serialize the tests
// that mutate it so they don't race. Only `diverge::run` reads these vars and
// only this test binary sets them, so a process-local mutex is sufficient.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that points the roots at `dir` and unconditionally restores
/// (removes) them on drop — even if the test panics in between.
struct RootEnv;
impl RootEnv {
    fn set(dir: &Path) -> Self {
        // SAFETY: serialized by ENV_LOCK; no other reader runs concurrently.
        unsafe {
            std::env::set_var("PORTAGE_CONFIGROOT", dir);
            std::env::set_var("ROOT", dir);
        }
        RootEnv
    }
}
impl Drop for RootEnv {
    fn drop(&mut self) {
        // SAFETY: same serialization invariant as `set`.
        unsafe {
            std::env::remove_var("PORTAGE_CONFIGROOT");
            std::env::remove_var("ROOT");
        }
    }
}

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
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let _env = RootEnv::set(dir.path());

    // -p hello resolves and prints a plan; run returns Ok.
    let r = diverge::run(["-p", "app-misc/hello"]);
    assert!(r.is_ok(), "run failed: {:?}", r.err());
    // --version path.
    assert!(diverge::run(["--version"]).is_ok());
    // A search.
    assert!(diverge::run(["-s", "hello"]).is_ok());
}

#[test]
fn run_surfaces_cli_error() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let err = diverge::run(["--bogus-option-xyz"]);
    assert!(err.is_err());
    // RunError Display covers the Cli arm.
    let msg = format!("{}", err.unwrap_err());
    assert!(!msg.is_empty());
}

#[test]
fn run_exit_codes_match_emerge() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // No arguments: usage banner, exit 1 (like emerge).
    let no_args: [&str; 0] = [];
    assert_eq!(diverge::run(no_args).ok(), Some(1));
    // --help / -h: usage banner, exit 0.
    assert_eq!(diverge::run(["--help"]).ok(), Some(0));
    assert_eq!(diverge::run(["-h"]).ok(), Some(0));
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
