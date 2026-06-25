//! Covers `main.rs` by spawning the actual built binary. cargo-llvm-cov merges
//! the subprocess's profile data, so the binary entry/exit paths count.

use std::fs;
use std::path::Path;
use std::process::Command;

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

fn run(args: &[&str], root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_diverge"))
        .args(args)
        .env("PORTAGE_CONFIGROOT", root)
        .env("ROOT", root)
        .output()
        .expect("spawn diverge")
}

#[test]
fn binary_pretend_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let out = run(&["-p", "app-misc/hello"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("app-misc/hello-1"), "stdout: {stdout}");
}

#[test]
fn binary_version_and_search() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    assert!(run(&["--version"], dir.path()).status.success());
    assert!(run(&["-s", "hello"], dir.path()).status.success());
}

#[test]
fn binary_cli_error_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    // An unknown option makes parse fail -> main prints to stderr and exits 1.
    let out = run(&["--definitely-not-an-option"], dir.path());
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("diverge:"));
}
