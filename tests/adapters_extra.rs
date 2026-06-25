//! Additional integration tests for adapters: xpak, news, sync, and gpkg.
//!
//! This test file ports additional observable behavior from Portage's test suites:
//! - `research/portage/lib/portage/tests/xpak/test_decodeint.py` (xpak encoding edge cases)
//! - `research/portage/lib/portage/tests/news/test_NewsItem.py` (multi-restriction combinations)
//! - `research/portage/lib/portage/tests/sync/test_sync_local.py` (sync with nested dirs)
//! - `research/portage/lib/portage/tests/gpkg/test_gpkg_checksum.py` (checksum verification)
//! - `research/portage/lib/portage/tests/gpkg/test_gpkg_size.py` (size calculation)
//! - `research/portage/lib/portage/tests/gpkg/test_gpkg_metadata_update.py` (metadata updates)

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use diverge::gpkg::Gpkg;
use diverge::news::{NewsEnvironment, NewsItem};
use diverge::sync::{LocalSync, SyncBackend, SyncConfig, SyncType};
use diverge::xpak::{decodeint, encodeint};

// ============================================================================
// XPAK: Extended integer encoding roundtrips
// ============================================================================
// Reference: research/portage/lib/portage/tests/xpak/test_decodeint.py
// Tests that encodeint/decodeint properly handle full range of u32 values.

/// Roundtrip tests for all u32 values in a larger range.
/// Original: for n in range(1000): self.assertEqual(decodeint(encodeint(n)), n)
#[test]
fn xpak_encodeint_decodeint_roundtrip_0_to_1000() {
    for n in 0u32..1000 {
        let encoded = encodeint(n);
        let decoded = decodeint(&encoded);
        assert_eq!(
            decoded,
            Some(n),
            "roundtrip failed for {}: encoded={:?}, decoded={:?}",
            n,
            encoded,
            decoded
        );
    }
}

/// Boundary value: u32::MAX (2^32 - 1).
/// Original: self.assertEqual(decodeint(encodeint(2**32 - 1)), 2**32 - 1)
#[test]
fn xpak_encodeint_decodeint_roundtrip_u32_max() {
    let max_u32 = u32::MAX;
    assert_eq!(decodeint(&encodeint(max_u32)), Some(max_u32));
}

/// Sampling larger values: powers of 2, millions, etc.
#[test]
fn xpak_encodeint_decodeint_roundtrip_samples() {
    let samples = vec![
        0u32,
        1,
        255,
        256,
        65535,
        65536,
        1_000_000,
        1_000_000_000,
        u32::MAX / 2,
        u32::MAX - 1,
        u32::MAX,
    ];
    for n in samples {
        assert_eq!(decodeint(&encodeint(n)), Some(n));
    }
}

/// decodeint returns None on insufficient bytes.
#[test]
fn xpak_decodeint_insufficient_bytes() {
    assert_eq!(decodeint(&[]), None);
    assert_eq!(decodeint(&[0x00]), None);
    assert_eq!(decodeint(&[0x00, 0x00]), None);
    assert_eq!(decodeint(&[0x00, 0x00, 0x00]), None);
    assert_eq!(decodeint(&[0x00, 0x00, 0x00, 0x00]), Some(0));
    assert_eq!(decodeint(&[0xFF, 0xFF, 0xFF, 0xFF]), Some(u32::MAX));
}

// ============================================================================
// NEWS: Complex display restriction combinations
// ============================================================================
// Reference: research/portage/lib/portage/tests/news/test_NewsItem.py
// Tests complex interactions of Display-If-{Installed,Keyword,Profile}.

fn news_with_restrictions(restrictions: &str) -> String {
    format!(
        "Title: Test Item\n\
         Author: dev <dev@example.com>\n\
         Posted: 2024-01-01\n\
         Revision: 1\n\
         News-Item-Format: 2.0\n\
         {}\n\
         Body text.\n",
        restrictions
    )
}

/// Test: All three restriction types present; all match -> relevant.
/// Combination of Display-If-Installed, Display-If-Keyword, Display-If-Profile.
#[test]
fn news_three_restrictions_all_match() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Installed: dev-libs/A\n\
         Display-If-Keyword: amd64\n\
         Display-If-Profile: /var/db/repos/gentoo/profiles/default/linux/amd64",
    ));
    let env = NewsEnvironment {
        installed: vec!["dev-libs/A-1.0".to_string()],
        keyword: "amd64".to_string(),
        profile: Some("/var/db/repos/gentoo/profiles/default/linux/amd64".to_string()),
    };
    assert!(item.is_relevant(&env));
}

/// Test: Three restrictions present; one fails -> irrelevant.
#[test]
fn news_three_restrictions_one_fails() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Installed: dev-libs/A\n\
         Display-If-Keyword: amd64\n\
         Display-If-Profile: /wrong/profile",
    ));
    let env = NewsEnvironment {
        installed: vec!["dev-libs/A-1.0".to_string()],
        keyword: "amd64".to_string(),
        profile: Some("/var/db/repos/gentoo/profiles/default/linux/amd64".to_string()),
    };
    assert!(!item.is_relevant(&env));
}

/// Test: Multiple profiles listed; match one -> relevant.
/// Original: testDisplayIfProfile with multiple profile options.
#[test]
fn news_multiple_profiles_match_one() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Profile: /profile/a\n\
         Display-If-Profile: /profile/b\n\
         Display-If-Profile: /profile/c",
    ));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "".to_string(),
        profile: Some("/profile/b".to_string()),
    };
    assert!(item.is_relevant(&env));
}

/// Test: Multiple profiles listed; match none -> irrelevant.
#[test]
fn news_multiple_profiles_match_none() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Profile: /profile/a\n\
         Display-If-Profile: /profile/b",
    ));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "".to_string(),
        profile: Some("/profile/c".to_string()),
    };
    assert!(!item.is_relevant(&env));
}

/// Test: Multiple keywords listed; match one -> relevant.
#[test]
fn news_multiple_keywords_match_one() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Keyword: x86\n\
         Display-If-Keyword: amd64\n\
         Display-If-Keyword: arm64",
    ));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "amd64".to_string(),
        profile: None,
    };
    assert!(item.is_relevant(&env));
}

/// Test: Multiple keywords listed; match none -> irrelevant.
#[test]
fn news_multiple_keywords_match_none() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Keyword: x86\n\
         Display-If-Keyword: arm64",
    ));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "amd64".to_string(),
        profile: None,
    };
    assert!(!item.is_relevant(&env));
}

/// Test: Multiple installed packages; match one -> relevant.
#[test]
fn news_multiple_installed_match_one() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Installed: sys-apps/portage\n\
         Display-If-Installed: app-portage/gentoolkit\n\
         Display-If-Installed: dev-util/pkgdev",
    ));
    let env = NewsEnvironment {
        installed: vec!["app-portage/gentoolkit-0.5".to_string()],
        keyword: "".to_string(),
        profile: None,
    };
    assert!(item.is_relevant(&env));
}

/// Test: Multiple installed packages; match none -> irrelevant.
#[test]
fn news_multiple_installed_match_none() {
    let item = NewsItem::parse(&news_with_restrictions(
        "Display-If-Installed: sys-apps/portage\n\
         Display-If-Installed: app-portage/gentoolkit",
    ));
    let env = NewsEnvironment {
        installed: vec!["dev-util/pkgdev-0.1".to_string()],
        keyword: "".to_string(),
        profile: None,
    };
    assert!(!item.is_relevant(&env));
}

/// Test: Empty profile Some("") should not match.
#[test]
fn news_profile_empty_string_no_match() {
    let item = NewsItem::parse(&news_with_restrictions("Display-If-Profile: /some/profile"));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "".to_string(),
        profile: Some("".to_string()),
    };
    assert!(!item.is_relevant(&env));
}

/// Test: None profile with Display-If-Profile -> irrelevant.
#[test]
fn news_profile_none_no_match() {
    let item = NewsItem::parse(&news_with_restrictions("Display-If-Profile: /some/profile"));
    let env = NewsEnvironment {
        installed: vec![],
        keyword: "".to_string(),
        profile: None,
    };
    assert!(!item.is_relevant(&env));
}

// ============================================================================
// SYNC: More complex local sync scenarios
// ============================================================================
// Reference: research/portage/lib/portage/tests/sync/test_sync_local.py
// Tests sync behavior with nested directories and non-trivial payloads.

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, content).expect("write file");
}

/// Test: Sync with nested directory structure.
#[test]
fn sync_local_nested_directories() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");

    // Create nested structure
    write(&src.join("profiles/repo_name"), "test_repo\n");
    write(&src.join("metadata/layout.conf"), "masters = gentoo\n");
    write(&src.join("dev-libs/A/A-1.0.ebuild"), "EAPI=7\n");
    write(
        &src.join("dev-libs/B/B-2.0.ebuild"),
        "EAPI=8\nDEPEND=\"dev-libs/A\"\n",
    );
    write(&src.join("sys-apps/foo/files/patch.diff"), "--- a\n+++ b\n");

    let mut backend = LocalSync;
    let config = SyncConfig {
        name: "test_repo".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };

    let outcome = backend.sync(&config).expect("sync");
    assert!(outcome.updated);
    assert!(dest.join("profiles/repo_name").exists());
    assert!(dest.join("metadata/layout.conf").exists());
    assert!(dest.join("dev-libs/A/A-1.0.ebuild").exists());
    assert!(dest.join("dev-libs/B/B-2.0.ebuild").exists());
    assert!(dest.join("sys-apps/foo/files/patch.diff").exists());
    assert_eq!(outcome.changed_files.len(), 5);
}

/// Test: Sync with large files.
#[test]
fn sync_local_large_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");

    // Create a moderate-size file (1 MB)
    let large_content = "x".repeat(1_000_000);
    write(&src.join("data/large.bin"), &large_content);
    write(&src.join("metadata/timestamp.chk"), "2024-01-01 00:00:00\n");

    let mut backend = LocalSync;
    let config = SyncConfig {
        name: "test_repo".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };

    let outcome = backend.sync(&config).expect("sync");
    assert!(outcome.updated);
    assert!(dest.join("data/large.bin").exists());
    let dest_content = fs::read_to_string(dest.join("data/large.bin")).expect("read");
    assert_eq!(dest_content.len(), 1_000_000);
}

/// Test: Sync detects and reports file changes.
#[test]
fn sync_local_detects_file_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");

    write(&src.join("file.txt"), "version 1\n");

    let mut backend = LocalSync;
    let config = SyncConfig {
        name: "test_repo".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };

    // First sync
    let outcome1 = backend.sync(&config).expect("sync 1");
    assert!(outcome1.updated);
    assert_eq!(outcome1.changed_files.len(), 1);

    // Second sync: no changes
    let outcome2 = backend.sync(&config).expect("sync 2");
    assert!(!outcome2.updated);
    assert_eq!(outcome2.changed_files.len(), 0);

    // Modify source
    write(&src.join("file.txt"), "version 2\n");

    // Third sync: file changed
    let outcome3 = backend.sync(&config).expect("sync 3");
    assert!(outcome3.updated);
    assert_eq!(outcome3.changed_files.len(), 1);
    let dest_content = fs::read_to_string(dest.join("file.txt")).expect("read");
    assert_eq!(dest_content, "version 2\n");
}

/// Test: Sync with many small files.
#[test]
fn sync_local_many_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");

    for i in 0..50 {
        write(
            &src.join(format!("file-{}.txt", i)),
            &format!("content {}\n", i),
        );
    }

    let mut backend = LocalSync;
    let config = SyncConfig {
        name: "test_repo".to_string(),
        location: dest.clone(),
        uri: src.to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };

    let outcome = backend.sync(&config).expect("sync");
    assert!(outcome.updated);
    assert_eq!(outcome.changed_files.len(), 50);
}

// ============================================================================
// GPKG: Extended encoding/decoding and metadata management
// ============================================================================
// Reference:
// - research/portage/lib/portage/tests/gpkg/test_gpkg_checksum.py
// - research/portage/lib/portage/tests/gpkg/test_gpkg_size.py
// - research/portage/lib/portage/tests/gpkg/test_gpkg_metadata_update.py

fn make_metadata() -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert("SLOT".to_string(), b"0".to_vec());
    m.insert("repository".to_string(), b"gentoo".to_vec());
    m.insert("DEPEND".to_string(), b"dev-libs/B".to_vec());
    m
}

/// Test: Empty image roundtrips.
#[test]
fn gpkg_empty_image_roundtrips() {
    let pkg = Gpkg::new(make_metadata(), vec![]);
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded, pkg);
    assert_eq!(decoded.image.len(), 0);
}

/// Test: Large image (1 MB) roundtrips.
#[test]
fn gpkg_large_image_roundtrips() {
    let large_image = vec![0xAAu8; 1_000_000];
    let pkg = Gpkg::new(make_metadata(), large_image.clone());
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded.image, large_image);
}

/// Test: Signature roundtrips with non-trivial content.
#[test]
fn gpkg_signature_with_content_roundtrips() {
    let signature = b"GPG_SIGNATURE_DATA_HERE".to_vec();
    let pkg = Gpkg::new(make_metadata(), b"payload".to_vec()).with_signature(signature.clone());
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded.signature.as_ref(), Some(&signature));
}

/// Test: Metadata with many entries.
#[test]
fn gpkg_many_metadata_entries() {
    let mut meta = BTreeMap::new();
    for i in 0..50 {
        meta.insert(format!("KEY_{}", i), format!("value_{}", i).into_bytes());
    }
    let pkg = Gpkg::new(meta, b"image".to_vec());
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded.metadata.len(), 50);
    for i in 0..50 {
        assert_eq!(
            decoded.metadata.get(&format!("KEY_{}", i)),
            Some(&format!("value_{}", i).into_bytes())
        );
    }
}

/// Test: Size calculation for various configurations.
#[test]
fn gpkg_size_empty() {
    let pkg = Gpkg::new(BTreeMap::new(), vec![]);
    let size = pkg.size();
    // Should be at least the magic, header lengths, and final checksum.
    assert!(size > 20);
}

/// Test: Size grows with image size.
#[test]
fn gpkg_size_grows_with_image() {
    let small = Gpkg::new(make_metadata(), b"small".to_vec());
    let small_size = small.size();

    let large = Gpkg::new(make_metadata(), vec![0xAAu8; 10000]);
    let large_size = large.size();

    assert!(large_size > small_size);
    // The difference should be at least most of the image size difference
    // (accounting for framing overhead of ~10 bytes)
    assert!(large_size - small_size > 9980);
}

/// Test: Size grows with metadata entries.
#[test]
fn gpkg_size_grows_with_metadata() {
    let pkg1 = Gpkg::new(make_metadata(), b"image".to_vec());
    let size1 = pkg1.size();

    let mut meta2 = make_metadata();
    meta2.insert("NEW_KEY".to_string(), b"value_value_value".to_vec());
    let pkg2 = Gpkg::new(meta2, b"image".to_vec());
    let size2 = pkg2.size();

    assert!(size2 > size1);
}

/// Test: Corruption in metadata section is detected.
#[test]
fn gpkg_corrupted_metadata_detected() {
    let pkg = Gpkg::new(make_metadata(), b"payload".to_vec());
    let mut encoded = pkg.encode();

    // Flip a bit in the metadata region (after magic, in the first length field).
    if encoded.len() > 20 {
        encoded[15] ^= 0x01;
    }

    let result = Gpkg::decode(&encoded);
    // Should either fail checksum or fail to parse
    assert!(result.is_err());
}

/// Test: Signature slot is independent of image content.
#[test]
fn gpkg_signature_independent_of_image() {
    let image1 = b"image_v1".to_vec();
    let image2 = b"image_v2".to_vec();
    let sig = b"SIGNATURE".to_vec();

    let pkg1 = Gpkg::new(make_metadata(), image1).with_signature(sig.clone());
    let pkg2 = Gpkg::new(make_metadata(), image2).with_signature(sig.clone());

    let enc1 = pkg1.encode();
    let enc2 = pkg2.encode();

    let dec1 = Gpkg::decode(&enc1).expect("decode 1");
    let dec2 = Gpkg::decode(&enc2).expect("decode 2");

    // Both should have the same signature
    assert_eq!(dec1.signature.as_ref(), Some(&sig));
    assert_eq!(dec2.signature.as_ref(), Some(&sig));

    // But different images
    assert_ne!(dec1.image, dec2.image);
}

/// Test: Metadata updates preserve other fields.
#[test]
fn gpkg_metadata_updates_preserve_signature() {
    let original_sig = b"ORIGINAL_SIGNATURE".to_vec();
    let original_image = b"image_data".to_vec();
    let original_meta = make_metadata();

    let pkg = Gpkg::new(original_meta, original_image.clone()).with_signature(original_sig.clone());
    let encoded = pkg.encode();

    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded.signature, Some(original_sig.clone()));
    assert_eq!(decoded.image, original_image);

    // Simulate metadata update by creating a new package with same image/sig
    let mut new_meta = BTreeMap::new();
    new_meta.insert("SLOT".to_string(), b"1".to_vec());
    new_meta.insert("IUSE".to_string(), b"+flag".to_vec());

    let updated = Gpkg::new(new_meta, original_image.clone()).with_signature(original_sig.clone());
    let updated_encoded = updated.encode();
    let updated_decoded = Gpkg::decode(&updated_encoded).expect("decode updated");

    assert_eq!(updated_decoded.metadata_str("SLOT").as_deref(), Some("1"));
    assert_eq!(updated_decoded.signature, Some(original_sig));
}

/// Test: Metadata roundtrip preserves all keys (UTF-8 and special bytes).
#[test]
fn gpkg_metadata_special_values() {
    let mut meta = BTreeMap::new();
    meta.insert("ASCII".to_string(), b"hello".to_vec());
    meta.insert("BINARY".to_string(), vec![0x00, 0x01, 0x02, 0xFF, 0xFE]);
    meta.insert("MULTILINE".to_string(), b"line1\nline2\nline3".to_vec());
    meta.insert("EMPTY".to_string(), vec![]);

    let pkg = Gpkg::new(meta, b"img".to_vec());
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");

    assert_eq!(decoded.metadata_str("ASCII").as_deref(), Some("hello"));
    assert_eq!(
        decoded.metadata.get("BINARY"),
        Some(&vec![0x00, 0x01, 0x02, 0xFF, 0xFE])
    );
    assert_eq!(
        decoded.metadata_str("MULTILINE").as_deref(),
        Some("line1\nline2\nline3")
    );
    assert_eq!(decoded.metadata.get("EMPTY"), Some(&vec![]));
}

/// Test: Multiple roundtrip cycles preserve content.
#[test]
fn gpkg_multiple_roundtrips() {
    let original =
        Gpkg::new(make_metadata(), b"original_image".to_vec()).with_signature(b"sig".to_vec());

    let mut current = original.clone();
    for _ in 0..3 {
        let encoded = current.encode();
        current = Gpkg::decode(&encoded).expect("decode");
    }

    assert_eq!(current, original);
}
