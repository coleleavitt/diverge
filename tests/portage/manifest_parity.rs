//! Ports of upstream checksum and Manifest behavior.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/util/test_checksum.py`
//! - `research/portage/lib/portage/manifest.py`

use diverge::manifest::{Manifest, ManifestError, ManifestType, checksum_str};

const TEXT: &[u8] = b"Some test string used to check if the hash works";

#[test]
fn checksum_str_matches_portage_vectors() {
    // Exact digests asserted in test_checksum.py.
    assert_eq!(
        checksum_str(b"", "MD5").unwrap(),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(
        checksum_str(TEXT, "MD5").unwrap(),
        "094c3bf4732f59b39d577e9726f1e934"
    );
    assert_eq!(
        checksum_str(b"", "SHA1").unwrap(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        checksum_str(TEXT, "SHA1").unwrap(),
        "5c572017d4e4d49e4aa03a2eda12dbb54a1e2e4f"
    );
    assert_eq!(
        checksum_str(b"", "SHA256").unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        checksum_str(TEXT, "SHA256").unwrap(),
        "e3d4a1135181fe156d61455615bb6296198e8ca5b2f20ddeb85cb4cd27f62320"
    );
    assert_eq!(
        checksum_str(b"", "SHA512").unwrap(),
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    );
    assert_eq!(checksum_str(b"", "BOGUS"), None);
}

fn manifest_for(name: &str, data: &[u8]) -> Manifest {
    let line = format!(
        "DIST {name} {} BLAKE2B {} SHA512 {}",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap(),
        checksum_str(data, "SHA512").unwrap()
    );
    Manifest::parse(&line).expect("manifest parses")
}

#[test]
fn manifest_parses_and_classifies_entries() {
    let content = "\
EBUILD foo-1.ebuild 100 SHA512 abc BLAKE2B def
DIST foo-1.tar.gz 2048 SHA512 111 BLAKE2B 222
MISC metadata.xml 50 SHA512 333 BLAKE2B 444
";
    let manifest = Manifest::parse(content).unwrap();
    assert_eq!(manifest.len(), 3);
    assert_eq!(manifest.entries_of(ManifestType::Dist).len(), 1);
    let dist = manifest.entry("foo-1.tar.gz").unwrap();
    assert_eq!(dist.size, 2048);
    assert_eq!(dist.hashes.get("SHA512").map(String::as_str), Some("111"));
}

#[test]
fn manifest_verifies_matching_bytes() {
    let data = TEXT;
    let manifest = manifest_for("payload.bin", data);
    assert!(manifest.verify("payload.bin", data).is_ok());
}

#[test]
fn manifest_rejects_size_and_digest_mismatch() {
    let data = TEXT;
    let manifest = manifest_for("payload.bin", data);

    // Wrong size.
    let err = manifest.verify("payload.bin", b"short").unwrap_err();
    assert!(
        matches!(err, ManifestError::SizeMismatch { .. }),
        "got {err}"
    );

    // Right size, wrong content.
    let tampered: Vec<u8> = std::iter::repeat_n(b'x', data.len()).collect();
    let err = manifest.verify("payload.bin", &tampered).unwrap_err();
    assert!(
        matches!(err, ManifestError::DigestMismatch { .. }),
        "got {err}"
    );

    // Unknown file.
    let err = manifest.verify("missing", data).unwrap_err();
    assert!(matches!(err, ManifestError::UnknownFile(_)), "got {err}");
}

#[test]
fn manifest_round_trips_through_render() {
    let content = "DIST foo-1.tar.gz 2048 BLAKE2B 222 SHA512 111\n";
    let manifest = Manifest::parse(content).unwrap();
    // render sorts hashes (BTreeMap) and preserves the entry; re-parse matches.
    let reparsed = Manifest::parse(&manifest.render()).unwrap();
    assert_eq!(
        reparsed.entry("foo-1.tar.gz"),
        manifest.entry("foo-1.tar.gz")
    );
}

#[test]
fn manifest_rejects_malformed_lines() {
    assert!(matches!(
        Manifest::parse("BOGUS foo 100 SHA512 abc"),
        Err(ManifestError::MalformedLine(_))
    ));
    assert!(matches!(
        Manifest::parse("DIST foo 100 SHA512"),
        Err(ManifestError::UnpairedHash(_))
    ));
}
