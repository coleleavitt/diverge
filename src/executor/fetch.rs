//! Distfile fetching with checksum verification.
//!
//! Modeled after emerge's fetch loop: for each `SRC_URI` distfile, try each
//! configured mirror/URI in turn via an injectable [`Fetcher`] backend, place
//! the result in `DISTDIR`, and verify it against the repository Manifest. A
//! distfile already present and verified is not re-fetched (resume/skip).
//!
//! The default [`LocalFetcher`] copies from `file://`-style local paths so the
//! flow is fully testable without network access.
//!
//! Reference:
//! - `research/portage/lib/portage/package/ebuild/fetch.py`
//! - `research/portage/lib/_emerge/EbuildFetcher.py`
//! - `research/portage/lib/portage/tests/ebuild/test_fetch.py`

use std::fmt;
use std::path::{Path, PathBuf};

use crate::manifest::{Manifest, ManifestError};

/// A source location for a distfile (one candidate URI/mirror).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The distfile name (basename stored in DISTDIR).
    pub filename: String,
    /// Candidate URIs to try in order (e.g. mirror list + upstream).
    pub uris: Vec<String>,
}

/// Error raised during fetching.
#[derive(Debug)]
pub enum FetchError {
    /// Every candidate URI failed.
    AllSourcesFailed {
        filename: String,
        tried: Vec<String>,
    },
    /// The fetched file failed Manifest verification.
    Verification(ManifestError),
    /// An I/O error writing to DISTDIR.
    Io(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllSourcesFailed { filename, tried } => write!(
                f,
                "all sources failed for '{filename}' (tried {})",
                tried.join(", ")
            ),
            Self::Verification(err) => write!(f, "verification failed: {err}"),
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for FetchError {}

impl From<ManifestError> for FetchError {
    fn from(err: ManifestError) -> Self {
        Self::Verification(err)
    }
}

/// A pluggable fetch backend: given a URI, return the file bytes (or `None` if
/// that URI is unavailable). Network backends, local copies, and test doubles
/// all implement this.
pub trait Fetcher {
    /// Attempts to retrieve `uri`. Returns `Ok(None)` when the URI is simply
    /// unavailable (try the next one); `Err` for an unexpected failure.
    fn retrieve(&mut self, uri: &str) -> Result<Option<Vec<u8>>, FetchError>;
}

/// A [`Fetcher`] that reads from local filesystem paths, accepting bare paths
/// or `file://` URIs. Used for tests and local mirrors.
#[derive(Debug, Default, Clone)]
pub struct LocalFetcher;

impl Fetcher for LocalFetcher {
    fn retrieve(&mut self, uri: &str) -> Result<Option<Vec<u8>>, FetchError> {
        let path = uri.strip_prefix("file://").unwrap_or(uri);
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(FetchError::Io(format!("{path}: {err}"))),
        }
    }
}

/// The result of fetching one distfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResult {
    pub filename: String,
    /// True when the file was already present and verified (no fetch needed).
    pub already_present: bool,
    /// The path in DISTDIR where the file now lives.
    pub path: PathBuf,
}

/// Fetches `source` into `distdir`, verifying against `manifest`.
///
/// If the distfile already exists in `distdir` and verifies, it is kept
/// (resume/skip). Otherwise each URI is tried in order until one yields bytes
/// that verify; the verified bytes are written to DISTDIR. Filesystem ownership
/// is explicit: the caller supplies `distdir`.
pub fn fetch_one(
    source: &Source,
    distdir: &Path,
    manifest: &Manifest,
    fetcher: &mut dyn Fetcher,
) -> Result<FetchResult, FetchError> {
    let dest = distdir.join(&source.filename);

    // Resume: a present, verifying distfile is not re-fetched.
    if let Ok(existing) = std::fs::read(&dest)
        && manifest.verify(&source.filename, &existing).is_ok()
    {
        return Ok(FetchResult {
            filename: source.filename.clone(),
            already_present: true,
            path: dest,
        });
    }

    let mut tried = Vec::new();
    for uri in &source.uris {
        tried.push(uri.clone());
        let Some(bytes) = fetcher.retrieve(uri)? else {
            continue; // URI unavailable, try the next.
        };
        // Verify before committing the bytes to DISTDIR.
        manifest.verify(&source.filename, &bytes)?;
        std::fs::create_dir_all(distdir)
            .map_err(|e| FetchError::Io(format!("{}: {e}", distdir.display())))?;
        std::fs::write(&dest, &bytes)
            .map_err(|e| FetchError::Io(format!("{}: {e}", dest.display())))?;
        return Ok(FetchResult {
            filename: source.filename.clone(),
            already_present: false,
            path: dest,
        });
    }

    Err(FetchError::AllSourcesFailed {
        filename: source.filename.clone(),
        tried,
    })
}

/// Fetches every source in order, returning each result. Stops at the first
/// failure (the caller decides whether to continue).
pub fn fetch_all(
    sources: &[Source],
    distdir: &Path,
    manifest: &Manifest,
    fetcher: &mut dyn Fetcher,
) -> Result<Vec<FetchResult>, FetchError> {
    sources
        .iter()
        .map(|s| fetch_one(s, distdir, manifest, fetcher))
        .collect()
}
