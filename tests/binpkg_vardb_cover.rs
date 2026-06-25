//! Coverage for xpak / vardb / manifest branches.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use diverge::dbapi::PackageMetadata;
use diverge::executor::merge::ContentEntry;
use diverge::manifest::{Manifest, ManifestError, ManifestType, checksum_str};
use diverge::vardb;
use diverge::xpak::{XpakError, decodeint, encodeint, xpak_mem, xpak_parse};

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn xpak_int_round_trip_and_short_bytes() {
    for n in [0u32, 1, 255, 256, 65535, 16_777_216, u32::MAX] {
        assert_eq!(decodeint(&encodeint(n)), Some(n));
    }
    assert_eq!(decodeint(&[0, 1]), None); // fewer than 4 bytes
}

#[test]
fn xpak_segment_round_trip_many() {
    let mut d = BTreeMap::new();
    d.insert("SLOT".to_string(), b"0".to_vec());
    d.insert("USE".to_string(), b"a b c".to_vec());
    d.insert("DEPEND".to_string(), b"d/B d/C".to_vec());
    d.insert("EAPI".to_string(), b"7".to_vec());
    let seg = xpak_mem(&d);
    assert_eq!(xpak_parse(&seg).unwrap(), d);
}

#[test]
fn xpak_parse_errors() {
    assert!(matches!(xpak_parse(b"short"), Err(XpakError::BadMagic)));
    assert!(matches!(
        xpak_parse(b"not even close to a real xpak"),
        Err(XpakError::BadMagic)
    ));
    // Valid magic but truncated body.
    let mut d = BTreeMap::new();
    d.insert("K".to_string(), b"value".to_vec());
    let seg = xpak_mem(&d);
    let truncated = &seg[..seg.len() - 10];
    assert!(xpak_parse(truncated).is_err());
}

#[test]
fn vardb_load_record_remove_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let vdb = dir.path().join("var/db/pkg");
    // Pre-seed one installed package directory.
    write(&vdb.join("sys-libs/foo-1/SLOT"), "0\n");
    write(&vdb.join("sys-libs/foo-1/KEYWORDS"), "amd64\n");
    write(&vdb.join("sys-libs/foo-1/IUSE"), "+a -b\n");
    write(&vdb.join("sys-libs/foo-1/RDEPEND"), "sys-libs/bar\n");
    let db = vardb::load(&vdb).unwrap();
    let m = db.metadata("sys-libs/foo-1").unwrap();
    assert_eq!(m.slot.as_deref(), Some("0"));
    assert_eq!(m.iuse, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(
        m.deps.get("RDEPEND").map(String::as_str),
        Some("sys-libs/bar")
    );

    // record_install writes a new entry.
    let meta = PackageMetadata {
        slot: Some("2".to_string()),
        sub_slot: Some("3".to_string()),
        repo: Some("gentoo".to_string()),
        eapi: Some("8".to_string()),
        iuse: vec!["x".to_string()],
        use_enabled: vec!["x".to_string()],
        keywords: vec!["amd64".to_string()],
        deps: Default::default(),
    };
    let contents = vec![
        ContentEntry::Dir {
            path: "usr".to_string(),
        },
        ContentEntry::File {
            path: "usr/bin/baz".to_string(),
            protected: false,
        },
        ContentEntry::Symlink {
            path: "usr/bin/q".to_string(),
            target: "baz".to_string(),
        },
    ];
    vardb::record_install(&vdb, "app-misc/baz-9", &meta, &contents).unwrap();
    let reloaded = vardb::load(&vdb).unwrap();
    assert!(!reloaded.match_str("app-misc/baz").unwrap().is_empty());
    let cont = fs::read_to_string(vdb.join("app-misc/baz-9/CONTENTS")).unwrap();
    assert!(cont.contains("obj /usr/bin/baz"));
    assert!(cont.contains("sym /usr/bin/q -> baz"));
    assert!(cont.contains("dir /usr"));

    // remove_install deletes it; second remove is a no-op.
    vardb::remove_install(&vdb, "app-misc/baz-9").unwrap();
    assert!(!vdb.join("app-misc/baz-9").exists());
    vardb::remove_install(&vdb, "app-misc/baz-9").unwrap();
}

#[test]
fn vardb_missing_root_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let db = vardb::load(dir.path().join("no/pkg")).unwrap();
    assert!(db.is_empty());
    assert!(vardb::vdb_path(Path::new("/x")).ends_with("var/db/pkg"));
}

#[test]
fn manifest_unknown_algo_and_mismatch() {
    assert_eq!(checksum_str(b"x", "BOGUS"), None);
    let data = b"payload";
    let line = format!(
        "DIST f {} BLAKE2B {} SHA512 {}",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap(),
        checksum_str(data, "SHA512").unwrap()
    );
    let m = Manifest::parse(&line).unwrap();
    assert!(m.verify("f", data).is_ok());
    // size mismatch.
    assert!(matches!(
        m.verify("f", b"short"),
        Err(ManifestError::SizeMismatch { .. })
    ));
    // unknown file.
    assert!(matches!(
        m.verify("nope", data),
        Err(ManifestError::UnknownFile(_))
    ));
    // render round-trips.
    let m2 = Manifest::parse(&m.render()).unwrap();
    assert_eq!(m2.entry("f"), m.entry("f"));
    assert_eq!(m.entries_of(ManifestType::Dist).len(), 1);
    assert_eq!(m.len(), 1);
    assert!(!m.is_empty());
}

#[test]
fn manifest_no_usable_hash() {
    // Only an unknown hash recorded -> NoUsableHash on verify.
    let data = b"x";
    let line = format!("DIST f {} BOGUSHASH deadbeef", data.len());
    let m = Manifest::parse(&line).unwrap();
    assert!(matches!(
        m.verify("f", data),
        Err(ManifestError::NoUsableHash(_))
    ));
}
