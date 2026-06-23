//! Manifest parsing and checksum verification, ported from Portage.
//!
//! A Manifest2 file has one entry per line:
//! `TYPE filename size HASH1 value1 HASH2 value2 ...` where `TYPE` is one of
//! `AUX`, `MISC`, `DIST`, `EBUILD`. This module parses those lines and verifies
//! a file's bytes against the recorded size and digests.
//!
//! Reference:
//! - `research/portage/lib/portage/manifest.py`
//! - `research/portage/lib/portage/checksum.py`
//! - `research/portage/lib/portage/tests/util/test_checksum.py`

use std::collections::BTreeMap;
use std::fmt;

use blake2::Blake2b512;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

/// Computes a lowercase hex digest of `data` for a Manifest hash name.
/// Supported names mirror Portage's `checksum` identifiers; unknown names
/// return `None`.
pub fn checksum_str(data: &[u8], algo: &str) -> Option<String> {
    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
    match algo.to_ascii_uppercase().as_str() {
        "MD5" => Some(hex(&Md5::digest(data))),
        "SHA1" => Some(hex(&Sha1::digest(data))),
        "SHA256" => Some(hex(&Sha256::digest(data))),
        "SHA512" => Some(hex(&Sha512::digest(data))),
        "BLAKE2B" => Some(hex(&Blake2b512::digest(data))),
        _ => None,
    }
}

/// The entry kind in the first column of a Manifest line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestType {
    Aux,
    Misc,
    Dist,
    Ebuild,
}

impl ManifestType {
    fn parse(token: &str) -> Option<Self> {
        match token {
            "AUX" => Some(Self::Aux),
            "MISC" => Some(Self::Misc),
            "DIST" => Some(Self::Dist),
            "EBUILD" => Some(Self::Ebuild),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Aux => "AUX",
            Self::Misc => "MISC",
            Self::Dist => "DIST",
            Self::Ebuild => "EBUILD",
        }
    }
}

/// One parsed Manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub kind: ManifestType,
    pub name: String,
    pub size: u64,
    /// Hash name (uppercase) -> recorded hex digest, in file order.
    pub hashes: BTreeMap<String, String>,
}

/// Error raised when parsing or verifying a Manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// A line did not match the `TYPE name size hash...` grammar.
    MalformedLine(String),
    /// A hash column list had an odd length (missing a value).
    UnpairedHash(String),
    /// The requested file is not present in the Manifest.
    UnknownFile(String),
    /// The file's size did not match the recorded size.
    SizeMismatch {
        name: String,
        expected: u64,
        actual: u64,
    },
    /// A digest did not match the recorded value.
    DigestMismatch { name: String, algo: String },
    /// No usable (recognized) hash was recorded for the file.
    NoUsableHash(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedLine(line) => write!(f, "malformed manifest line: '{line}'"),
            Self::UnpairedHash(line) => write!(f, "unpaired hash in manifest line: '{line}'"),
            Self::UnknownFile(name) => write!(f, "file not in manifest: '{name}'"),
            Self::SizeMismatch {
                name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "size mismatch for '{name}': expected {expected}, got {actual}"
                )
            }
            Self::DigestMismatch { name, algo } => {
                write!(f, "{algo} digest mismatch for '{name}'")
            }
            Self::NoUsableHash(name) => write!(f, "no usable hash for '{name}'"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// A parsed Manifest: entries keyed by `(type, name)` preserving file order.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    entries: Vec<ManifestEntry>,
}

impl Manifest {
    /// Parses Manifest text. Blank lines are skipped; unknown leading tokens
    /// cause a [`ManifestError::MalformedLine`].
    pub fn parse(content: &str) -> Result<Self, ManifestError> {
        let mut entries = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            entries.push(parse_entry(line)?);
        }
        Ok(Self { entries })
    }

    /// Looks up an entry by file name (ignoring type).
    pub fn entry(&self, name: &str) -> Option<&ManifestEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// All entries of a given type.
    pub fn entries_of(&self, kind: ManifestType) -> Vec<&ManifestEntry> {
        self.entries.iter().filter(|e| e.kind == kind).collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Verifies `data` against the recorded size and every recognized digest
    /// for `name`. Unknown hash algorithms in the Manifest are skipped, but at
    /// least one recognized hash must match (mirrors Portage requiring a usable
    /// hash). Returns `Ok(())` on success.
    pub fn verify(&self, name: &str, data: &[u8]) -> Result<(), ManifestError> {
        let entry = self
            .entry(name)
            .ok_or_else(|| ManifestError::UnknownFile(name.to_string()))?;

        let actual_size = data.len() as u64;
        if actual_size != entry.size {
            return Err(ManifestError::SizeMismatch {
                name: name.to_string(),
                expected: entry.size,
                actual: actual_size,
            });
        }

        let mut checked_any = false;
        for (algo, expected) in &entry.hashes {
            let Some(actual) = checksum_str(data, algo) else {
                continue; // Unknown algorithm: skip, like Portage's filter.
            };
            checked_any = true;
            if &actual != expected {
                return Err(ManifestError::DigestMismatch {
                    name: name.to_string(),
                    algo: algo.clone(),
                });
            }
        }

        if checked_any {
            Ok(())
        } else {
            Err(ManifestError::NoUsableHash(name.to_string()))
        }
    }

    /// Renders the Manifest back to its canonical text form (entries in order).
    pub fn render(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(entry.kind.as_str());
            out.push(' ');
            out.push_str(&entry.name);
            out.push(' ');
            out.push_str(&entry.size.to_string());
            for (algo, value) in &entry.hashes {
                out.push_str(&format!(" {algo} {value}"));
            }
            out.push('\n');
        }
        out
    }
}

fn parse_entry(line: &str) -> Result<ManifestEntry, ManifestError> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(ManifestError::MalformedLine(line.to_string()));
    }
    let kind = ManifestType::parse(tokens[0])
        .ok_or_else(|| ManifestError::MalformedLine(line.to_string()))?;
    let name = tokens[1].to_string();
    let size: u64 = tokens[2]
        .parse()
        .map_err(|_| ManifestError::MalformedLine(line.to_string()))?;

    let rest = &tokens[3..];
    if !rest.len().is_multiple_of(2) {
        return Err(ManifestError::UnpairedHash(line.to_string()));
    }
    let mut hashes = BTreeMap::new();
    for pair in rest.chunks_exact(2) {
        hashes.insert(pair[0].to_ascii_uppercase(), pair[1].to_string());
    }
    Ok(ManifestEntry {
        kind,
        name,
        size,
        hashes,
    })
}
