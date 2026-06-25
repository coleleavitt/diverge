//! Coverage finisher: session depclean sets, required-use flags operators,
//! manifest type rendering, merge symlink replace, dep DNF deeper paths.

use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn session_depclean_with_world_set_arg() {
    use diverge::cli::EmergeRequest;
    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &dir.path().join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    // Two installed packages; world protects one.
    write(&dir.path().join("var/db/pkg/d/keep-1/SLOT"), "0\n");
    write(&dir.path().join("var/db/pkg/d/keep-1/EAPI"), "7\n");
    write(&dir.path().join("var/db/pkg/d/orphan-1/SLOT"), "0\n");
    write(&dir.path().join("var/db/pkg/d/orphan-1/EAPI"), "7\n");
    write(&dir.path().join("var/lib/portage/world"), "d/keep\n");

    let s = Session::load(dir.path(), dir.path()).unwrap();
    // --depclean @world exercises the set-name branch (323-325).
    let req = EmergeRequest::parse(["--depclean", "@world"]).unwrap();
    let report = s.depclean_report(&req);
    assert!(report.contains("d/orphan-1"));
    assert!(!report.contains("d/keep-1"));
}

#[test]
fn required_use_flags_operators_and_conditionals() {
    use diverge::matching::get_required_use_flags;
    let flags = |s: &str| {
        let mut v: Vec<String> = get_required_use_flags(s).unwrap().into_iter().collect();
        v.sort();
        v
    };
    // operators ^^ ?? || and ?-conditionals (lines 518-533).
    assert_eq!(flags("^^ ( a b )"), vec!["a", "b"]);
    assert_eq!(flags("?? ( a b )"), vec!["a", "b"]);
    assert_eq!(flags("|| ( a b )"), vec!["a", "b"]);
    assert_eq!(flags("c? ( d )"), vec!["c", "d"]);
    assert_eq!(flags("!e? ( f )"), vec!["e", "f"]);
    assert_eq!(flags("a b c"), vec!["a", "b", "c"]);
    // malformed: operator needing a bracket then a bare flag.
    assert!(get_required_use_flags("|| a").is_err());
    assert!(get_required_use_flags("a? b").is_err());
}

#[test]
fn manifest_type_rendering_all_kinds() {
    use diverge::manifest::Manifest;
    // AUX, MISC, EBUILD, DIST all round-trip through render (as_str arms).
    let content = "\
AUX patch.diff 10 SHA512 a BLAKE2B b
MISC metadata.xml 20 SHA512 c BLAKE2B d
EBUILD foo-1.ebuild 30 SHA512 e BLAKE2B f
DIST foo-1.tar.gz 40 SHA512 g BLAKE2B h
";
    let m = Manifest::parse(content).unwrap();
    let rendered = m.render();
    assert!(rendered.contains("AUX patch.diff"));
    assert!(rendered.contains("MISC metadata.xml"));
    assert!(rendered.contains("EBUILD foo-1.ebuild"));
    assert!(rendered.contains("DIST foo-1.tar.gz"));
}

#[test]
fn merge_replaces_existing_symlink() {
    use diverge::executor::MergeTransaction;
    use diverge::executor::config_protect::ConfigProtect;
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("img");
    let root = dir.path().join("root");
    // Pre-existing symlink at the destination that the merge must replace.
    fs::create_dir_all(root.join("usr/lib")).unwrap();
    std::os::unix::fs::symlink("old-target", root.join("usr/lib/libfoo.so")).unwrap();
    write(&image.join("usr/lib/libfoo.so.1"), "lib\n");
    std::os::unix::fs::symlink("libfoo.so.1", image.join("usr/lib/libfoo.so")).unwrap();

    let protect = ConfigProtect::new(&["/etc"], &[]);
    MergeTransaction::new(&image, &root, &protect)
        .run()
        .unwrap();
    // The symlink now points at the new target.
    let link = root.join("usr/lib/libfoo.so");
    assert!(link.is_symlink());
    assert_eq!(
        fs::read_link(&link).unwrap().to_string_lossy(),
        "libfoo.so.1"
    );
}

#[test]
fn use_reduce_deep_dnf_paths() {
    use diverge::dep::{UseReduceOptions, use_reduce};
    let opts = UseReduceOptions {
        uselist: &["a", "b", "c"],
        ..UseReduceOptions::default()
    };
    // Deeply nested mix of conditionals, groups and || that drives the
    // special-append and single-inner-group collapse paths.
    let cases = [
        "a? ( b? ( ( dev/x ) ) )",
        "|| ( ( dev/x dev/y ) dev/z )",
        "a? ( || ( dev/x ( dev/y dev/z ) ) )",
        "( ( ( dev/x ) ) )",
        "a? ( dev/x ) b? ( dev/y ) c? ( dev/z )",
    ];
    for c in cases {
        let r = use_reduce(c, &opts);
        assert!(r.is_ok(), "{c}: {r:?}");
    }
}
