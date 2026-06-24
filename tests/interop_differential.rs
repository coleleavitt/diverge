//! Python interop differential test.
//!
//! Runs `tests/interop/portage_oracle.py` against the *real* upstream Portage
//! sources in `research/portage/lib`, then cross-checks every emitted record
//! against diverge's Rust implementations. This converts the domain-layer
//! parity ports from hand-written expectations into a genuine differential
//! test against emerge's own behavior.
//!
//! The test skips cleanly (returns without failing) when the environment
//! cannot provide the oracle:
//!   * `python3` is not on `PATH`
//!   * the `research/portage` reference checkout is absent
//!   * the oracle exits 77 (portage failed to import)
//!
//! When the oracle does run, any divergence between upstream Portage and the
//! Rust port fails the test with a per-record summary.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use diverge::atom::{Atom, AtomParseOptions};
use diverge::config::{getconfig, varexpand};
use diverge::dep::{
    Dep,
    UseReduceOptions,
    check_required_use,
    dep_getcpv,
    dep_getrepo,
    dep_getslot,
    dep_getusedeps,
    get_operator,
    isjustname,
    paren_enclose,
    paren_reduce,
    use_reduce,
};
use diverge::matching::{Candidate, match_from_list};
use diverge::version::{cpv_cmp, vercmp};

const NUL: &str = "\u{0}";
const ERR: &str = "\u{1}";

#[test]
fn rust_domain_layer_matches_upstream_portage() {
    let Some(repo_root) = repo_root() else {
        eprintln!("interop: repo root not found; skipping");
        return;
    };
    let portage_lib = repo_root.join("research/portage/lib");
    if !portage_lib.join("portage/dep/__init__.py").exists() {
        eprintln!("interop: research/portage reference checkout absent; skipping");
        return;
    }
    let oracle = repo_root.join("tests/interop/portage_oracle.py");
    if !oracle.exists() {
        eprintln!("interop: oracle script missing; skipping");
        return;
    }

    let Some(python) = find_python() else {
        eprintln!("interop: python3 not found on PATH; skipping");
        return;
    };

    let output = Command::new(&python)
        .arg(&oracle)
        .env("PYTHONPATH", &portage_lib)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // Scrub coverage instrumentation from the child env: under
        // `cargo llvm-cov`, the spawned python would otherwise inherit
        // `LLVM_PROFILE_FILE` and clobber the parent's profraw output.
        .env_remove("LLVM_PROFILE_FILE")
        .output()
        .expect("failed to spawn python3 oracle");

    if output.status.code() == Some(77) {
        eprintln!(
            "interop: portage import failed inside oracle; skipping\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert!(
        output.status.success(),
        "oracle exited with {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("oracle output is valid UTF-8");
    let mut checked = 0usize;
    let mut divergences = Vec::new();

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        checked += 1;
        if let Err(message) = check_record(&fields) {
            divergences.push(message);
        }
    }

    assert!(checked > 0, "oracle produced no records");
    assert!(
        divergences.is_empty(),
        "{} of {} interop records diverged from upstream Portage:\n{}",
        divergences.len(),
        checked,
        divergences.join("\n")
    );
    eprintln!("interop: {checked} records matched upstream Portage");
}

fn check_record(fields: &[&str]) -> Result<(), String> {
    let kind = fields[0];
    match kind {
        "vercmp" => {
            let (left, right, expected) = (fields[1], fields[2], fields[3]);
            // ERR means upstream returned None (invalid version) — skip; our
            // parser is intentionally lenient and this is not a parity claim.
            if expected == ERR {
                return Ok(());
            }
            let got = sign(vercmp(left, right));
            same(kind, &[left, right], expected, &got.to_string())
        }
        "cpv_sort" => {
            let input: Vec<&str> = fields[1].split(' ').collect();
            let mut owned: Vec<String> = input.iter().map(|s| s.to_string()).collect();
            owned.sort_by(|a, b| cpv_cmp(a, b));
            let got = owned.join(" ");
            same(kind, &[fields[1]], fields[2], &got)
        }
        "get_operator" => {
            let got = opt(get_operator(fields[1]));
            same(kind, &[fields[1]], fields[2], &got)
        }
        "dep_getcpv" => {
            let got = opt(dep_getcpv(fields[1]));
            same(kind, &[fields[1]], fields[2], &got)
        }
        "dep_getslot" => {
            let got = opt(dep_getslot(fields[1]));
            same(kind, &[fields[1]], fields[2], &got)
        }
        "dep_getrepo" => {
            let got = opt(dep_getrepo(fields[1]));
            same(kind, &[fields[1]], fields[2], &got)
        }
        "isjustname" => {
            let got = if isjustname(fields[1]) {
                "true"
            } else {
                "false"
            };
            same(kind, &[fields[1]], fields[2], got)
        }
        "dep_getusedeps" => {
            let got = match dep_getusedeps(fields[1]) {
                Ok(flags) => flags.join(" "),
                Err(_) => ERR.to_string(),
            };
            same(kind, &[fields[1]], fields[2], &got)
        }
        "paren_reduce" => {
            let got = match paren_reduce(fields[1]) {
                Ok(reduced) => canon(&reduced),
                Err(_) => ERR.to_string(),
            };
            same(kind, &[fields[1]], fields[2], &got)
        }
        "match_from_list" => check_match_from_list(fields),
        "use_reduce" => check_use_reduce(fields),
        "check_required_use" => check_cru(fields),
        "varexpand" => check_varexpand(fields),
        "getconfig" => check_getconfig(fields),
        other => Err(format!("unknown record kind '{other}'")),
    }
}

fn check_match_from_list(fields: &[&str]) -> Result<(), String> {
    let atom_str = fields[1];
    let candidates: Vec<Candidate> = fields[2]
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(Candidate::new)
        .collect();
    let expected = fields[3];

    let atom = match Atom::parse_with_options(
        atom_str,
        AtomParseOptions {
            allow_wildcard: true,
            allow_repo: true,
        },
    ) {
        Ok(atom) => atom,
        Err(_) => {
            // Upstream also records ERR for an invalid atom.
            return same("match_from_list", &[atom_str, fields[2]], expected, ERR);
        }
    };
    let got = match_from_list(&atom, &candidates)
        .into_iter()
        .map(|c| c.cpv.clone())
        .collect::<Vec<_>>()
        .join(" ");
    same("match_from_list", &[atom_str, fields[2]], expected, &got)
}

fn check_getconfig(fields: &[&str]) -> Result<(), String> {
    let content = b64_decode_str(fields[1]);
    let expand = fields[2] == "1";
    let initial = decode_dict(fields[3]);
    let expected = fields[4];

    let got = match getconfig(&content, expand, &initial) {
        Ok(map) => encode_dict(&map),
        Err(_) => ERR.to_string(),
    };
    if got == expected {
        Ok(())
    } else {
        Err(format!(
            "  getconfig({content:?}, expand={expand}): upstream={} rust={}",
            show_dict(expected),
            show_dict(&got)
        ))
    }
}

/// Re-encodes a parsed map into the oracle's base64 `k=v\x1f...` form so it
/// compares byte-for-byte with the upstream-produced field.
fn encode_dict(map: &HashMap<String, String>) -> String {
    let mut entries: Vec<(&String, &String)> = map.iter().collect();
    entries.sort();
    let joined = entries
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    b64_encode(joined.as_bytes())
}

fn show_dict(field: &str) -> String {
    if field == ERR {
        "<ParseError>".to_string()
    } else {
        format!("{:?}", b64_decode_str(field))
    }
}

/// Minimal standard-base64 encoder (no external crate), matching the oracle.
fn b64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn check_varexpand(fields: &[&str]) -> Result<(), String> {
    let input = b64_decode_str(fields[1]);
    let dict = decode_dict(fields[2]);
    let expected = b64_decode_str(fields[3]);
    let got = varexpand(&input, &dict);
    if got == expected {
        Ok(())
    } else {
        Err(format!(
            "  varexpand({input:?}, {dict:?}): upstream={expected:?} rust={got:?}"
        ))
    }
}

/// Decodes the base64 `k=v\x1fk=v` dict serialization from the oracle.
fn decode_dict(field: &str) -> HashMap<String, String> {
    let decoded = b64_decode_str(field);
    let mut map = HashMap::new();
    if decoded.is_empty() {
        return map;
    }
    for entry in decoded.split('\u{1f}') {
        if let Some((k, v)) = entry.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn b64_decode_str(input: &str) -> String {
    String::from_utf8(b64_decode(input)).expect("oracle base64 decodes to UTF-8")
}

/// Minimal standard-base64 decoder (no external crate). The oracle only emits
/// canonical base64 with `=` padding, so this need not handle URL-safe input.
fn b64_decode(input: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

fn check_use_reduce(fields: &[&str]) -> Result<(), String> {
    let dep = fields[1];
    let uselist = split_words(fields[2]);
    let masklist = split_words(fields[3]);
    let excludeall = split_words(fields[4]);
    let subset_field = fields[5];
    let matchall = fields[6] == "1";
    let expected = fields[7];

    let uselist_ref: Vec<&str> = uselist.iter().map(String::as_str).collect();
    let masklist_ref: Vec<&str> = masklist.iter().map(String::as_str).collect();
    let excludeall_ref: Vec<&str> = excludeall.iter().map(String::as_str).collect();
    let subset_owned: Option<Vec<String>> = if subset_field == NUL {
        None
    } else {
        Some(split_words(subset_field))
    };
    let subset_ref: Option<Vec<&str>> = subset_owned
        .as_ref()
        .map(|s| s.iter().map(String::as_str).collect());

    let options = UseReduceOptions {
        uselist: &uselist_ref,
        masklist: &masklist_ref,
        excludeall: &excludeall_ref,
        subset: subset_ref.as_deref(),
        matchall,
        ..UseReduceOptions::default()
    };

    let got = match use_reduce(dep, &options) {
        Ok(reduced) => canon(&reduced),
        Err(_) => ERR.to_string(),
    };
    same("use_reduce", &[dep, fields[2], fields[5]], expected, &got)
}

fn check_cru(fields: &[&str]) -> Result<(), String> {
    let required_use = fields[1];
    let use_: Vec<String> = split_words(fields[2]);
    let iuse: Vec<String> = split_words(fields[3]);
    let eapi = if fields[4] == NUL {
        None
    } else {
        Some(fields[4])
    };
    let expected = fields[5];

    let use_ref: Vec<&str> = use_.iter().map(String::as_str).collect();
    let matcher = |flag: &str| iuse.iter().any(|f| f == flag);
    let got = match check_required_use(required_use, &use_ref, matcher, eapi) {
        Ok(true) => "true".to_string(),
        Ok(false) => "false".to_string(),
        Err(_) => ERR.to_string(),
    };
    same(
        "check_required_use",
        &[required_use, fields[2]],
        expected,
        &got,
    )
}

/// Canonical paren-enclosed form matching the Python oracle's `canon`.
fn canon(nodes: &[Dep]) -> String {
    format!("( {} )", paren_enclose(nodes))
}

fn split_words(field: &str) -> Vec<String> {
    field
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn opt(value: Option<String>) -> String {
    value.unwrap_or_else(|| NUL.to_string())
}

fn sign(ordering: Ordering) -> i32 {
    match ordering {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn same(kind: &str, args: &[&str], expected: &str, got: &str) -> Result<(), String> {
    if expected == got {
        Ok(())
    } else {
        Err(format!(
            "  {kind}({args:?}): upstream={} rust={}",
            show(expected),
            show(got)
        ))
    }
}

fn show(value: &str) -> String {
    match value {
        NUL => "<None>".to_string(),
        ERR => "<Err>".to_string(),
        other => format!("'{other}'"),
    }
}

fn find_python() -> Option<PathBuf> {
    for candidate in ["python3", "python"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

fn repo_root() -> Option<PathBuf> {
    let mut dir = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("tests/interop").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
