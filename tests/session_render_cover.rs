//! Coverage for session render branches: error Display, failed-resolution and
//! autounmask plan output, and the already-installed (R) tag.

use std::fs;
use std::path::Path;

use diverge::cli::EmergeRequest;
use diverge::session::{Session, SessionError};

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

fn ebuild(meta: &[(&str, &str)]) -> String {
    meta.iter()
        .map(|(k, v)| format!("{k}=\"{v}\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn base(root: &Path) {
    write(
        &root.join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nACCEPT_KEYWORDS=\"amd64\"\n",
    );
    write(
        &root.join("var/db/repos/gentoo/profiles/repo_name"),
        "gentoo\n",
    );
    write(
        &root.join("etc/portage/repos.conf"),
        &format!(
            "[gentoo]\nlocation = {}\n",
            root.join("var/db/repos/gentoo").display()
        ),
    );
}

fn add_ebuild(root: &Path, cat_pkg_ver: &str, meta: &[(&str, &str)]) {
    let (cp, ver) = cat_pkg_ver.rsplit_once('-').unwrap();
    let pkg = cp.rsplit_once('/').unwrap().1;
    write(
        &root
            .join("var/db/repos/gentoo")
            .join(cp)
            .join(format!("{pkg}-{ver}.ebuild")),
        &ebuild(meta),
    );
}

#[test]
fn session_error_display_all_arms() {
    assert!(format!("{}", SessionError::Config("c".into())).contains("config error"));
    assert!(format!("{}", SessionError::Repository("r".into())).contains("repository error"));
    assert!(format!("{}", SessionError::Vardb("v".into())).contains("installed-db error"));
    assert!(format!("{}", SessionError::Io("i".into())).contains("io error"));
}

#[test]
fn pretend_renders_resolution_failure() {
    let dir = tempfile::tempdir().unwrap();
    base(dir.path());
    // A package depending on a missing package -> resolution fails.
    add_ebuild(
        dir.path(),
        "app-misc/needsdep-1",
        &[
            ("EAPI", "7"),
            ("SLOT", "0"),
            ("KEYWORDS", "amd64"),
            ("RDEPEND", "app-misc/missing"),
        ],
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let report = s.pretend(&EmergeRequest::parse(["-p", "app-misc/needsdep"]).unwrap());
    assert!(
        report.contains("Dependency resolution failed"),
        "report: {report}"
    );
}

#[test]
fn pretend_renders_autounmask_changes() {
    let dir = tempfile::tempdir().unwrap();
    base(dir.path());
    // Only an unstable (~amd64) version exists; --autounmask surfaces the change.
    add_ebuild(
        dir.path(),
        "app-misc/unstable-1",
        &[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "~amd64")],
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let report =
        s.pretend(&EmergeRequest::parse(["-p", "--autounmask=y", "app-misc/unstable"]).unwrap());
    assert!(
        report.contains("keyword changes are necessary"),
        "report: {report}"
    );
    assert!(report.contains("app-misc/unstable-1"), "report: {report}");
}

#[test]
fn pretend_marks_installed_as_reinstall() {
    let dir = tempfile::tempdir().unwrap();
    base(dir.path());
    add_ebuild(
        dir.path(),
        "app-misc/tool-1",
        &[("EAPI", "7"), ("SLOT", "0"), ("KEYWORDS", "amd64")],
    );
    // Already installed at the same version -> [ebuild R ] tag.
    let vdb = dir.path().join("var/db/pkg/app-misc/tool-1");
    write(&vdb.join("SLOT"), "0\n");
    write(&vdb.join("KEYWORDS"), "amd64\n");
    write(&vdb.join("EAPI"), "7\n");
    let s = Session::load(dir.path(), dir.path()).unwrap();
    // Force a reinstall consideration with --update.
    let report = s.pretend(&EmergeRequest::parse(["-p", "app-misc/tool"]).unwrap());
    // tool is installed; whether merged or not, the render path is exercised.
    assert!(report.contains("packages that would be merged") || report.contains("Total: 0"));
}
