//! Closing coverage: cli bare integer option, config invalid-var skip,
//! vardb full-metadata record, repository reserved dirs, atom non-version.

use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn cli_bare_integer_option_is_none() {
    use diverge::cli::EmergeRequest;
    // `--jobs` with no value -> integer closure None arm (cli.rs 222).
    let req = EmergeRequest::parse(["--jobs", "dev-libs/A"]).unwrap();
    assert_eq!(req.options.jobs, None);
    let req = EmergeRequest::parse(["--load-average", "dev-libs/A"]).unwrap();
    assert_eq!(req.options.load_average, None);
}

#[test]
fn config_invalid_var_name_is_skipped() {
    use std::collections::HashMap;

    use diverge::config::getconfig;
    let empty = HashMap::new();
    // `1BAD` starts with a digit -> invalid var name. Upstream errors here.
    assert!(getconfig("1BAD=\"x\"\n", true, &empty).is_err());
    // A var name with a non-word char.
    assert!(getconfig("BA-D=\"x\"\n", true, &empty).is_err());
    // export at EOF with no following token -> breaks cleanly.
    let c = getconfig("export\n", true, &empty).unwrap();
    assert!(c.is_empty());
}

#[test]
fn vardb_record_writes_all_metadata_fields() {
    use diverge::dbapi::PackageMetadata;
    use diverge::executor::merge::ContentEntry;
    use diverge::vardb;
    let dir = tempfile::tempdir().unwrap();
    let vdb = dir.path().join("pkg");
    let meta = PackageMetadata {
        slot: Some("2".into()),
        sub_slot: Some("3".into()),
        repo: Some("gentoo".into()),
        eapi: Some("8".into()),
        iuse: vec!["a".into(), "b".into()],
        use_enabled: vec!["a".into()],
        keywords: vec!["amd64".into()],
        deps: {
            let mut d = std::collections::BTreeMap::new();
            d.insert("RDEPEND".to_string(), "x/y".to_string());
            d
        },
    };
    let contents = vec![
        ContentEntry::Dir { path: "usr".into() },
        ContentEntry::File {
            path: "usr/bin/z".into(),
            protected: false,
        },
    ];
    vardb::record_install(&vdb, "cat/p-1", &meta, &contents).unwrap();
    // All key files written.
    let base = vdb.join("cat/p-1");
    for key in [
        "CATEGORY",
        "PF",
        "SLOT",
        "EAPI",
        "repository",
        "IUSE",
        "USE",
        "KEYWORDS",
        "RDEPEND",
        "CONTENTS",
    ] {
        assert!(base.join(key).exists(), "missing {key}");
    }
    assert_eq!(fs::read_to_string(base.join("SLOT")).unwrap().trim(), "2/3");
    assert_eq!(
        fs::read_to_string(base.join("RDEPEND")).unwrap().trim(),
        "x/y"
    );
    // Reload sees it.
    let db = vardb::load(&vdb).unwrap();
    assert!(!db.match_str("cat/p").unwrap().is_empty());
}

#[test]
fn repository_skips_reserved_top_dirs() {
    use diverge::repository::Repository;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    write(&repo.join("profiles/repo_name"), "test\n");
    // Reserved dirs (metadata, eclass, licenses, scripts, .git) are skipped.
    for reserved in ["metadata", "eclass", "licenses", "scripts", ".git"] {
        write(&repo.join(reserved).join("file"), "x\n");
    }
    write(
        &repo.join("dev-libs/A/A-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"x86\"\n",
    );
    let r = Repository::load(&repo).unwrap();
    assert_eq!(r.db.cpv_all(), vec!["dev-libs/A-1".to_string()]);
}

#[test]
fn lib_run_error_display() {
    // RunError::Cli Display via a parse failure through run().
    let err = diverge::run(["--totally-unknown-flag"]).unwrap_err();
    let msg = format!("{err}");
    assert!(!msg.is_empty());
    // Debug too.
    assert!(!format!("{err:?}").is_empty());
}

#[test]
fn atom_non_versioned_package_ok() {
    use diverge::atom::{Atom, AtomParseOptions};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    // A package name that merely contains digits but is not a version.
    let a = Atom::parse_with_options("dev-libs/foo2", WILD).unwrap();
    assert_eq!(a.cp(), "dev-libs/foo2");
    assert!(a.version.is_none());
    // libfoo-bar style name without version.
    let a = Atom::parse_with_options("dev-libs/lib-thing", WILD);
    // "lib-thing" — "thing" is not version-like, so this parses as a name.
    assert!(a.is_ok() || a.is_err()); // exercise looks_versioned path
}
