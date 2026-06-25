//! Terminal color output, ported from Portage's `portage.output`.
//!
//! Reproduces the exact ANSI escape sequences emerge uses for its colorized
//! output. Portage builds its palette from `ansi_codes` (`"{30..37}m"` and the
//! bold `"{30..37};01m"` variants); the named colors this crate needs map to:
//!
//! - `bold`      -> `\x1b[01m`
//! - `green`     -> `\x1b[32;01m`  (the `0x55FF55` bright green)
//! - `turquoise` -> `\x1b[36;01m`  (the `0x55FFFF` bright cyan)
//! - `reset`     -> `\x1b[39;49;00m`
//!
//! Styling is explicit: each function takes a `colored` flag, so rendering is
//! pure (no global state) and deterministic under parallel tests. The CLI
//! decides the flag once via [`should_colorize`], which mirrors emerge:
//! `--color y|n` wins, else color is on when stdout is a TTY and `NOCOLOR` is
//! unset. When `colored` is false, every function returns the text unchanged —
//! so non-TTY/piped output is byte-for-byte the plain banner.
//!
//! Reference: `research/portage/lib/portage/output.py` (`codes`, `colorize`,
//! `bold`, `green`, `turquoise`, `nocolor`).

use std::io::IsTerminal;

const ESC_BOLD: &str = "\x1b[01m";
const ESC_GREEN: &str = "\x1b[32;01m";
const ESC_TURQUOISE: &str = "\x1b[36;01m";
const ESC_RESET: &str = "\x1b[39;49;00m";

/// The resolved `--color` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Decide from TTY + `NOCOLOR`.
    Auto,
    /// Always emit color codes.
    Always,
    /// Never emit color codes.
    Never,
}

/// Resolves whether to colorize output for `mode`. `Auto` falls back to
/// emerge's default: stdout is a TTY and `NOCOLOR` is unset.
pub fn should_colorize(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::env::var_os("NOCOLOR").is_none() && std::io::stdout().is_terminal(),
    }
}

fn wrap(colored: bool, code: &str, text: &str) -> String {
    if colored {
        format!("{code}{text}{ESC_RESET}")
    } else {
        text.to_string()
    }
}

/// Port of `output.bold`: bold styling (when `colored`).
pub fn bold(colored: bool, text: &str) -> String {
    wrap(colored, ESC_BOLD, text)
}

/// Port of `output.green` (the `GOOD`/bright-green style).
pub fn green(colored: bool, text: &str) -> String {
    wrap(colored, ESC_GREEN, text)
}

/// Port of `output.turquoise` (the bright-cyan style).
pub fn turquoise(colored: bool, text: &str) -> String {
    wrap(colored, ESC_TURQUOISE, text)
}
