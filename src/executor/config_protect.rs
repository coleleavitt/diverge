//! CONFIG_PROTECT resolution, ported from Portage's `ConfigProtect` and
//! `new_protect_filename`.
//!
//! A path is "protected" when it lies under a `CONFIG_PROTECT` entry and is not
//! overridden by a longer `CONFIG_PROTECT_MASK` entry. When merging a protected
//! file that already exists, emerge writes the new version to a sibling
//! `._cfg<NNNN>_<name>` file instead of overwriting, so the admin can merge it
//! later.
//!
//! Reference: `research/portage/lib/portage/util/__init__.py`
//! (`ConfigProtect.isprotected`, `new_protect_filename`).

use crate::util::normalize_path;

/// Resolves whether destination paths are config-protected.
#[derive(Debug, Clone)]
pub struct ConfigProtect {
    protect: Vec<String>,
    protect_mask: Vec<String>,
}

impl ConfigProtect {
    /// Builds a resolver from `CONFIG_PROTECT` and `CONFIG_PROTECT_MASK` token
    /// lists. Each entry is normalized to a single leading slash.
    pub fn new(protect: &[&str], protect_mask: &[&str]) -> Self {
        let norm = |entries: &[&str]| -> Vec<String> {
            entries
                .iter()
                .filter(|e| !e.is_empty())
                .map(|e| normalize_path(&format!("/{}", e.trim_start_matches('/'))))
                .collect()
        };
        Self {
            protect: norm(protect),
            protect_mask: norm(protect_mask),
        }
    }

    /// Port of `ConfigProtect.isprotected`. `obj` must be an absolute path
    /// (single leading slash). Returns true when the path is protected and not
    /// masked. The longest matching protect entry wins, but a `protect_mask`
    /// entry at least as long flips it back to unprotected.
    pub fn is_protected(&self, obj: &str) -> bool {
        let mut masked = 0usize;
        let mut protected = 0usize;

        for ppath in &self.protect {
            if ppath.len() > masked && path_under(obj, ppath) {
                protected = ppath.len();
                for pmpath in &self.protect_mask {
                    if pmpath.len() >= protected && path_under(obj, pmpath) {
                        masked = pmpath.len();
                    }
                }
            }
        }
        protected > masked
    }

    /// Port of `new_protect_filename`: given the destination path and the list
    /// of sibling filenames already present in its directory, returns the
    /// `._cfg<NNNN>_<name>` filename to write the new config version to.
    ///
    /// `dest_exists` indicates whether the plain destination already exists; if
    /// not (and not forced) the plain destination is returned unchanged.
    pub fn protect_filename(dest_basename: &str, siblings: &[String], dest_exists: bool) -> String {
        if !dest_exists {
            return dest_basename.to_string();
        }
        // Find the highest existing ._cfgNNNN_<name> counter.
        let mut prot_num: i64 = -1;
        for pfile in siblings {
            let Some(rest) = pfile.strip_prefix("._cfg") else {
                continue;
            };
            if rest.len() < 5 {
                continue;
            }
            let (digits, suffix) = rest.split_at(4);
            // suffix begins with '_'; the remainder must equal the real name.
            let Some(name) = suffix.strip_prefix('_') else {
                continue;
            };
            if name != dest_basename {
                continue;
            }
            if let Ok(n) = digits.parse::<i64>() {
                prot_num = prot_num.max(n);
            }
        }
        let next = prot_num + 1;
        format!("._cfg{next:04}_{dest_basename}")
    }
}

/// True when `obj` is `ppath` itself or lies beneath it (`ppath/...`).
fn path_under(obj: &str, ppath: &str) -> bool {
    obj == ppath || obj.starts_with(&format!("{ppath}/"))
}
