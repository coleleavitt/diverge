//! Coverage for session config-loading branches and gpkg container.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use diverge::cli::EmergeRequest;
use diverge::gpkg::{Gpkg, GpkgError};
use diverge::session::Session;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

fn base_repo(root: &Path) {
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
}

#[test]
fn session_reads_make_conf_directory_fragments() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    // make.conf as a directory of fragments.
    write(
        &dir.path().join("etc/portage/make.conf/00-arch"),
        "ARCH=\"amd64\"\n",
    );
    write(
        &dir.path().join("etc/portage/make.conf/10-use"),
        "USE=\"foo bar\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    // ARCH default still applies since fragments are read.
    assert_eq!(s.arch(), "amd64");
}

#[test]
fn session_use_flags_apply_removals() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\nUSE=\"foo bar -bar baz\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let flags = s.use_flags();
    assert!(flags.contains(&"foo".to_string()));
    assert!(flags.contains(&"baz".to_string()));
    assert!(!flags.contains(&"bar".to_string()), "-bar removes bar");
}

#[test]
fn session_info_and_list_sets() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let info = s.info();
    assert!(info.contains("ARCH=amd64"));
    assert!(info.contains("Available packages:"));
    let sets = s.list_sets();
    assert!(sets.contains("world") && sets.contains("selected") && sets.contains("system"));
}

#[test]
fn session_search_empty_terms_lists_all() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let all = s.search(&[]);
    assert!(all.contains("app-misc/hello"));
}

#[test]
fn session_world_path_and_atoms_default_empty() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(s.world_path().ends_with("var/lib/portage/world"));
    assert!(s.world_atoms().is_empty());
}

#[test]
fn install_and_unmerge_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let image = dir.path().join("img");
    write(&image.join("usr/bin/hello"), "x\n");
    let r = s.install_image("app-misc/hello-1", &image, false).unwrap();
    assert!(dir.path().join("usr/bin/hello").exists());
    assert!(s.world_atoms().contains(&"app-misc/hello".to_string()));
    s.unmerge_package("app-misc/hello-1", &r.contents).unwrap();
    assert!(!dir.path().join("usr/bin/hello").exists());
}

#[test]
fn dispatch_version_and_moo() {
    let dir = tempfile::tempdir().unwrap();
    base_repo(dir.path());
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(
        s.dispatch(&EmergeRequest::parse(["--version"]).unwrap())
            .contains("diverge")
    );
    assert!(
        s.dispatch(&EmergeRequest::parse(["--moo"]).unwrap())
            .contains("moo")
    );
}

// ---- gpkg ----

fn meta() -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert("SLOT".to_string(), b"0".to_vec());
    m.insert("DEPEND".to_string(), b"d/B".to_vec());
    m
}

#[test]
fn gpkg_size_and_metadata_str() {
    let g = Gpkg::new(meta(), b"image".to_vec());
    assert!(g.size() > 0);
    assert_eq!(g.metadata_str("SLOT").as_deref(), Some("0"));
    assert_eq!(g.metadata_str("MISSING"), None);
}

#[test]
fn gpkg_decode_bad_magic_and_truncated() {
    assert!(matches!(
        Gpkg::decode(b"not a gpkg"),
        Err(GpkgError::Malformed(_))
    ));
    // Truncate a valid container.
    let g = Gpkg::new(meta(), b"img".to_vec());
    let enc = g.encode();
    let truncated = &enc[..enc.len() / 2];
    assert!(Gpkg::decode(truncated).is_err());
}

#[test]
fn gpkg_corruption_detected() {
    let g = Gpkg::new(meta(), b"payload".to_vec());
    let mut enc = g.encode();
    let i = enc.len() / 2;
    enc[i] ^= 0xff;
    assert!(Gpkg::decode(&enc).is_err());
}

#[test]
fn gpkg_signature_round_trip_and_verify() {
    let g = Gpkg::new(meta(), b"p".to_vec()).with_signature(b"SIG".to_vec());
    let d = Gpkg::decode(&g.encode()).unwrap();
    assert!(d.verify_signature(|_i, s| s == b"SIG"));
    assert!(!d.verify_signature(|_i, s| s == b"NO"));
    let unsigned = Gpkg::new(meta(), b"p".to_vec());
    assert!(!unsigned.verify_signature(|_, _| true));
    // Error Display.
    assert!(format!("{}", GpkgError::Malformed("x".into())).contains("malformed"));
}
