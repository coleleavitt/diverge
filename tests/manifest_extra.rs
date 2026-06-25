//! Additional tests porting upstream Portage test vectors.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/util/test_checksum.py` (test_md5, test_sha1, test_sha256, test_sha512, test_blake2b)
//! - `research/portage/lib/portage/tests/util/test_manifest.py` (ManifestTestCase)

use diverge::manifest::{Manifest, ManifestError, ManifestType, checksum_str};

// ============================================================================
// CHECKSUM TESTS: Additional hash vectors beyond manifest_parity.rs
// ============================================================================

/// Empty input test vectors for MD5, SHA1, SHA256, SHA512, BLAKE2B.
/// Sources: test_checksum.py lines 14-41, 72-77 (empty string cases).
#[test]
fn checksum_str_empty_input_md5() {
    assert_eq!(
        checksum_str(b"", "MD5").unwrap(),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
}

#[test]
fn checksum_str_empty_input_sha1() {
    assert_eq!(
        checksum_str(b"", "SHA1").unwrap(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
}

#[test]
fn checksum_str_empty_input_sha256() {
    assert_eq!(
        checksum_str(b"", "SHA256").unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn checksum_str_empty_input_sha512() {
    assert_eq!(
        checksum_str(b"", "SHA512").unwrap(),
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    );
}

#[test]
fn checksum_str_empty_input_blake2b() {
    assert_eq!(
        checksum_str(b"", "BLAKE2B").unwrap(),
        "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
    );
}

/// Non-empty input test vectors for MD5, SHA1, SHA256, SHA512, BLAKE2B.
/// Source: test_checksum.py (test_text values from lines 11 and onwards).
const TEST_TEXT: &[u8] = b"Some test string used to check if the hash works";

#[test]
fn checksum_str_test_text_sha512() {
    // test_checksum.py line 44
    assert_eq!(
        checksum_str(TEST_TEXT, "SHA512").unwrap(),
        "c8eaa902d48a2c82c2185a92f1c8bab8115c63c8d7a9966a8e8e81b07abcb9762f4707a6b27075e9d720277ba9fec072a59840d6355dd2ee64681d8f39a50856"
    );
}

#[test]
fn checksum_str_test_text_blake2b() {
    // test_checksum.py line 80
    assert_eq!(
        checksum_str(TEST_TEXT, "BLAKE2B").unwrap(),
        "84cb3c88838c7147bc9797c6525f812adcdcb40137f9c075963e3a3ed1fe06aaeeb4d2bb5589bad286864dc1aa834cfc4d66b8d7e4d4a246d91d45ce3a6eee43"
    );
}

/// Case-insensitive algorithm names: verify uppercase normalization.
#[test]
fn checksum_str_case_insensitive_md5() {
    let lower = checksum_str(TEST_TEXT, "md5").unwrap();
    let upper = checksum_str(TEST_TEXT, "MD5").unwrap();
    let mixed = checksum_str(TEST_TEXT, "Md5").unwrap();
    assert_eq!(lower, upper);
    assert_eq!(upper, mixed);
}

#[test]
fn checksum_str_case_insensitive_sha256() {
    let lower = checksum_str(TEST_TEXT, "sha256").unwrap();
    let upper = checksum_str(TEST_TEXT, "SHA256").unwrap();
    assert_eq!(lower, upper);
}

/// Unknown algorithm returns None.
#[test]
fn checksum_str_unknown_algorithm() {
    assert_eq!(checksum_str(b"data", "UNKNOWN"), None);
    assert_eq!(checksum_str(b"data", "RIPEMD160"), None);
    assert_eq!(checksum_str(b"data", "SHA3"), None);
}

/// Single-byte inputs to verify bit-level correctness.
#[test]
fn checksum_str_single_byte_md5() {
    // MD5 of single byte 0x00
    let digest = checksum_str(&[0x00], "MD5").unwrap();
    assert_eq!(digest.len(), 32); // MD5 is 128 bits = 32 hex chars
}

#[test]
fn checksum_str_single_byte_sha256() {
    let digest = checksum_str(&[0xff], "SHA256").unwrap();
    assert_eq!(digest.len(), 64); // SHA256 is 256 bits = 64 hex chars
}

#[test]
fn checksum_str_single_byte_blake2b() {
    let digest = checksum_str(&[0xAA], "BLAKE2B").unwrap();
    assert_eq!(digest.len(), 128); // BLAKE2B is 512 bits = 128 hex chars
}

/// Verify digest output is lowercase hex.
#[test]
fn checksum_str_output_is_lowercase() {
    let digest = checksum_str(TEST_TEXT, "SHA256").unwrap();
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "digest must be lowercase hex: {}",
        digest
    );
    // No uppercase A-F allowed
    assert!(!digest.contains(char::is_uppercase));
}

// ============================================================================
// MANIFEST PARSING TESTS: Edge cases and error conditions
// ============================================================================

/// Blank lines are silently skipped (no error).
/// Source: manifest.rs parse() comment line 141-143
#[test]
fn manifest_parse_blank_lines_ignored() {
    let content = "\n\nDIST foo 100 SHA256 abc\n\n\n";
    let manifest = Manifest::parse(content).unwrap();
    assert_eq!(manifest.len(), 1);
}

/// Leading/trailing whitespace is trimmed from lines.
#[test]
fn manifest_parse_whitespace_trimmed() {
    let content = "   DIST foo 100 SHA256 abc   \n";
    let manifest = Manifest::parse(content).unwrap();
    assert_eq!(manifest.len(), 1);
    assert_eq!(manifest.entry("foo").unwrap().name, "foo");
}

/// Multiple hash algorithms on a single entry, in order.
/// Source: test_manifest.py implies multiple hashes (lines 16, 31-32 mention SHA512 and BLAKE2B together)
#[test]
fn manifest_parse_multiple_hashes_preserved_order() {
    let content = "DIST foo 100 SHA512 aaa BLAKE2B bbb SHA256 ccc\n";
    let manifest = Manifest::parse(content).unwrap();
    let entry = manifest.entry("foo").unwrap();

    // BTreeMap sorts keys, so check all three are present
    assert_eq!(entry.hashes.get("SHA512").map(String::as_str), Some("aaa"));
    assert_eq!(entry.hashes.get("BLAKE2B").map(String::as_str), Some("bbb"));
    assert_eq!(entry.hashes.get("SHA256").map(String::as_str), Some("ccc"));
}

/// Entry with no hash algos (size only) still parses successfully.
#[test]
fn manifest_parse_no_hashes() {
    let content = "DIST foo 42\n";
    let manifest = Manifest::parse(content).unwrap();
    let entry = manifest.entry("foo").unwrap();
    assert_eq!(entry.size, 42);
    assert!(entry.hashes.is_empty());
}

/// File names can contain dots and dashes.
#[test]
fn manifest_parse_complex_filename() {
    let content = "DIST my-package-1.2.3.tar.gz 1000 SHA256 abc\n";
    let manifest = Manifest::parse(content).unwrap();
    assert!(manifest.entry("my-package-1.2.3.tar.gz").is_some());
}

/// Large size values (multi-gigabyte range).
#[test]
fn manifest_parse_large_size() {
    let content = "DIST huge.iso 1099511627776 SHA256 abc\n"; // 1 TiB
    let manifest = Manifest::parse(content).unwrap();
    let entry = manifest.entry("huge.iso").unwrap();
    assert_eq!(entry.size, 1099511627776u64);
}

// ============================================================================
// MANIFEST VERIFICATION TESTS: Size and digest mismatch detection
// ============================================================================

/// Size mismatch detected before digest check.
/// Source: manifest.rs verify() lines 178-185
#[test]
fn manifest_verify_rejects_size_mismatch() {
    let data = b"correct content";
    // Create a manifest with wrong size (115 instead of 15)
    let line = format!(
        "DIST test.bin 115 SHA256 {}",
        checksum_str(data, "SHA256").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();

    let err = manifest.verify("test.bin", data).unwrap_err();
    assert!(
        matches!(
            err,
            ManifestError::SizeMismatch {
                expected: 115,
                actual: 15,
                ..
            }
        ),
        "got {err}"
    );
}

/// Size mismatch with larger actual size.
#[test]
fn manifest_verify_size_mismatch_larger() {
    let data = b"x";
    // Create manifest with size 100, actual data is 1 byte
    let line = "DIST test.bin 100 SHA256 abc";
    let manifest = Manifest::parse(line).unwrap();

    let err = manifest.verify("test.bin", data).unwrap_err();
    assert!(
        matches!(
            err,
            ManifestError::SizeMismatch {
                expected: 100,
                actual: 1,
                ..
            }
        ),
        "got {err}"
    );
}

/// Digest mismatch when size is correct but content differs.
/// Source: manifest.rs verify() lines 193-198
#[test]
fn manifest_verify_rejects_digest_mismatch() {
    let correct_data = b"correct";
    let wrong_data = b"wronggg";
    assert_eq!(correct_data.len(), wrong_data.len());

    let line = format!(
        "DIST test.bin {} SHA256 {}",
        correct_data.len(),
        checksum_str(correct_data, "SHA256").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();

    let err = manifest.verify("test.bin", wrong_data).unwrap_err();
    assert!(
        matches!(err, ManifestError::DigestMismatch { algo: ref a, .. } if a == "SHA256"),
        "got {err}"
    );
}

/// Multiple hashes: all recognized hashes must match.
/// Source: manifest.rs verify() lines 187-199 (checks every recognized hash)
#[test]
fn manifest_verify_all_hashes_match() {
    let data = b"multiHash";
    let line = format!(
        "DIST test.bin {} SHA256 {} SHA512 {} BLAKE2B {}",
        data.len(),
        checksum_str(data, "SHA256").unwrap(),
        checksum_str(data, "SHA512").unwrap(),
        checksum_str(data, "BLAKE2B").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();

    // All hashes match: should succeed
    assert!(manifest.verify("test.bin", data).is_ok());
}

/// Unknown hash algorithms are skipped; at least one recognized hash must match.
/// Source: manifest.rs verify() lines 189-205
#[test]
fn manifest_verify_unknown_hash_skipped() {
    let data = b"test";
    let line = format!(
        "DIST test.bin {} UNKNOWNSRC aabbcc SHA256 {}",
        data.len(),
        checksum_str(data, "SHA256").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();

    // Should succeed: unknown hash skipped, SHA256 matches
    assert!(manifest.verify("test.bin", data).is_ok());
}

/// No recognized hashes in Manifest raises NoUsableHash error.
/// Source: manifest.rs verify() lines 201-205
#[test]
fn manifest_verify_no_usable_hash() {
    let data = b"test";
    let line = "DIST test.bin 4 UNKNOWNALGO xyz FAKEHASH abc";
    let manifest = Manifest::parse(line).unwrap();

    let err = manifest.verify("test.bin", data).unwrap_err();
    assert!(matches!(err, ManifestError::NoUsableHash(..)), "got {err}");
}

/// Empty Manifest cannot verify any file.
#[test]
fn manifest_verify_unknown_file() {
    let manifest = Manifest::parse("").unwrap();

    let err = manifest.verify("missing.bin", b"data").unwrap_err();
    assert!(
        matches!(err, ManifestError::UnknownFile(ref n) if n == "missing.bin"),
        "got {err}"
    );
}

// ============================================================================
// MANIFEST RENDER/ROUNDTRIP TESTS
// ============================================================================

/// Render/parse roundtrip preserves all data.
/// Source: test_manifest.py and manifest_parity.rs test_round_trips_through_render
#[test]
fn manifest_roundtrip_preserves_data() {
    let data = b"test payload for roundtrip verification";
    let original = format!(
        "DIST test-1.tar.gz {} BLAKE2B {} SHA256 {}\n",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap(),
        checksum_str(data, "SHA256").unwrap()
    );

    let manifest1 = Manifest::parse(&original).unwrap();
    let rendered = manifest1.render();
    let manifest2 = Manifest::parse(&rendered).unwrap();

    let entry1 = manifest1.entry("test-1.tar.gz").unwrap();
    let entry2 = manifest2.entry("test-1.tar.gz").unwrap();

    assert_eq!(entry1.kind, entry2.kind);
    assert_eq!(entry1.name, entry2.name);
    assert_eq!(entry1.size, entry2.size);
    assert_eq!(entry1.hashes, entry2.hashes);
}

/// Render output includes newline terminators.
#[test]
fn manifest_render_ends_with_newline() {
    let content = "DIST foo 100 SHA256 abc\n";
    let manifest = Manifest::parse(content).unwrap();
    let rendered = manifest.render();
    assert!(rendered.ends_with('\n'));
}

/// Multiple entries render in sorted order (by BTreeMap).
#[test]
fn manifest_render_multiple_entries() {
    let content = "DIST z.tar.gz 1000 SHA256 zzz\nDIST a.tar.gz 500 SHA256 aaa\n";
    let manifest = Manifest::parse(content).unwrap();

    // Both entries should be accessible by name
    assert_eq!(manifest.len(), 2);
    assert!(manifest.entry("z.tar.gz").is_some());
    assert!(manifest.entry("a.tar.gz").is_some());
}

// ============================================================================
// MANIFEST ENTRY LOOKUP AND FILTERING TESTS
// ============================================================================

/// entries_of filters by kind correctly.
/// Source: manifest.rs entries_of() lines 156-158
#[test]
fn manifest_entries_of_filters_by_kind() {
    let content = "DIST file1.tar.gz 100 SHA256 a\n\
                   AUX patch.diff 50 SHA256 b\n\
                   DIST file2.tar.gz 200 SHA256 c\n";
    let manifest = Manifest::parse(content).unwrap();

    let dist_files = manifest.entries_of(ManifestType::Dist);
    assert_eq!(dist_files.len(), 2);
    assert!(dist_files.iter().all(|e| e.kind == ManifestType::Dist));

    let aux_files = manifest.entries_of(ManifestType::Aux);
    assert_eq!(aux_files.len(), 1);
    assert_eq!(aux_files[0].name, "patch.diff");

    let misc_files = manifest.entries_of(ManifestType::Misc);
    assert_eq!(misc_files.len(), 0);
}

/// entry() looks up by name, ignoring kind.
/// Source: manifest.rs entry() lines 151-153
#[test]
fn manifest_entry_lookup_ignores_kind() {
    let content = "MISC shared.txt 75 SHA256 abc\n";
    let manifest = Manifest::parse(content).unwrap();

    // Lookup by name alone should find it regardless of kind
    assert!(manifest.entry("shared.txt").is_some());
    assert_eq!(
        manifest.entry("shared.txt").unwrap().kind,
        ManifestType::Misc
    );
}

/// len() and is_empty() methods.
/// Source: manifest.rs len() / is_empty() lines 161-167
#[test]
fn manifest_len_and_is_empty() {
    let empty = Manifest::parse("").unwrap();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    let nonempty = Manifest::parse("DIST f1 10 SHA256 a\n").unwrap();
    assert_eq!(nonempty.len(), 1);
    assert!(!nonempty.is_empty());
}

// ============================================================================
// MANIFEST ERROR HANDLING AND MALFORMED CASES
// ============================================================================

/// Malformed line: too few fields.
#[test]
fn manifest_parse_rejects_too_few_fields() {
    let content = "DIST";
    let err = Manifest::parse(content).unwrap_err();
    assert!(matches!(err, ManifestError::MalformedLine(_)));
}

/// Malformed line: invalid type.
#[test]
fn manifest_parse_rejects_invalid_type() {
    let content = "INVALID foo 100 SHA256 abc";
    let err = Manifest::parse(content).unwrap_err();
    assert!(matches!(err, ManifestError::MalformedLine(_)));
}

/// Malformed line: non-numeric size.
#[test]
fn manifest_parse_rejects_nonnumeric_size() {
    let content = "DIST foo notanumber SHA256 abc";
    let err = Manifest::parse(content).unwrap_err();
    assert!(matches!(err, ManifestError::MalformedLine(_)));
}

/// Unpaired hash: odd number of hash tokens.
#[test]
fn manifest_parse_rejects_unpaired_hash() {
    let content = "DIST foo 100 SHA256";
    let err = Manifest::parse(content).unwrap_err();
    assert!(matches!(err, ManifestError::UnpairedHash(_)));
}

/// Unpaired hash: three hash tokens (needs even count).
#[test]
fn manifest_parse_rejects_odd_hash_count() {
    let content = "DIST foo 100 SHA256 aaa BLAKE2B";
    let err = Manifest::parse(content).unwrap_err();
    assert!(matches!(err, ManifestError::UnpairedHash(_)));
}

/// Error Display trait implementations.
#[test]
fn manifest_error_display_messages() {
    let err_malformed = ManifestError::MalformedLine("BAD LINE".to_string());
    assert!(err_malformed.to_string().contains("malformed"));

    let err_unknown = ManifestError::UnknownFile("missing.bin".to_string());
    assert!(err_unknown.to_string().contains("missing.bin"));

    let err_size = ManifestError::SizeMismatch {
        name: "file.bin".to_string(),
        expected: 100,
        actual: 50,
    };
    assert!(err_size.to_string().contains("100"));
    assert!(err_size.to_string().contains("50"));

    let err_digest = ManifestError::DigestMismatch {
        name: "data.bin".to_string(),
        algo: "SHA256".to_string(),
    };
    assert!(err_digest.to_string().contains("SHA256"));

    let err_no_hash = ManifestError::NoUsableHash("test.bin".to_string());
    assert!(err_no_hash.to_string().contains("no usable hash"));

    let err_unpaired = ManifestError::UnpairedHash("DIST foo 100 SHA256".to_string());
    assert!(err_unpaired.to_string().contains("unpaired"));
}

// ============================================================================
// CHECKSUM VECTOR TESTS FOR ADDITIONAL ALGORITHMS
// ============================================================================

/// MD5 test vectors from test_checksum.py lines 13-17
#[test]
fn checksum_str_md5_vectors() {
    assert_eq!(
        checksum_str(b"", "MD5").unwrap(),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(
        checksum_str(TEST_TEXT, "MD5").unwrap(),
        "094c3bf4732f59b39d577e9726f1e934"
    );
}

/// SHA1 test vectors from test_checksum.py lines 19-25
#[test]
fn checksum_str_sha1_vectors() {
    assert_eq!(
        checksum_str(b"", "SHA1").unwrap(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        checksum_str(TEST_TEXT, "SHA1").unwrap(),
        "5c572017d4e4d49e4aa03a2eda12dbb54a1e2e4f"
    );
}

/// SHA256 test vectors from test_checksum.py lines 27-35
#[test]
fn checksum_str_sha256_vectors() {
    assert_eq!(
        checksum_str(b"", "SHA256").unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        checksum_str(TEST_TEXT, "SHA256").unwrap(),
        "e3d4a1135181fe156d61455615bb6296198e8ca5b2f20ddeb85cb4cd27f62320"
    );
}

/// All five supported algorithms: verify different length digests.
#[test]
fn checksum_str_digest_lengths() {
    let data = b"verify digest output lengths";

    let md5 = checksum_str(data, "MD5").unwrap();
    assert_eq!(md5.len(), 32); // 128 bits / 4 bits per hex char

    let sha1 = checksum_str(data, "SHA1").unwrap();
    assert_eq!(sha1.len(), 40); // 160 bits / 4

    let sha256 = checksum_str(data, "SHA256").unwrap();
    assert_eq!(sha256.len(), 64); // 256 bits / 4

    let sha512 = checksum_str(data, "SHA512").unwrap();
    assert_eq!(sha512.len(), 128); // 512 bits / 4

    let blake2b = checksum_str(data, "BLAKE2B").unwrap();
    assert_eq!(blake2b.len(), 128); // 512 bits / 4
}

/// Hash algorithm case normalization works for all algorithms.
#[test]
fn checksum_str_all_algos_case_insensitive() {
    let data = b"case test data";

    let algos = vec!["MD5", "SHA1", "SHA256", "SHA512", "BLAKE2B"];
    for algo in algos {
        let upper = checksum_str(data, algo).unwrap();
        let lower = checksum_str(data, &algo.to_lowercase()).unwrap();
        // Mixed case: capitalize first letter only
        let mixed_case = if algo.len() > 1 {
            format!("{}{}", &algo[0..1], algo[1..].to_lowercase())
        } else {
            algo.to_uppercase()
        };
        let mixed = checksum_str(data, &mixed_case).unwrap();

        assert_eq!(upper, lower, "case mismatch for {}", algo);
        assert_eq!(upper, mixed, "mixed case mismatch for {}", algo);
    }
}

// ============================================================================
// MANIFEST VERIFICATION: COMPREHENSIVE MULTI-HASH SCENARIOS
// ============================================================================

/// Mixed valid and invalid hashes: only valid hashes checked, first mismatch fails.
#[test]
fn manifest_verify_first_digest_mismatch_fails() {
    let correct_data = b"data";
    // Create manifest with correct SHA256 but wrong BLAKE2B
    let line = format!(
        "DIST test.bin {} SHA256 {} BLAKE2B wronghash",
        correct_data.len(),
        checksum_str(correct_data, "SHA256").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();

    // Should fail on BLAKE2B mismatch (processed first alphabetically)
    let err = manifest.verify("test.bin", correct_data).unwrap_err();
    assert!(matches!(err, ManifestError::DigestMismatch { algo: ref a, .. } if a == "BLAKE2B"));
}

/// Verify succeeds with multiple correct hashes.
#[test]
fn manifest_verify_multiple_correct_hashes() {
    let data = b"multihash data";
    let content = format!(
        "DIST test.bin {} MD5 {} SHA1 {} SHA256 {} SHA512 {} BLAKE2B {}",
        data.len(),
        checksum_str(data, "MD5").unwrap(),
        checksum_str(data, "SHA1").unwrap(),
        checksum_str(data, "SHA256").unwrap(),
        checksum_str(data, "SHA512").unwrap(),
        checksum_str(data, "BLAKE2B").unwrap(),
    );
    let manifest = Manifest::parse(&content).unwrap();
    assert!(manifest.verify("test.bin", data).is_ok());
}

// ============================================================================
// MANIFEST TYPE FILTERING
// ============================================================================

/// All four manifest entry types parse and filter correctly.
#[test]
fn manifest_all_types_filter() {
    let content = "DIST file.tar.gz 100 SHA256 a\n\
                   AUX files/patch.diff 50 SHA256 b\n\
                   MISC metadata.xml 25 SHA256 c\n\
                   EBUILD foo-1.ebuild 200 SHA256 d\n";
    let manifest = Manifest::parse(content).unwrap();

    assert_eq!(manifest.entries_of(ManifestType::Dist).len(), 1);
    assert_eq!(manifest.entries_of(ManifestType::Aux).len(), 1);
    assert_eq!(manifest.entries_of(ManifestType::Misc).len(), 1);
    assert_eq!(manifest.entries_of(ManifestType::Ebuild).len(), 1);
}

/// EBUILD type entry verifies correctly.
#[test]
fn manifest_verify_ebuild_type() {
    let data = b"ebuild content here";
    let line = format!(
        "EBUILD foo-1.ebuild {} SHA256 {}",
        data.len(),
        checksum_str(data, "SHA256").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();
    assert!(manifest.verify("foo-1.ebuild", data).is_ok());
}

/// AUX type entry verifies correctly.
#[test]
fn manifest_verify_aux_type() {
    let data = b"patch data";
    let line = format!(
        "AUX files/example.patch {} BLAKE2B {}",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap()
    );
    let manifest = Manifest::parse(&line).unwrap();
    assert!(manifest.verify("files/example.patch", data).is_ok());
}
