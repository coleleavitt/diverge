//! Shared filesystem helpers for executor/integration tests.
//!
//! Included once into `tests/portage.rs`; test files reference these via
//! `crate::fs_fixture::*`.

use std::fs;
use std::path::Path;

/// Writes `content` to `path`, creating parent directories as needed.
#[allow(dead_code)]
pub fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, content).expect("write file");
}
