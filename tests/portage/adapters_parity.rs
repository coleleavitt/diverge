//! Tests for the adapter layer: news items, sync, and gpkg binary packages.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/news/test_NewsItem.py`
//! - `research/portage/lib/portage/tests/sync/test_sync_local.py`
//! - `research/portage/lib/portage/tests/gpkg/test_gpkg_checksum.py`

use std::collections::BTreeMap;

use diverge::gpkg::{Gpkg, GpkgError};
use diverge::news::{NewsEnvironment, NewsItem, ReadTracker};
use diverge::sync::{LocalSync, SyncBackend, SyncConfig, SyncType};

use crate::fs_fixture::write;

// ---------------------------------------------------------------- news

fn news_text(restrictions: &str) -> String {
    format!(
        "Title: Test Item\n\
         Author: dev <dev@example.com>\n\
         Posted: 2024-01-01\n\
         Revision: 1\n\
         News-Item-Format: 2.0\n\
         {restrictions}\n\
         A news body.\n"
    )
}

#[test]
fn news_with_no_restrictions_is_always_relevant() {
    let item = NewsItem::parse(&news_text(""));
    assert!(item.is_valid());
    assert!(item.is_relevant(&NewsEnvironment::default()));
}

#[test]
fn news_display_if_installed_matches_vardb() {
    let item = NewsItem::parse(&news_text("Display-If-Installed: dev-libs/A"));
    let relevant = NewsEnvironment {
        installed: vec!["dev-libs/A-1".to_string()],
        ..Default::default()
    };
    let irrelevant = NewsEnvironment {
        installed: vec!["dev-libs/B-1".to_string()],
        ..Default::default()
    };
    assert!(item.is_relevant(&relevant));
    assert!(!item.is_relevant(&irrelevant));
}

#[test]
fn news_restriction_types_are_anded() {
    // Installed AND keyword must both match.
    let item = NewsItem::parse(&news_text(
        "Display-If-Installed: dev-libs/A\nDisplay-If-Keyword: amd64",
    ));
    let both = NewsEnvironment {
        installed: vec!["dev-libs/A-1".to_string()],
        keyword: "amd64".to_string(),
        profile: None,
    };
    let only_installed = NewsEnvironment {
        installed: vec!["dev-libs/A-1".to_string()],
        keyword: "x86".to_string(),
        profile: None,
    };
    assert!(item.is_relevant(&both));
    assert!(!item.is_relevant(&only_installed));
}

#[test]
fn news_same_type_restrictions_are_ored() {
    let item = NewsItem::parse(&news_text(
        "Display-If-Keyword: amd64\nDisplay-If-Keyword: x86",
    ));
    let env = NewsEnvironment {
        keyword: "x86".to_string(),
        ..Default::default()
    };
    assert!(item.is_relevant(&env));
}

#[test]
fn news_read_tracker_tracks_unread() {
    let mut tracker = ReadTracker::parse("item-a\nitem-b\n");
    assert!(tracker.is_read("item-a"));
    assert!(!tracker.is_read("item-c"));
    let all = vec![
        "item-a".to_string(),
        "item-b".to_string(),
        "item-c".to_string(),
    ];
    assert_eq!(tracker.unread(&all), vec![&"item-c".to_string()]);
    assert!(tracker.mark_read("item-c"));
    assert!(!tracker.mark_read("item-c")); // already read
    assert!(tracker.render().contains("item-c"));
}

// ---------------------------------------------------------------- sync

#[test]
fn local_sync_copies_tree_and_reports_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dest = dir.path().join("dest");
    write(&src.join("profiles/repo_name"), "test_repo\n");
    write(&src.join("dev-libs/A/A-1.ebuild"), "EAPI=7\n");

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
    assert!(dest.join("dev-libs/A/A-1.ebuild").exists());
    assert_eq!(outcome.changed_files.len(), 2);

    // A second sync with no source changes reports no updates.
    let outcome2 = backend.sync(&config).expect("sync again");
    assert!(!outcome2.updated, "idempotent re-sync: {:?}", outcome2);
}

#[test]
fn local_sync_missing_source_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut backend = LocalSync;
    let config = SyncConfig {
        name: "r".to_string(),
        location: dir.path().join("dest"),
        uri: dir.path().join("nope").to_string_lossy().into_owned(),
        sync_type: SyncType::Local,
    };
    assert!(backend.sync(&config).is_err());
}

// ---------------------------------------------------------------- gpkg

fn meta() -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    m.insert("SLOT".to_string(), b"0".to_vec());
    m.insert("repository".to_string(), b"gentoo".to_vec());
    m.insert("DEPEND".to_string(), b"dev-libs/B".to_vec());
    m
}

#[test]
fn gpkg_round_trips_metadata_and_image() {
    let pkg = Gpkg::new(meta(), b"image-payload-bytes".to_vec());
    let encoded = pkg.encode();
    let decoded = Gpkg::decode(&encoded).expect("decode");
    assert_eq!(decoded, pkg);
    assert_eq!(decoded.metadata_str("SLOT").as_deref(), Some("0"));
    assert_eq!(
        decoded.metadata_str("repository").as_deref(),
        Some("gentoo")
    );
    assert_eq!(decoded.image, b"image-payload-bytes");
}

#[test]
fn gpkg_detects_corruption() {
    let pkg = Gpkg::new(meta(), b"payload".to_vec());
    let mut encoded = pkg.encode();
    // Flip a byte in the image region (after the magic + meta length).
    let idx = encoded.len() / 2;
    encoded[idx] ^= 0xff;
    let err = Gpkg::decode(&encoded).expect_err("corruption detected");
    assert!(
        matches!(
            err,
            GpkgError::ChecksumMismatch(_) | GpkgError::Malformed(_) | GpkgError::Xpak(_)
        ),
        "got {err}"
    );
}

#[test]
fn gpkg_signature_slot_round_trips_and_verifies() {
    let pkg = Gpkg::new(meta(), b"payload".to_vec()).with_signature(b"SIGNATURE".to_vec());
    let decoded = Gpkg::decode(&pkg.encode()).expect("decode");
    assert_eq!(decoded.signature.as_deref(), Some(b"SIGNATURE".as_slice()));

    // A verifier that accepts the known signature passes; a stricter one fails.
    assert!(decoded.verify_signature(|_image, sig| sig == b"SIGNATURE"));
    assert!(!decoded.verify_signature(|_image, sig| sig == b"WRONG"));

    // An unsigned package never verifies as signed.
    let unsigned = Gpkg::new(meta(), b"payload".to_vec());
    assert!(!unsigned.verify_signature(|_, _| true));
}
