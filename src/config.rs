//! Configuration-file primitives ported from `portage.util`.
//!
//! This is the foundation of the config/profile layer: emerge reads
//! `make.globals`, `make.conf`, profile files, and `/etc/env.d` with a
//! shell-variable expander and a shell-like `KEY=value` parser. This module
//! starts with [`varexpand`], a faithful port of `portage.util.varexpand`
//! (`research/portage/lib/portage/util/__init__.py`).

use std::collections::HashMap;

/// Word characters allowed in a `$VAR` / `${VAR}` name: `[A-Za-z0-9_]`.
/// Mirrors upstream `_varexpand_word_chars`.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Port of `portage.util.varexpand`.
///
/// Expands `$VAR` and `${VAR}` references in `mystring` using `mydict`,
/// preserving quote characters (shlex performs quote removal upstream) and
/// reproducing bash-in-a-sourced-file backslash semantics: `\\` and `\$` are
/// unescaped, an escaped newline is dropped, and any other `\x` is preserved
/// verbatim. A reference to an unset variable expands to nothing.
///
/// On a malformed substitution (`${` with no closing `}`, or an empty name)
/// upstream writes a diagnostic and returns the empty string; this port
/// returns the empty string to match the observable value.
///
/// The backslash handling intentionally reproduces upstream's documented
/// bug-for-bug behavior (the `\\` look-ahead that consumes a following quote
/// or `$`).
pub fn varexpand(mystring: &str, mydict: &HashMap<String, String>) -> String {
    let chars: Vec<char> = mystring.chars().collect();
    let length = chars.len();
    let mut insing = false;
    let mut indoub = false;
    let mut pos = 0usize;
    let mut out = String::new();

    while pos < length {
        let current = chars[pos];

        if current == '\'' {
            // Quote removal is handled by shlex upstream; keep the quote.
            out.push('\'');
            if !indoub {
                insing = !insing;
            }
            pos += 1;
            continue;
        } else if current == '"' {
            out.push('"');
            if !insing {
                indoub = !indoub;
            }
            pos += 1;
            continue;
        }

        if insing {
            out.push(current);
            pos += 1;
            continue;
        }

        match current {
            '\n' => {
                // Convert newlines to spaces.
                out.push(' ');
                pos += 1;
            }
            '\\' => {
                if pos + 1 >= length {
                    out.push(current);
                    break;
                }
                let escaped = chars[pos + 1];
                pos += 2;
                match escaped {
                    '$' => out.push(escaped),
                    '\\' => {
                        out.push(escaped);
                        // Bug-for-bug compatible with upstream: a following
                        // quote or '$' is also consumed here.
                        if pos < length && matches!(chars[pos], '\'' | '"' | '$') {
                            out.push(chars[pos]);
                            pos += 1;
                        }
                    }
                    '\n' => { /* escaped newline is dropped */ }
                    other => {
                        // Upstream appends mystring[pos-2:pos]: backslash + char.
                        out.push('\\');
                        out.push(other);
                    }
                }
            }
            '$' => match expand_dollar(&chars, pos, mydict) {
                Expansion::Done(text, next) => {
                    out.push_str(&text);
                    pos = next;
                }
                Expansion::BadSubstitution => return String::new(),
            },
            other => {
                out.push(other);
                pos += 1;
            }
        }
    }

    out
}

/// Outcome of expanding a `$`-reference starting at the `$`.
enum Expansion {
    /// Text to append, and the position to continue from.
    Done(String, usize),
    /// Malformed `${...}` / empty name: `varexpand` returns "" overall.
    BadSubstitution,
}

/// Expands a single `$VAR` / `${VAR}` reference. `pos` indexes the `$`.
fn expand_dollar(chars: &[char], pos: usize, mydict: &HashMap<String, String>) -> Expansion {
    let length = chars.len();
    let mut pos = pos + 1;
    if pos == length {
        // Shells handle a trailing '$' like '\$'.
        return Expansion::Done("$".to_string(), pos);
    }

    let braced = chars[pos] == '{';
    if braced {
        pos += 1;
        if pos == length {
            return Expansion::BadSubstitution;
        }
    }

    let var_start = pos;
    loop {
        if !is_word_char(chars[pos]) {
            break;
        }
        if pos + 1 >= length {
            if braced {
                return Expansion::BadSubstitution;
            }
            pos += 1;
            break;
        }
        pos += 1;
    }

    let var_name: String = chars[var_start..pos].iter().collect();
    if braced {
        if chars[pos] != '}' {
            return Expansion::BadSubstitution;
        }
        pos += 1;
    }
    if var_name.is_empty() {
        return Expansion::BadSubstitution;
    }

    let text = mydict.get(&var_name).cloned().unwrap_or_default();
    Expansion::Done(text, pos)
}
