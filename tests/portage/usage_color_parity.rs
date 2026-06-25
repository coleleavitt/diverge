//! Parity tests for the colorized usage/help banner.
//!
//! Reference:
//! - `research/portage/lib/_emerge/help.py` (`emerge_help` layout)
//! - `research/portage/lib/portage/output.py` (bold/green/turquoise escapes)
//!
//! The plain (color-disabled) banner is asserted byte-for-byte against the
//! exact text emerge prints; the colored form is asserted to use emerge's exact
//! ANSI escape sequences and to strip back to the plain banner.

use diverge::cli::{EmergeAction, EmergeRequest, YesNo, usage_banner};

/// The exact plain banner emerge prints (captured from `emerge` with color
/// disabled). Must match byte-for-byte.
const EMERGE_PLAIN: &str = "\
emerge: command-line interface to the Portage system
Usage:
   emerge [ options ] [ action ] [ ebuild | tbz2 | file | @set | atom ] [ ... ]
   emerge [ options ] [ action ] < @system | @world >
   emerge < --sync | --metadata | --info >
   emerge --resume [ --pretend | --ask | --skipfirst ]
   emerge --help
Options: -[abBcCdDefgGhjkKlnNoOpPqrsStuUvVwW]
          [ --color < y | n >            ] [ --columns    ]
          [ --complete-graph             ] [ --deep       ]
          [ --jobs JOBS ] [ --keep-going ] [ --load-average LOAD            ]
          [ --newrepo   ] [ --newuse     ] [ --noconfmem  ] [ --nospinner   ]
          [ --oneshot   ] [ --onlydeps   ] [ --quiet-build [ y | n ]        ]
          [ --reinstall changed-use      ] [ --with-bdeps < y | n >         ]
Actions:  [ --depclean | --list-sets | --search | --sync | --version        ]

   For more help consult the man page.
";

/// Removes ANSI SGR escape sequences (`\x1b[...m`).
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip up to and including the terminating 'm'.
            for e in chars.by_ref() {
                if e == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn plain_banner_is_byte_identical_to_emerge() {
    assert_eq!(usage_banner(false), EMERGE_PLAIN);
}

#[test]
fn colored_banner_uses_emerge_escape_sequences() {
    let banner = usage_banner(true);
    // emerge's exact escapes from output.py.
    assert!(
        banner.contains("\x1b[01memerge:\x1b[39;49;00m"),
        "bold emerge:"
    );
    assert!(
        banner.contains("\x1b[36;01memerge\x1b[39;49;00m"),
        "turquoise emerge"
    );
    assert!(
        banner.contains("\x1b[32;01moptions\x1b[39;49;00m"),
        "green options"
    );
    assert!(
        banner.contains("\x1b[32;01m--depclean\x1b[39;49;00m"),
        "green action"
    );
    // Stripping the color yields the plain banner byte-for-byte.
    assert_eq!(strip_ansi(&banner), EMERGE_PLAIN);
}

#[test]
fn color_option_parses_separate_and_equals_forms() {
    // `--color y` (separate arg) and `--color=n` both set the option.
    let req = EmergeRequest::parse(["--color", "y", "dev-libs/A"]).unwrap();
    assert_eq!(req.options.color, YesNo::Yes);
    assert_eq!(req.targets.len(), 1, "value `y` consumed, not a target");

    let req = EmergeRequest::parse(["--color=n", "dev-libs/A"]).unwrap();
    assert_eq!(req.options.color, YesNo::No);

    // `--jobs N` separate form too.
    let req = EmergeRequest::parse(["--jobs", "8", "dev-libs/A"]).unwrap();
    assert_eq!(req.options.jobs, Some(8));
}

#[test]
fn help_action_is_recognized() {
    assert_eq!(
        EmergeRequest::parse(["--help"]).unwrap().action,
        EmergeAction::Help
    );
    assert_eq!(
        EmergeRequest::parse(["-h"]).unwrap().action,
        EmergeAction::Help
    );
}
