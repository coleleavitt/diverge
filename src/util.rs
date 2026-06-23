//! Shared config-file utilities ported from `portage.util`.
//!
//! These reproduce the observable behavior of upstream `normalize_path`,
//! `grabfile`/`grablines`, `grabdict`, `stack_lists`, and `stack_dicts`
//! (`research/portage/lib/portage/util/__init__.py`). The profile/config
//! layer composes these to read `make.conf`, profile files, and the
//! `package.*` config directories.

use std::collections::BTreeMap;

/// Port of `portage.util.normalize_path`: collapses redundant separators and
/// `.`/`..` components without resolving symlinks, special-casing a single
/// leading slash (POSIX keeps `//` but not `///`).
pub fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let leading_slashes = path.bytes().take_while(|&b| b == b'/').count();
    // POSIX: exactly two leading slashes are preserved, one or 3+ collapse to 1.
    let prefix = if leading_slashes == 2 {
        "//"
    } else if leading_slashes >= 1 {
        "/"
    } else {
        ""
    };

    let mut stack: Vec<&str> = Vec::new();
    let absolute = !prefix.is_empty();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => apply_parent_component(&mut stack, absolute),
            other => stack.push(other),
        }
    }

    let joined = stack.join("/");
    if prefix.is_empty() {
        if joined.is_empty() {
            ".".to_string()
        } else {
            joined
        }
    } else {
        format!("{prefix}{joined}")
    }
}

/// Applies a `..` path component to the component stack. An absolute path
/// cannot ascend past root (the `..` is dropped); a relative path keeps a
/// leading `..` run when it cannot cancel a prior component.
fn apply_parent_component(stack: &mut Vec<&str>, absolute: bool) {
    if absolute {
        stack.pop();
    } else if matches!(stack.last(), Some(&"..") | None) {
        stack.push("..");
    } else {
        stack.pop();
    }
}

/// Normalizes one raw line the way `grabfile`/`grabdict` do: collapse internal
/// whitespace, then drop everything at and after the first token that begins
/// with `#`. Returns the resulting space-joined tokens.
fn strip_comment_tokens(line: &str) -> Vec<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut kept = Vec::new();
    for token in tokens {
        if token.starts_with('#') {
            break;
        }
        kept.push(token.to_string());
    }
    kept
}

/// Port of `portage.util.grabfile` (non-recursive, no compat-level handling):
/// returns the non-empty, non-comment lines of `content` with whitespace
/// normalized. A line whose first token starts with `#` is dropped entirely.
pub fn grabfile(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in content.lines() {
        // A full-line comment (first non-space char is '#') is skipped.
        if raw.trim_start().starts_with('#') {
            continue;
        }
        let tokens = strip_comment_tokens(raw);
        let joined = tokens.join(" ");
        if !joined.is_empty() {
            lines.push(joined);
        }
    }
    lines
}

/// Port of `portage.util.grabdict`: parses `key v1 v2 ...` lines into a map.
///
/// With `incremental`, repeated keys accumulate their values; otherwise the
/// last line wins. With `empty`, lines with only a key (no values) are kept;
/// otherwise they are skipped. Comment handling matches `grabfile`.
pub fn grabdict(content: &str, incremental: bool, empty: bool) -> BTreeMap<String, Vec<String>> {
    let mut dict: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for raw in content.lines() {
        if raw.trim_start().starts_with('#') {
            continue;
        }
        let tokens = strip_comment_tokens(raw);
        if tokens.is_empty() {
            continue;
        }
        if tokens.len() < 2 && !empty {
            continue;
        }
        let (key, values) = tokens.split_first().expect("non-empty checked above");
        let values: Vec<String> = values.to_vec();
        if incremental {
            dict.entry(key.clone()).or_default().extend(values);
        } else {
            dict.insert(key.clone(), values);
        }
    }
    dict
}

/// Port of `portage.util.stack_lists` (incremental): stacks lists with later
/// entries preferred, supporting `-value` removal and `-*` clear. Preserves
/// first-seen order of surviving tokens, matching upstream's dict semantics.
pub fn stack_lists(lists: &[Vec<String>], incremental: bool) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    for sub_list in lists {
        for token in sub_list {
            if incremental {
                if token == "-*" {
                    order.clear();
                } else if let Some(stripped) = token.strip_prefix('-') {
                    order.retain(|existing| existing != stripped);
                } else if !order.contains(token) {
                    order.push(token.clone());
                }
            } else if !order.contains(token) {
                order.push(token.clone());
            }
        }
    }
    order
}

/// Port of `portage.util.stack_dicts`: merges dicts of `key -> string`. A key
/// repeated in a later dict is appended (space-joined) when `incremental` or
/// the key is in `incrementals`; otherwise the later value overwrites.
/// A `None` entry aborts and returns `None` (mirrors `ignore_none == 0`).
pub fn stack_dicts(
    dicts: &[Option<BTreeMap<String, String>>],
    incremental: bool,
    incrementals: &[&str],
    ignore_none: bool,
) -> Option<BTreeMap<String, String>> {
    let mut final_dict: BTreeMap<String, String> = BTreeMap::new();
    for entry in dicts {
        let Some(mydict) = entry else {
            if ignore_none {
                continue;
            } else {
                return None;
            }
        };
        for (k, v) in mydict {
            let append = incremental || incrementals.contains(&k.as_str());
            if append && final_dict.contains_key(k) {
                let combined = format!("{} {}", final_dict[k], v);
                final_dict.insert(k.clone(), combined);
            } else {
                final_dict.insert(k.clone(), v.clone());
            }
        }
    }
    Some(final_dict)
}
