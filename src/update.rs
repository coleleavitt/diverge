//! Global package-move updates, ported from Portage's `update.py`.
//!
//! Repositories ship `profiles/updates/*` files with `move a/b c/d` and
//! `slotmove a/b 0 1` directives that rename packages or change slots across
//! the tree. This module ports `parse_updates` (read the directive list) and
//! `update_dbentry` (rewrite dependency strings / installed cpvs), so the
//! resolver and installed-db views see post-rename atoms.
//!
//! Reference:
//! - `research/portage/lib/portage/update.py` (`parse_updates`, `update_dbentry`)
//! - `research/portage/lib/portage/tests/update/test_move_ent.py`,
//!   `test_move_slot_ent.py`, `test_update_dbentry.py`

use crate::atom::{Atom, AtomParseOptions};

const UPDATE_ATOM_OPTIONS: AtomParseOptions = AtomParseOptions {
    allow_wildcard: false,
    allow_repo: true,
};

/// A single parsed update directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCommand {
    /// `move old/cp new/cp`: rename a package.
    Move { from: String, to: String },
    /// `slotmove cp old new`: change a package's slot.
    SlotMove {
        cp: String,
        from_slot: String,
        to_slot: String,
    },
}

/// Error raised when parsing an updates file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateParseError(pub String);

impl std::fmt::Display for UpdateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpdateParseError {}

/// Port of `parse_updates`: parses the directive lines of an updates file.
/// Blank lines are skipped; malformed lines raise an error.
pub fn parse_updates(content: &str) -> Result<Vec<UpdateCommand>, UpdateParseError> {
    let mut commands = Vec::new();
    for line in content.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        match tokens.as_slice() {
            ["move", from, to] => {
                validate_cp(from)?;
                validate_cp(to)?;
                commands.push(UpdateCommand::Move {
                    from: (*from).to_string(),
                    to: (*to).to_string(),
                });
            }
            ["slotmove", cp, from_slot, to_slot] => {
                validate_cp(cp)?;
                commands.push(UpdateCommand::SlotMove {
                    cp: (*cp).to_string(),
                    from_slot: (*from_slot).to_string(),
                    to_slot: (*to_slot).to_string(),
                });
            }
            _ => {
                return Err(UpdateParseError(format!(
                    "invalid update command: '{line}'"
                )));
            }
        }
    }
    Ok(commands)
}

fn validate_cp(cp: &str) -> Result<(), UpdateParseError> {
    // A cp must be `category/package` with no version/operator.
    match Atom::parse_with_options(cp, UPDATE_ATOM_OPTIONS) {
        Ok(atom) if atom.operator.is_none() && atom.version.is_none() => Ok(()),
        _ => Err(UpdateParseError(format!("invalid package name: '{cp}'"))),
    }
}

/// Applies one update command to a dependency string, rewriting matching atom
/// tokens while preserving surrounding whitespace. Mirrors `update_dbentry`.
pub fn update_dbentry(command: &UpdateCommand, content: &str) -> String {
    match command {
        UpdateCommand::Move { from, to } => {
            rewrite_tokens(content, |token| move_token(token, from, to))
        }
        UpdateCommand::SlotMove {
            cp,
            from_slot,
            to_slot,
        } => rewrite_tokens(content, |token| {
            slotmove_token(token, cp, from_slot, to_slot)
        }),
    }
}

/// Applies a sequence of update commands in order.
pub fn update_dbentries(commands: &[UpdateCommand], content: &str) -> String {
    commands
        .iter()
        .fold(content.to_string(), |acc, cmd| update_dbentry(cmd, &acc))
}

/// Rewrites whitespace-separated tokens with `f`, preserving the exact original
/// separators (so `a   b` stays `a   b`).
fn rewrite_tokens(content: &str, f: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut chars = content.char_indices().peekable();
    let mut token_start = None;

    let flush = |out: &mut String, token: &str| match f(token) {
        Some(replacement) => out.push_str(&replacement),
        None => out.push_str(token),
    };

    while let Some((idx, ch)) = chars.next() {
        if ch.is_whitespace() {
            if let Some(start) = token_start.take() {
                flush(&mut out, &content[start..idx]);
            }
            out.push(ch);
        } else if token_start.is_none() {
            token_start = Some(idx);
        }
        if chars.peek().is_none()
            && let Some(start) = token_start.take()
        {
            flush(&mut out, &content[start..]);
        }
    }
    out
}

/// Rewrites a single dependency token under a `move` directive: if its cp
/// equals `from`, replace it with `to` (preserving operator/version/slot/use).
fn move_token(token: &str, from: &str, to: &str) -> Option<String> {
    let atom = Atom::parse_with_options(token, UPDATE_ATOM_OPTIONS).ok()?;
    if atom.cp() != from {
        return None;
    }
    // Replace only the first occurrence of the cp, like upstream's replace(.,1).
    Some(token.replacen(from, to, 1))
}

/// Rewrites a single token under a `slotmove`: if its cp equals `cp` and it
/// carries the old slot (or no slot), update the slot to `to_slot`.
fn slotmove_token(token: &str, cp: &str, from_slot: &str, to_slot: &str) -> Option<String> {
    let atom = Atom::parse_with_options(token, UPDATE_ATOM_OPTIONS).ok()?;
    if atom.cp() != cp || atom.version.is_some() {
        return None;
    }
    // Only rewrite when the token explicitly references the old slot.
    if atom.slot() == Some(from_slot) {
        Some(token.replacen(&format!(":{from_slot}"), &format!(":{to_slot}"), 1))
    } else {
        None
    }
}
