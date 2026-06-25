//! Coverage for profile / repository / config / util branches.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use diverge::config::{ParseError, getconfig, varexpand};
use diverge::profile::{ProfileStack, StackedProfile};
use diverge::repository::Repository;
use diverge::util::{grabdict, grabfile, normalize_path, stack_dicts, stack_lists};

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn profile_deep_chain_and_multiple_parents() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(&r.join("base/make.defaults"), "USE=\"a\"\n");
    write(&r.join("mid/parent"), "../base\n");
    write(&r.join("mid/make.defaults"), "USE=\"b\"\n");
    write(&r.join("extra/make.defaults"), "USE=\"e\"\n");
    // leaf has two parents.
    write(&r.join("leaf/parent"), "../mid\n../extra\n");
    write(&r.join("leaf/make.defaults"), "USE=\"-b c\"\n");
    let stack = ProfileStack::resolve(r.join("leaf")).unwrap();
    let names: Vec<String> = stack
        .profiles
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.last().unwrap(), "leaf");
    assert!(names.contains(&"base".to_string()));
    assert!(names.contains(&"extra".to_string()));

    let prof = StackedProfile::from_dir(r.join("leaf")).unwrap();
    let use_tokens = prof.incremental_tokens("USE");
    assert!(use_tokens.contains(&"a".to_string()));
    assert!(use_tokens.contains(&"c".to_string()));
    assert!(!use_tokens.contains(&"b".to_string()));
}

#[test]
fn profile_errors() {
    let dir = tempfile::tempdir().unwrap();
    let leaf = dir.path().join("leaf");
    write(&leaf.join("parent"), "../missing\n");
    assert!(ProfileStack::resolve(&leaf).is_err());

    let leaf2 = dir.path().join("leaf2");
    write(&leaf2.join("parent"), "\n# comment only\n");
    assert!(ProfileStack::resolve(&leaf2).is_err());

    assert!(ProfileStack::resolve(dir.path().join("does-not-exist")).is_err());
}

#[test]
fn profile_package_use_mask_stacking() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(&r.join("base/package.use"), "dev-libs/A foo\n");
    write(&r.join("base/package.mask"), "dev-libs/evil\n");
    write(&r.join("base/use.force"), "forced\n");
    write(&r.join("base/use.mask"), "masked\n");
    write(&r.join("base/packages"), "*sys-apps/portage\n");
    write(&r.join("leaf/parent"), "../base\n");
    write(&r.join("leaf/package.use"), "dev-libs/A bar\n");
    write(&r.join("leaf/package.mask"), "-dev-libs/evil\n");
    let p = StackedProfile::from_dir(r.join("leaf")).unwrap();
    let a = p.package_use.get("dev-libs/A").unwrap();
    assert!(a.contains(&"foo".to_string()) && a.contains(&"bar".to_string()));
    assert!(!p.package_mask.contains(&"dev-libs/evil".to_string()));
    assert!(p.use_force.contains(&"forced".to_string()));
    assert!(p.use_mask.contains(&"masked".to_string()));
    assert!(p.system_set.contains(&"sys-apps/portage".to_string()));
}

#[test]
fn repository_loads_metadata_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    write(&repo.join("profiles/repo_name"), "test\n");
    // reserved top dir is skipped (eclass).
    write(&repo.join("eclass/foo.eclass"), "x\n");
    write(
        &repo.join("dev-libs/A/A-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"2/3\"\nKEYWORDS=\"x86\"\nIUSE=\"+a -b\"\nDEPEND=\"dev-libs/B\"\n",
    );
    let r = Repository::load(&repo).unwrap();
    assert_eq!(r.name, "test");
    let m = r.db.metadata("dev-libs/A-1").unwrap();
    assert_eq!(m.slot.as_deref(), Some("2"));
    assert_eq!(m.sub_slot.as_deref(), Some("3"));
    assert_eq!(m.iuse, vec!["a".to_string(), "b".to_string()]);

    // Missing root + missing repo_name.
    assert!(Repository::load(dir.path().join("nope")).is_err());
    let bad = dir.path().join("bad");
    fs::create_dir_all(bad.join("dev-libs/A")).unwrap();
    assert!(Repository::load(&bad).is_err());
}

#[test]
fn getconfig_branches() {
    let empty = HashMap::new();
    let c = getconfig("export FOO=\"bar\"\nB=$FOO\n# comment\n", true, &empty).unwrap();
    assert_eq!(c.get("FOO").map(String::as_str), Some("bar"));
    assert_eq!(c.get("B").map(String::as_str), Some("bar"));
    // invalid var name -> Err
    assert!(getconfig("1BAD=\"x\"\n", true, &empty).is_err());
    // value-less -> Err
    assert!(getconfig("FOO\n", true, &empty).is_err());
    // unterminated quote -> Err
    assert!(getconfig("FOO=\"unterminated\n", true, &empty).is_err());
}

#[test]
fn varexpand_branches() {
    let mut m = HashMap::new();
    m.insert("A".to_string(), "5".to_string());
    m.insert("B".to_string(), "7".to_string());
    assert_eq!(varexpand("$A$B", &m), "57");
    assert_eq!(varexpand("${A}x", &m), "5x");
    assert_eq!(varexpand("$UNSET", &m), "");
    // ParseError Display path.
    let e = ParseError("boom".to_string());
    assert!(format!("{e}").contains("boom"));
}

#[test]
fn util_branches() {
    assert_eq!(normalize_path("///a/b/../c"), "/a/c");
    assert_eq!(normalize_path("a/./b/"), "a/b");
    assert_eq!(normalize_path(""), ".");
    let lines = grabfile("# c\na\n  b   # inline\n\n");
    assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);
    let d = grabdict("k v1 v2\nk v3\n", true, false);
    assert_eq!(d.get("k").unwrap().len(), 3);
    assert_eq!(
        stack_lists(&[vec!["a".into(), "b".into()], vec!["-a".into()]], true),
        vec!["b".to_string()]
    );
    assert!(stack_lists(&[vec!["a".into()], vec!["-*".into()]], true).is_empty());
    // stack_dicts None abort + ignore_none.
    let mut da = std::collections::BTreeMap::new();
    da.insert("x".to_string(), "1".to_string());
    assert!(stack_dicts(&[None, Some(da.clone())], false, &[], false).is_none());
    assert!(stack_dicts(&[None, Some(da)], false, &[], true).is_some());
}
