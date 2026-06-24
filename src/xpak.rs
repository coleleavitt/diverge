//! XPAK binary-package metadata segment, ported from Portage's `xpak.py`.
//!
//! An XPAK segment stores a package's metadata files (`SLOT`, `USE`, `DEPEND`,
//! ...) appended after the tarball in a `.tbz2`/`.xpak`. The format is:
//!
//! ```text
//! "XPAKPACK" <u32 index_len> <u32 data_len> <index> <data> "XPAKSTOP"
//! index := repeated [ <u32 name_len> <name> <u32 data_offset> <u32 data_len> ]
//! ```
//!
//! All integers are 4-byte big-endian. This module ports `encodeint`/
//! `decodeint`, `xpak_mem` (build), and `xsplit_mem`/`getindex` (parse).
//!
//! Reference: `research/portage/lib/portage/xpak.py`,
//! `research/portage/lib/portage/tests/xpak/test_decodeint.py`

use std::collections::BTreeMap;

/// Port of `encodeint`: a u32 as 4 big-endian bytes.
pub fn encodeint(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

/// Port of `decodeint`: 4 big-endian bytes to a u32. Returns `None` if fewer
/// than 4 bytes are available.
pub fn decodeint(bytes: &[u8]) -> Option<u32> {
    let arr: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
    Some(u32::from_be_bytes(arr))
}

const MAGIC_START: &[u8] = b"XPAKPACK";
const MAGIC_STOP: &[u8] = b"XPAKSTOP";

/// Error raised when parsing an XPAK segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XpakError {
    /// The segment did not start with `XPAKPACK` or end with `XPAKSTOP`.
    BadMagic,
    /// The segment was truncated relative to its declared lengths.
    Truncated,
}

impl std::fmt::Display for XpakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic => f.write_str("invalid XPAK magic"),
            Self::Truncated => f.write_str("truncated XPAK segment"),
        }
    }
}

impl std::error::Error for XpakError {}

/// Port of `xpak_mem`: builds an XPAK segment from a name -> bytes map. Entries
/// are emitted in sorted key order for determinism.
pub fn xpak_mem(data: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut index = Vec::new();
    let mut blob = Vec::new();
    let mut data_pos: u32 = 0;

    for (name, value) in data {
        let name_bytes = name.as_bytes();
        let size = value.len() as u32;
        index.extend_from_slice(&encodeint(name_bytes.len() as u32));
        index.extend_from_slice(name_bytes);
        index.extend_from_slice(&encodeint(data_pos));
        index.extend_from_slice(&encodeint(size));
        blob.extend_from_slice(value);
        data_pos += size;
    }

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC_START);
    out.extend_from_slice(&encodeint(index.len() as u32));
    out.extend_from_slice(&encodeint(blob.len() as u32));
    out.extend_from_slice(&index);
    out.extend_from_slice(&blob);
    out.extend_from_slice(MAGIC_STOP);
    out
}

/// Port of `xsplit_mem` + `getindex`: parses an XPAK segment into a name ->
/// bytes map.
pub fn xpak_parse(segment: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, XpakError> {
    if segment.len() < 24 || !segment.starts_with(MAGIC_START) || !segment.ends_with(MAGIC_STOP) {
        return Err(XpakError::BadMagic);
    }
    let index_len = decodeint(&segment[8..12]).ok_or(XpakError::Truncated)? as usize;
    let data_len = decodeint(&segment[12..16]).ok_or(XpakError::Truncated)? as usize;

    let index_start = 16;
    let data_start = index_start + index_len;
    let data_end = data_start + data_len;
    if data_end + MAGIC_STOP.len() > segment.len() {
        return Err(XpakError::Truncated);
    }

    let index = &segment[index_start..data_start];
    let data = &segment[data_start..data_end];

    let mut result = BTreeMap::new();
    let mut pos = 0usize;
    while pos < index.len() {
        let name_len = decodeint(index.get(pos..pos + 4).ok_or(XpakError::Truncated)?)
            .ok_or(XpakError::Truncated)? as usize;
        pos += 4;
        let name_bytes = index.get(pos..pos + name_len).ok_or(XpakError::Truncated)?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        pos += name_len;
        let offset = decodeint(index.get(pos..pos + 4).ok_or(XpakError::Truncated)?)
            .ok_or(XpakError::Truncated)? as usize;
        pos += 4;
        let size = decodeint(index.get(pos..pos + 4).ok_or(XpakError::Truncated)?)
            .ok_or(XpakError::Truncated)? as usize;
        pos += 4;
        let value = data
            .get(offset..offset + size)
            .ok_or(XpakError::Truncated)?;
        result.insert(name, value.to_vec());
    }
    Ok(result)
}
