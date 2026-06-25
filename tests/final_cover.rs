//! Final reachable-branch coverage: config tokenizer comments, nested
//! use_reduce special-append, repository file filtering, depgraph update path.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn getconfig_inline_comment_terminates_token() {
    use diverge::config::getconfig;
    let empty = HashMap::new();
    // A `#` after a value terminates the token (tokenizer comment branch).
    let c = getconfig("FOO=\"bar\" # trailing comment\n", true, &empty).unwrap();
    assert_eq!(c.get("FOO").map(String::as_str), Some("bar"));
    // A full-line comment is skipped.
    let c = getconfig("# whole line\nBAR=\"x\"\n", true, &empty).unwrap();
    assert_eq!(c.get("BAR").map(String::as_str), Some("x"));
}

#[test]
fn use_reduce_nested_single_group_collapse() {
    use diverge::dep::{UseReduceOptions, use_reduce};
    let opts = UseReduceOptions {
        uselist: &["foo", "bar"],
        ..UseReduceOptions::default()
    };
    // Nested conditionals + groups exercise the special-append/collapse paths.
    let r = use_reduce("foo? ( bar? ( ( dev-libs/A ) ) )", &opts).unwrap();
    assert!(format!("{r:?}").contains("dev-libs/A"));
    // || group nested inside a plain group.
    let r = use_reduce("( || ( dev-libs/A dev-libs/B ) )", &opts).unwrap();
    assert!(format!("{r:?}").contains("dev-libs/A"));
    // Single-atom group inside a conditional.
    let r = use_reduce("foo? ( ( dev-libs/C ) )", &opts).unwrap();
    assert!(format!("{r:?}").contains("dev-libs/C"));
}

#[test]
fn repository_skips_non_ebuild_and_mismatched_files() {
    use diverge::repository::Repository;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    write(&repo.join("profiles/repo_name"), "test\n");
    // A package dir with a non-ebuild file and a mismatched-prefix ebuild.
    write(
        &repo.join("dev-libs/A/A-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"x86\"\n",
    );
    write(&repo.join("dev-libs/A/metadata.xml"), "<x/>\n");
    write(&repo.join("dev-libs/A/Manifest"), "DIST foo 1 BLAKE2B ab\n");
    write(&repo.join("dev-libs/A/OTHER-9.ebuild"), "EAPI=\"7\"\n"); // wrong prefix
    let r = Repository::load(&repo).unwrap();
    // Only A-1 is loaded; the non-ebuild and mismatched files are skipped.
    assert_eq!(r.db.cpv_all(), vec!["dev-libs/A-1".to_string()]);
}

#[test]
fn depgraph_update_pulls_higher_version() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveParams, Resolver};

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

    let mut available = PackageDb::new();
    available.insert("app/main-1", pkg(&[("RDEPEND", "app/lib")]));
    available.insert("app/lib-1", pkg(&[]));
    available.insert("app/lib-2", pkg(&[]));
    let mut installed = PackageDb::new();
    installed.insert("app/lib-1", pkg(&[]));

    // --update --deep upgrades the installed lib-1 -> lib-2.
    let params = ResolveParams::default().with_update(true).with_deep(true);
    let outcome = Resolver::new(&available, &installed, params).resolve(&["app/main"]);
    assert!(
        outcome.mergelist.contains(&"app/lib-2".to_string()),
        "{:?}",
        outcome.mergelist
    );
}

#[test]
fn profile_read_optional_io_error() {
    use diverge::profile::StackedProfile;
    // Pointing make.defaults at a directory (not a file) yields a non-NotFound
    // read error, exercising the read_optional Io arm.
    let dir = tempfile::tempdir().unwrap();
    let prof = dir.path().join("p");
    fs::create_dir_all(prof.join("make.defaults")).unwrap(); // dir, not file
    let res = StackedProfile::from_dir(&prof);
    assert!(res.is_err(), "reading a dir as make.defaults should error");
}
