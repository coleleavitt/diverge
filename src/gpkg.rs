//! Gentoo binary package (gpkg) container modeling.
//!
//! A gpkg is a tar container holding the package's metadata, the install image,
//! and (optionally) a detached signature. This module models the container's
//! logical structure — metadata key/values (stored via the [`crate::xpak`]
//! segment encoding), the image payload, and per-member checksums — with
//! build/parse round-tripping and integrity verification.
//!
//! GPG signing itself is out of scope (no real crypto); the signature slot is
//! modeled as opaque bytes plus a verification hook, so the surrounding
//! checksum-and-structure behavior is testable.
//!
//! Reference:
//! - `research/portage/lib/portage/gpkg.py`
//! - `research/portage/lib/portage/tests/gpkg/test_gpkg_checksum.py`,
//!   `test_gpkg_metadata_update.py`, `test_gpkg_size.py`

use std::collections::BTreeMap;
use std::fmt;

use crate::manifest::checksum_str;
use crate::xpak::{XpakError, xpak_mem, xpak_parse};

/// The checksum algorithm gpkg members are verified with.
const GPKG_HASH: &str = "BLAKE2B";

/// A binary package container: metadata, image payload, optional signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpkg {
    /// Metadata files (`SLOT`, `USE`, `DEPEND`, `repository`, ...).
    pub metadata: BTreeMap<String, Vec<u8>>,
    /// The install-image payload (e.g. a compressed tar of `D`).
    pub image: Vec<u8>,
    /// Optional detached signature over the image+metadata.
    pub signature: Option<Vec<u8>>,
}

/// Error raised building or verifying a gpkg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpkgError {
    /// The container framing was malformed.
    Malformed(String),
    /// A member's recorded checksum did not match its bytes.
    ChecksumMismatch(String),
    /// The embedded xpak metadata segment failed to parse.
    Xpak(XpakError),
}

impl fmt::Display for GpkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed gpkg: {m}"),
            Self::ChecksumMismatch(m) => write!(f, "gpkg checksum mismatch: {m}"),
            Self::Xpak(e) => write!(f, "gpkg metadata: {e}"),
        }
    }
}

impl std::error::Error for GpkgError {}

impl From<XpakError> for GpkgError {
    fn from(e: XpakError) -> Self {
        Self::Xpak(e)
    }
}

const MAGIC: &[u8] = b"GPKG1\0";

impl Gpkg {
    /// Builds a gpkg from metadata and an image payload (unsigned).
    pub fn new(metadata: BTreeMap<String, Vec<u8>>, image: Vec<u8>) -> Self {
        Self {
            metadata,
            image,
            signature: None,
        }
    }

    /// Attaches a (opaque) detached signature.
    pub fn with_signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = Some(signature);
        self
    }

    /// Reads one metadata value (e.g. `SLOT`) as a UTF-8 string.
    pub fn metadata_str(&self, key: &str) -> Option<String> {
        self.metadata
            .get(key)
            .map(|v| String::from_utf8_lossy(v).into_owned())
    }

    /// The on-disk size of the serialized container.
    pub fn size(&self) -> usize {
        self.encode().len()
    }

    /// Serializes the container to bytes:
    /// `MAGIC | u32 meta_len | meta(xpak) | u32 image_len | image | u32 sig_len | sig`
    /// followed by a trailing BLAKE2B checksum of everything above.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);

        let meta = xpak_mem(&self.metadata);
        push_block(&mut out, &meta);
        push_block(&mut out, &self.image);
        push_block(&mut out, self.signature.as_deref().unwrap_or(&[]));

        // Trailing integrity checksum over the framed content.
        let digest = checksum_str(&out, GPKG_HASH).unwrap_or_default();
        push_block(&mut out, digest.as_bytes());
        out
    }

    /// Parses and verifies a serialized container.
    pub fn decode(bytes: &[u8]) -> Result<Self, GpkgError> {
        if !bytes.starts_with(MAGIC) {
            return Err(GpkgError::Malformed("bad magic".to_string()));
        }
        let mut pos = MAGIC.len();
        let meta = take_block(bytes, &mut pos)?;
        let image = take_block(bytes, &mut pos)?;
        let signature = take_block(bytes, &mut pos)?;
        let recorded_digest = take_block(bytes, &mut pos)?;

        // Verify the trailing checksum over everything before it.
        let content_end = pos - (recorded_digest.len() + 4);
        let actual = checksum_str(&bytes[..content_end], GPKG_HASH).unwrap_or_default();
        if actual.as_bytes() != recorded_digest.as_slice() {
            return Err(GpkgError::ChecksumMismatch(
                "container integrity digest".to_string(),
            ));
        }

        let metadata = xpak_parse(&meta)?;
        Ok(Self {
            metadata,
            image,
            signature: if signature.is_empty() {
                None
            } else {
                Some(signature)
            },
        })
    }

    /// Verifies an attached signature with the provided verifier. Returns
    /// `Ok(true)` when signed-and-valid, `Ok(false)` when unsigned, and the
    /// verifier's error otherwise. (No crypto is performed here.)
    pub fn verify_signature(&self, verify: impl Fn(&[u8], &[u8]) -> bool) -> bool {
        match &self.signature {
            Some(sig) => verify(&self.image, sig),
            None => false,
        }
    }
}

fn push_block(out: &mut Vec<u8>, block: &[u8]) {
    out.extend_from_slice(&(block.len() as u32).to_be_bytes());
    out.extend_from_slice(block);
}

fn take_block(bytes: &[u8], pos: &mut usize) -> Result<Vec<u8>, GpkgError> {
    let len_bytes = bytes
        .get(*pos..*pos + 4)
        .ok_or_else(|| GpkgError::Malformed("truncated length".to_string()))?;
    let len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
    *pos += 4;
    let block = bytes
        .get(*pos..*pos + len)
        .ok_or_else(|| GpkgError::Malformed("truncated block".to_string()))?
        .to_vec();
    *pos += len;
    Ok(block)
}
