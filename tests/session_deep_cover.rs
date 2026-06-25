//! Deep session config-loading coverage: make.conf-as-directory fragments,
//! PORTDIR fallback, malformed-repo skip, plus depgraph cycle/EqualGlob.

use std::fs;
use std::path::Path;

use diverge::session::Session;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

fn repo_with_pkg(root: &Path, loc: &str) {
    let repo = root.join(loc);
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app-misc/hello/hello-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    );
}

#[test]
fn session_make_conf_directory_fragments_merge() {
    let dir = tempfile::tempdir().unwrap();
    repo_with_pkg(dir.path(), "var/db/repos/gentoo");
    write(
        &dir.path().join("etc/portage/repos.conf"),
        &format!(
            "[gentoo]\nlocation = {}\n",
            dir.path().join("var/db/repos/gentoo").display()
        ),
    );
    // make.conf is a DIRECTORY of fragments (00-*, 10-*) -> the dir branch.
    write(
        &dir.path().join("etc/portage/make.conf/00-base"),
        "ARCH=\"amd64\"\n",
    );
    write(
        &dir.path().join("etc/portage/make.conf/10-kw"),
        "ACCEPT_KEYWORDS=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert_eq!(s.arch(), "amd64");
    assert!(s.accept_keywords().contains(&"amd64".to_string()));
    assert!(!s.available.is_empty());
}

#[test]
fn session_portdir_fallback_when_no_repos_conf() {
    let dir = tempfile::tempdir().unwrap();
    // No repos.conf; PORTDIR points at the tree.
    let portdir = dir.path().join("custom/tree");
    repo_with_pkg(dir.path(), "custom/tree");
    write(
        &dir.path().join("etc/portage/make.conf"),
        &format!("ARCH=\"amd64\"\nPORTDIR=\"{}\"\n", portdir.display()),
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(
        !s.available.match_str("app-misc/hello").unwrap().is_empty(),
        "PORTDIR fallback should load the tree"
    );
}

#[test]
fn session_conventional_location_fallback() {
    let dir = tempfile::tempdir().unwrap();
    // No repos.conf, no PORTDIR; conventional var/db/repos/gentoo is used.
    repo_with_pkg(dir.path(), "var/db/repos/gentoo");
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(!s.available.match_str("app-misc/hello").unwrap().is_empty());
}

#[test]
fn session_skips_malformed_repo() {
    let dir = tempfile::tempdir().unwrap();
    // A repos.conf pointing at a dir with no profiles/repo_name -> load fails,
    // and the session skips it rather than erroring.
    let bad = dir.path().join("bad-repo");
    fs::create_dir_all(bad.join("dev-libs/A")).unwrap();
    write(
        &dir.path().join("etc/portage/repos.conf"),
        &format!("[bad]\nlocation = {}\n", bad.display()),
    );
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(s.available.is_empty(), "malformed repo skipped");
}

#[test]
fn depgraph_cycle_and_equalglob() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

    fn pkg(deps: &[(&str, &str)]) -> PackageMetadata {
        let mut m = PackageMetadata {
            slot: Some("0".into()),
            sub_slot: None,
            repo: Some("r".into()),
            eapi: Some("7".into()),
            iuse: vec![],
            use_enabled: vec![],
            keywords: vec!["x86".into()],
            deps: Default::default(),
        };
        for (k, v) in deps {
            m.deps.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    // Build-time cycle A<->B via DEPEND.
    let mut available = PackageDb::new();
    available.insert("c/A-1", pkg(&[("DEPEND", "c/B")]));
    available.insert("c/B-1", pkg(&[("DEPEND", "c/A")]));
    let outcome =
        Resolver::new(&available, &PackageDb::new(), ResolveParams::default()).resolve(&["c/A"]);
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::CircularDependency(_))
    ));

    // =glob target via the resolver selection path.
    let mut av = PackageDb::new();
    av.insert("c/X-1.2", pkg(&[]));
    av.insert("c/X-2.0", pkg(&[]));
    let outcome =
        Resolver::new(&av, &PackageDb::new(), ResolveParams::default()).resolve(&["=c/X-1*"]);
    assert!(outcome.is_success());
    assert_eq!(outcome.mergelist, vec!["c/X-1.2"]);
}
