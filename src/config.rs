//! Configuration-file primitives ported from `portage.util`.
//!
//! This is the foundation of the config/profile layer: emerge reads
//! `make.globals`, `make.conf`, profile files, and `/etc/env.d` with a
//! shell-variable expander and a shell-like `KEY=value` parser. This module
//! starts with [`varexpand`], a faithful port of `portage.util.varexpand`
//! (`research/portage/lib/portage/util/__init__.py`).

use std::collections::HashMap;
use std::fmt;

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

// ---------------------------------------------------------------------------
// getconfig (portage.util.getconfig) — KEY=value shell config parser
// ---------------------------------------------------------------------------

/// Error returned when a config file cannot be parsed, mirroring upstream's
/// `ParseError` observable behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

/// Word characters upstream assigns to the `getconfig` shlex lexer:
/// `string.digits + string.ascii_letters + r"~!@#$%*_\:;?,./-+{}"`.
fn is_shlex_wordchar(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '~' | '!'
                | '@'
                | '#'
                | '$'
                | '%'
                | '*'
                | '_'
                | '\\'
                | ':'
                | ';'
                | '?'
                | ','
                | '.'
                | '/'
                | '-'
                | '+'
                | '{'
                | '}'
        )
}

/// A faithful subset of Python's POSIX `shlex` as configured by
/// `getconfig`: whitespace `" \t\r\n"`, comment char `#`, escape `\`,
/// escaped-quote set `"`, quote set `"'`, and the wordchars above.
///
/// This is sufficient for `make.conf`/`make.globals`/profile parsing; the
/// `source` directive (allow_sourcing) is handled by `getconfig` at a higher
/// level and is not part of the token stream here.
struct ShlexLexer {
    chars: Vec<char>,
    pos: usize,
}

impl ShlexLexer {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn is_whitespace(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n')
    }

    /// Returns the next token, or `None` at end of input. Mirrors
    /// `shlex.get_token()` for the configured lexer. Returns `Err` on an
    /// unterminated quote (upstream raises `ValueError` -> `ParseError`).
    fn next_token(&mut self) -> Result<Option<String>, ParseError> {
        // Iterative (not recursive) so a long run of separators cannot overflow
        // the stack — every iteration makes forward progress on `self.pos`.
        loop {
            // Skip whitespace and full-line/inline comments.
            loop {
                while self.pos < self.chars.len() && Self::is_whitespace(self.chars[self.pos]) {
                    self.pos += 1;
                }
                if self.pos < self.chars.len() && self.chars[self.pos] == '#' {
                    while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                        self.pos += 1;
                    }
                    continue;
                }
                break;
            }

            if self.pos >= self.chars.len() {
                return Ok(None);
            }

            // The `=` separator is its own token (it is not a wordchar).
            if self.chars[self.pos] == '=' {
                self.pos += 1;
                return Ok(Some("=".to_string()));
            }

            let mut token = String::new();
            let mut produced = false;

            while self.pos < self.chars.len() {
                let c = self.chars[self.pos];
                if c == '\'' || c == '"' {
                    produced = true;
                    self.pos += 1;
                    self.read_quote(c, &mut token)?;
                } else if c == '\\' {
                    // POSIX shlex escape: backslash removes itself and the next
                    // character is taken literally — including a newline, which
                    // becomes a literal newline in the token (it is NOT dropped).
                    produced = true;
                    self.pos += 1;
                    if let Some(next) = self.current() {
                        token.push(next);
                        self.pos += 1;
                    }
                } else if c == '#' {
                    // A comment terminates the current token.
                    break;
                } else if is_shlex_wordchar(c) {
                    produced = true;
                    token.push(c);
                    self.pos += 1;
                } else {
                    // A non-word separator (e.g. `(`, `)`, `&`, `|`, `<`, `>`):
                    // end of this token.
                    break;
                }
            }

            if produced {
                return Ok(Some(token));
            }

            // Landed on a single non-word separator char that the inner loop
            // did not consume. Consume it (guaranteeing progress) and continue
            // scanning for the next real token — mirroring the previous
            // recurse-and-skip intent without unbounded recursion.
            self.pos += 1;
        }
    }

    fn read_quote(&mut self, quote: char, token: &mut String) -> Result<(), ParseError> {
        // `\` only escapes inside double quotes (escapedquotes = '"').
        let allow_escape = quote == '"';
        while let Some(c) = self.current() {
            if c == quote {
                self.pos += 1;
                return Ok(());
            }
            if allow_escape && c == '\\' {
                if let Some(next) = self.peek(1)
                    && (next == '"' || next == '\\')
                {
                    token.push(next);
                    self.pos += 2;
                    continue;
                }
                token.push(c);
                self.pos += 1;
                continue;
            }
            token.push(c);
            self.pos += 1;
        }
        Err(ParseError(format!("No closing quotation for {quote}")))
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }
}

/// A variable name is invalid when it starts with a digit or contains a
/// non-word character. Mirrors upstream `_invalid_var_name_re = ^\d|\W`.
fn invalid_var_name(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        None => true,
        Some(first) if first.is_ascii_digit() => true,
        Some(first) if !(first.is_ascii_alphanumeric() || first == '_') => true,
        _ => !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
    }
}

/// Port of `portage.util.getconfig` (non-recursive, no sourcing) operating on
/// already-read file `content`.
///
/// Parses `KEY=value` assignments (with an optional `export` prefix) the way
/// bash would when sourcing the file, returning the resulting variable map.
/// When `expand` is true, values are passed through [`varexpand`] using an
/// expand map seeded with `initial` and accumulated from earlier assignments
/// (so `B=$A` sees the value assigned to `A` above it).
///
/// Returns [`ParseError`] for an unexpected EOF, a token that should be `=`
/// but is not, or an unterminated quote — matching upstream's non-tolerant
/// behavior. An invalid variable name is skipped (its value is consumed),
/// also matching upstream.
pub fn getconfig(
    content: &str,
    expand: bool,
    initial: &HashMap<String, String>,
) -> Result<HashMap<String, String>, ParseError> {
    let mut owned = content.to_string();
    if !owned.is_empty() && !owned.ends_with('\n') {
        owned.push('\n');
    }

    let mut lexer = ShlexLexer::new(&owned);
    let mut mykeys: HashMap<String, String> = HashMap::new();
    let mut expand_map = initial.clone();

    loop {
        let mut key = match lexer.next_token()? {
            None => break,
            Some(token) => token,
        };
        if key == "export" {
            key = match lexer.next_token()? {
                None => break,
                Some(token) => token,
            };
        }

        let equ = lexer
            .next_token()?
            .ok_or_else(|| ParseError("Unexpected EOF".to_string()))?;
        if equ != "=" {
            return Err(ParseError(format!("Invalid token '{equ}' (not '=')")));
        }

        let val = lexer.next_token()?.ok_or_else(|| {
            ParseError(format!("Unexpected end of config file: variable '{key}'"))
        })?;

        if invalid_var_name(&key) {
            // Non-tolerant upstream raises here (value already read).
            return Err(ParseError(format!("Invalid variable name '{key}'")));
        }

        if expand {
            let expanded = varexpand(&val, &expand_map);
            expand_map.insert(key.clone(), expanded.clone());
            mykeys.insert(key, expanded);
        } else {
            mykeys.insert(key, val);
        }
    }

    Ok(mykeys)
}
