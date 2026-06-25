//! Last-mile coverage of remaining reachable branch arms across the crate.

use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn atom_operator_requires_version_glob_edge() {
    use diverge::atom::{Atom, AtomError, AtomParseOptions};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    // `=cat/pkg*` with no version after operator -> OperatorRequiresVersion.
    assert!(matches!(
        Atom::parse_with_options("=dev-libs/*", WILD),
        Err(AtomError::OperatorRequiresVersion) | Err(AtomError::WildcardNotAllowed)
    ));
    // wildcard in package name without allow_wildcard.
    const NOWILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: false,
        allow_repo: true,
    };
    assert!(matches!(
        Atom::parse_with_options("dev-libs/*", NOWILD),
        Err(AtomError::WildcardNotAllowed)
    ));
    // category with invalid chars.
    assert!(Atom::parse_with_options("dev libs/A", WILD).is_err());
}

#[test]
fn resolver_fixture_all_operators() {
    use diverge::cli::EmergeOptions;
    use diverge::resolver::{PackageRecord, ResolverFixture};
    let fixture = ResolverFixture {
        ebuilds: vec![
            PackageRecord::new("d/A-1").with_keywords(["x86"]),
            PackageRecord::new("d/A-2").with_keywords(["x86"]),
            PackageRecord::new("d/A-3").with_keywords(["x86"]),
        ],
        binpkgs: vec![],
        installed: vec![],
    };
    let o = EmergeOptions::default();
    // > and <= operators (lines 233-245).
    assert!(fixture.resolve(">d/A-2", &o).success);
    assert!(fixture.resolve("<=d/A-2", &o).success);
    assert!(fixture.resolve(">=d/A-1", &o).success);
    assert!(fixture.resolve("<d/A-3", &o).success);
    // No-match operator (unsatisfiable) -> failure (line 251 _ => false).
    assert!(!fixture.resolve(">d/A-9", &o).success);
}

#[test]
fn profile_system_atoms_and_absolute_parent() {
    use diverge::profile::{ProfileStack, StackedProfile};
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    // base with `packages` containing *-prefixed and -*-prefixed atoms.
    write(
        &r.join("base/packages"),
        "*sys-apps/portage\n-*sys-apps/old\n",
    );
    write(&r.join("base/make.defaults"), "USE=\"a\"\n");
    // leaf with an ABSOLUTE parent path (line 138).
    let abs_base = r.join("base");
    write(&r.join("leaf/parent"), &format!("{}\n", abs_base.display()));
    write(&r.join("leaf/make.defaults"), "USE=\"b\"\n");
    let stack = ProfileStack::resolve(r.join("leaf")).unwrap();
    assert!(stack.profiles.iter().any(|p| p.ends_with("base")));
    let prof = StackedProfile::from_dir(r.join("leaf")).unwrap();
    // The *-prefixed atom is in the system set; the -* removal (system_atoms
    // line 259) is consumed by incremental stacking, leaving it absent.
    assert!(prof.system_set.contains(&"sys-apps/portage".to_string()));
    assert!(!prof.system_set.contains(&"sys-apps/old".to_string()));
}

#[test]
fn repository_parse_error_surfaces() {
    use diverge::repository::Repository;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    write(&repo.join("profiles/repo_name"), "test\n");
    // An ebuild with a syntax error (unterminated quote) -> Parse error.
    write(&repo.join("dev-libs/A/A-1.ebuild"), "SLOT=\"unterminated\n");
    let res = Repository::load(&repo);
    assert!(res.is_err(), "malformed ebuild should error");
    let msg = format!("{}", res.err().unwrap());
    assert!(msg.contains("A-1.ebuild"), "error cites the file: {msg}");
}

#[test]
fn vardb_iter_skips_files_and_dotdirs() {
    use diverge::vardb;
    let dir = tempfile::tempdir().unwrap();
    let vdb = dir.path().join("pkg");
    // A stray file at category level and a dotdir are skipped (lines 60/65).
    write(&vdb.join("README"), "not a category\n");
    fs::create_dir_all(vdb.join(".hidden")).unwrap();
    write(&vdb.join("sys-libs/foo-1/SLOT"), "0\n");
    write(&vdb.join("sys-libs/stray-file"), "x\n"); // file under category
    let db = vardb::load(&vdb).unwrap();
    assert!(!db.match_str("sys-libs/foo").unwrap().is_empty());
    assert_eq!(db.len(), 1);
}

#[test]
fn dbapi_aux_default_slot_and_update_invalid() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::update::parse_updates;
    // A package with no slot -> aux_get("SLOT") falls back to "0" (line 60).
    let mut db = PackageDb::new();
    db.insert(
        "d/A-1",
        PackageMetadata {
            slot: None,
            ..PackageMetadata::default()
        },
    );
    assert_eq!(db.aux_get("d/A-1", "SLOT").as_deref(), Some("0"));
    // update.rs line 86: a move with an invalid cp -> error.
    assert!(parse_updates("move not-a-cp other").is_err());
}

#[test]
fn util_grabdict_skips_short_and_comment_lines() {
    use diverge::util::grabdict;
    // A key-only line with empty=false is skipped (line 110); a comment line
    // is skipped (line 113).
    let d = grabdict("# comment\nkeyonly\nk v1 v2\n", false, false);
    assert!(!d.contains_key("keyonly"));
    assert_eq!(d.get("k").map(Vec::len), Some(2));
    // With empty=true, a key-only line is kept with no values.
    let d = grabdict("keyonly\n", false, true);
    assert_eq!(d.get("keyonly").map(Vec::len), Some(0));
}

#[test]
fn xpak_truncated_index_length() {
    use diverge::xpak::xpak_parse;
    // A segment with valid magic but a declared index length exceeding the
    // body -> Truncated (line 95).
    let mut seg = Vec::new();
    seg.extend_from_slice(b"XPAKPACK");
    seg.extend_from_slice(&999u32.to_be_bytes()); // index_len far too big
    seg.extend_from_slice(&0u32.to_be_bytes()); // data_len
    seg.extend_from_slice(b"XPAKSTOP");
    assert!(xpak_parse(&seg).is_err());
}
