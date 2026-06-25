//! emerge-compatible command-line request parsing.
//!
//! This grows the toy parser into the full emerge surface: the action set,
//! boolean options, the `y/n`-valued options (`--ask`, `--quiet`, ...), the
//! integer-valued options (`--jobs`, `--deep`), the short-option map and
//! bundled short flags (`-pv`), and `--opt=value` forms — mirroring
//! `research/portage/lib/_emerge/main.py` (`actions`, `options`,
//! `shortmapping`, `argument_options`).
//!
//! Parsing is split from execution: this module produces a typed
//! [`EmergeRequest`]; the actions layer (and resolver/executor) consume it.

use crate::atom::{Atom, DEPENDENCY_ATOM_OPTIONS};

/// The top-level emerge action. Exactly one action is in effect per request;
/// the default is [`EmergeAction::Merge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergeAction {
    Merge,
    Sync,
    Metadata,
    Search,
    Info,
    Depclean,
    Unmerge,
    Prune,
    Clean,
    RageClean,
    Config,
    ListSets,
    CheckNews,
    Regen,
    Version,
    Help,
    Moo,
}

impl EmergeAction {
    /// True when this action operates on a package list (and therefore
    /// validates/keeps atom targets).
    fn takes_targets(self) -> bool {
        matches!(
            self,
            Self::Merge
                | Self::Unmerge
                | Self::Prune
                | Self::Clean
                | Self::RageClean
                | Self::Config
        )
    }

    /// True when this action accepts free-form (non-atom) terms, like search.
    fn takes_free_terms(self) -> bool {
        matches!(self, Self::Search)
    }
}

/// A tri-state y/n option that may be unset, defaulting per emerge semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum YesNo {
    #[default]
    Unset,
    Yes,
    No,
}

impl YesNo {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "y" | "yes" | "True" | "true" => Some(Self::Yes),
            "n" | "no" | "False" | "false" => Some(Self::No),
            _ => None,
        }
    }

    pub fn is_yes(self) -> bool {
        matches!(self, Self::Yes)
    }
}

/// The full set of parsed emerge options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmergeOptions {
    // Boolean flags.
    pub noreplace: bool,
    pub update: bool,
    pub deep: bool,
    pub newuse: bool,
    pub changed_use: bool,
    pub oneshot: bool,
    pub onlydeps: bool,
    pub nodeps: bool,
    pub emptytree: bool,
    pub usepkg: bool,
    pub usepkgonly: bool,
    pub buildpkg: bool,
    pub buildpkgonly: bool,
    pub fetchonly: bool,
    pub pretend: bool,
    pub verbose: bool,
    pub columns: bool,
    pub tree: bool,
    pub resume: bool,
    pub skipfirst: bool,
    pub debug: bool,
    pub selective: bool,
    pub complete_graph: bool,
    pub keep_going: bool,
    pub newrepo: bool,
    pub noconfmem: bool,
    pub nospinner: bool,
    pub fetch_all_uri: bool,
    pub searchdesc: bool,
    // y/n-valued options.
    pub ask: YesNo,
    pub quiet: YesNo,
    pub quiet_build: YesNo,
    pub autounmask: YesNo,
    pub getbinpkg: YesNo,
    pub with_bdeps: YesNo,
    pub with_test_deps: YesNo,
    /// `--reinstall changed-use` (the only documented value).
    pub reinstall: Option<String>,
    /// `--color < y | n >`: force colored output on/off (default: auto).
    pub color: YesNo,
    // integer-valued options.
    pub jobs: Option<u32>,
    pub load_average: Option<u32>,
}

/// A fully parsed emerge invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergeRequest {
    pub action: EmergeAction,
    pub options: EmergeOptions,
    /// Validated atom targets (for actions that take atoms).
    pub targets: Vec<Atom>,
    /// The raw target strings as given on the command line.
    pub raw_targets: Vec<String>,
    /// Package set names referenced as `@name` targets (e.g. `@world`).
    pub sets: Vec<String>,
}

/// A structured CLI parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    MissingSearchTerm,
    UnexpectedTarget {
        action: EmergeAction,
        target: String,
    },
    UnknownOption(String),
    InvalidTarget {
        target: String,
        reason: String,
    },
    InvalidOptionValue {
        option: String,
        value: String,
    },
    MultipleActions {
        first: EmergeAction,
        second: EmergeAction,
    },
}

/// Resolves a short flag cluster character to its long option name.
fn short_to_long(flag: char) -> Option<&'static str> {
    Some(match flag {
        '1' => "--oneshot",
        'B' => "--buildpkgonly",
        'b' => "--buildpkg",
        'c' => "--depclean",
        'C' => "--unmerge",
        'd' => "--debug",
        'e' => "--emptytree",
        'f' => "--fetchonly",
        'F' => "--fetch-all-uri",
        'g' => "--usepkg",
        'G' => "--usepkgonly",
        'h' => "--help",
        'k' => "--usepkg",
        'K' => "--usepkgonly",
        'n' => "--noreplace",
        'N' => "--newuse",
        'o' => "--onlydeps",
        'O' => "--nodeps",
        'p' => "--pretend",
        'P' => "--prune",
        'q' => "--quiet",
        'r' => "--resume",
        's' => "--search",
        'S' => "--searchdesc",
        't' => "--tree",
        'u' => "--update",
        'U' => "--changed-use",
        'a' => "--ask",
        'v' => "--verbose",
        'V' => "--version",
        'D' => "--deep",
        'w' => "--columns",
        _ => return None,
    })
}

struct Parser {
    action: Option<EmergeAction>,
    options: EmergeOptions,
    raw_targets: Vec<String>,
    sets: Vec<String>,
}

impl Parser {
    fn set_action(&mut self, action: EmergeAction) -> Result<(), CliError> {
        if let Some(existing) = self.action
            && existing != action
        {
            return Err(CliError::MultipleActions {
                first: existing,
                second: action,
            });
        }
        self.action = Some(action);
        Ok(())
    }

    /// Applies one normalized long option (`name`) with an optional `=value`.
    fn apply_long(&mut self, name: &str, value: Option<&str>) -> Result<(), CliError> {
        // Action options first.
        if let Some(action) = action_for(name) {
            return self.set_action(action);
        }

        let yes_no = |value: Option<&str>| -> Result<YesNo, CliError> {
            match value {
                None => Ok(YesNo::Yes),
                Some(v) => YesNo::parse(v).ok_or_else(|| CliError::InvalidOptionValue {
                    option: name.to_string(),
                    value: v.to_string(),
                }),
            }
        };
        let integer = |value: Option<&str>| -> Result<Option<u32>, CliError> {
            match value {
                None => Ok(None),
                Some(v) => v
                    .parse::<u32>()
                    .map(Some)
                    .map_err(|_| CliError::InvalidOptionValue {
                        option: name.to_string(),
                        value: v.to_string(),
                    }),
            }
        };

        let opt = &mut self.options;
        match name {
            "--noreplace" => opt.noreplace = true,
            "--update" => opt.update = true,
            "--deep" => opt.deep = true,
            "--newuse" => opt.newuse = true,
            "--changed-use" => opt.changed_use = true,
            "--oneshot" => opt.oneshot = true,
            "--onlydeps" => opt.onlydeps = true,
            "--nodeps" => opt.nodeps = true,
            "--emptytree" => opt.emptytree = true,
            "--usepkg" => opt.usepkg = true,
            "--usepkgonly" => {
                opt.usepkgonly = true;
                opt.usepkg = true;
            }
            "--buildpkg" => opt.buildpkg = true,
            "--buildpkgonly" => opt.buildpkgonly = true,
            "--fetchonly" => opt.fetchonly = true,
            "--pretend" => opt.pretend = true,
            "--verbose" => opt.verbose = true,
            "--columns" | "--cols" => opt.columns = true,
            "--tree" => opt.tree = true,
            "--resume" => opt.resume = true,
            "--skipfirst" | "--skip-first" => opt.skipfirst = true,
            "--debug" => opt.debug = true,
            "--complete-graph" => opt.complete_graph = true,
            "--keep-going" => opt.keep_going = true,
            "--newrepo" => opt.newrepo = true,
            "--noconfmem" => opt.noconfmem = true,
            "--nospinner" => opt.nospinner = true,
            "--fetch-all-uri" => opt.fetch_all_uri = true,
            // --searchdesc implies the search action (emerge: searchdesc -> search).
            "--searchdesc" => {
                opt.searchdesc = true;
                self.set_action(EmergeAction::Search)?;
                return Ok(());
            }
            "--reinstall" => {
                opt.reinstall = Some(value.unwrap_or("changed-use").to_string());
            }
            "--ask" => opt.ask = yes_no(value)?,
            "--quiet" => opt.quiet = yes_no(value)?,
            "--quiet-build" => opt.quiet_build = yes_no(value)?,
            "--autounmask" => opt.autounmask = yes_no(value)?,
            "--getbinpkg" => opt.getbinpkg = yes_no(value)?,
            "--with-bdeps" => opt.with_bdeps = yes_no(value)?,
            "--with-test-deps" => opt.with_test_deps = yes_no(value)?,
            "--color" => opt.color = yes_no(value)?,
            "--jobs" => opt.jobs = integer(value)?,
            "--load-average" => opt.load_average = integer(value)?,
            _ => return Err(CliError::UnknownOption(name.to_string())),
        }
        Ok(())
    }

    /// Applies a bundled short-flag cluster like `-pv` or `-1`.
    fn apply_short_cluster(&mut self, cluster: &str) -> Result<(), CliError> {
        for flag in cluster.chars() {
            let long =
                short_to_long(flag).ok_or_else(|| CliError::UnknownOption(format!("-{flag}")))?;
            self.apply_long(long, None)?;
        }
        Ok(())
    }
}

/// Renders emerge's usage/help banner, colorized exactly like upstream's
/// `_emerge.help.emerge_help()`: `emerge:`/`Usage:`/`Options:`/`Actions:` in
/// bold, the `emerge` command and value placeholders in turquoise, and option
/// names in green. Color is applied via [`crate::color`], so on a non-TTY (or
/// `--color n`/`NOCOLOR`) the output is the plain banner byte-for-byte.
///
/// Reference: `research/portage/lib/_emerge/help.py`.
pub fn usage_banner(colored: bool) -> String {
    use crate::color::{bold, green, turquoise};
    let b = |t: &str| bold(colored, t);
    let g = |t: &str| green(colored, t);
    let t = |s: &str| turquoise(colored, s);
    let em = || t("emerge");
    let mut out = String::new();

    out.push_str(&format!(
        "{} command-line interface to the Portage system\n",
        b("emerge:")
    ));
    out.push_str(&format!("{}\n", b("Usage:")));
    out.push_str(&format!(
        "   {} [ {} ] [ {} ] [ {} | {} | {} | {} | {} ] [ ... ]\n",
        em(),
        g("options"),
        g("action"),
        t("ebuild"),
        t("tbz2"),
        t("file"),
        t("@set"),
        t("atom"),
    ));
    out.push_str(&format!(
        "   {} [ {} ] [ {} ] < {} | {} >\n",
        em(),
        g("options"),
        g("action"),
        t("@system"),
        t("@world"),
    ));
    out.push_str(&format!(
        "   {} < {} | {} | {} >\n",
        em(),
        t("--sync"),
        t("--metadata"),
        t("--info"),
    ));
    out.push_str(&format!(
        "   {} {} [ {} | {} | {} ]\n",
        em(),
        t("--resume"),
        g("--pretend"),
        g("--ask"),
        g("--skipfirst"),
    ));
    out.push_str(&format!("   {} {}\n", em(), t("--help")));
    out.push_str(&format!(
        "{} {}[{}]\n",
        b("Options:"),
        g("-"),
        g("abBcCdDefgGhjkKlnNoOpPqrsStuUvVwW"),
    ));
    out.push_str(&format!(
        "          [ {} < {} | {} >            ] [ {}    ]\n",
        g("--color"),
        t("y"),
        t("n"),
        g("--columns"),
    ));
    out.push_str(&format!(
        "          [ {}             ] [ {}       ]\n",
        g("--complete-graph"),
        g("--deep"),
    ));
    out.push_str(&format!(
        "          [ {} {} ] [ {} ] [ {} {}            ]\n",
        g("--jobs"),
        t("JOBS"),
        g("--keep-going"),
        g("--load-average"),
        t("LOAD"),
    ));
    out.push_str(&format!(
        "          [ {}   ] [ {}     ] [ {}  ] [ {}   ]\n",
        g("--newrepo"),
        g("--newuse"),
        g("--noconfmem"),
        g("--nospinner"),
    ));
    out.push_str(&format!(
        "          [ {}   ] [ {}   ] [ {} [ {} | {} ]        ]\n",
        g("--oneshot"),
        g("--onlydeps"),
        g("--quiet-build"),
        t("y"),
        t("n"),
    ));
    out.push_str(&format!(
        "          [ {}{}      ] [ {} < {} | {} >         ]\n",
        g("--reinstall "),
        t("changed-use"),
        g("--with-bdeps"),
        t("y"),
        t("n"),
    ));
    out.push_str(&format!(
        "{}  [ {} | {} | {} | {} | {}        ]\n",
        b("Actions:"),
        g("--depclean"),
        g("--list-sets"),
        g("--search"),
        g("--sync"),
        g("--version"),
    ));
    out.push('\n');
    out.push_str("   For more help consult the man page.\n");
    out
}

/// The kind of value an option's separate-argument form accepts. Used to decide
/// whether the *next* argument is the option's (optional) value or a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    /// A `y`/`n`-style word (`--color y`, `--ask y`).
    YesNo,
    /// A non-negative integer (`--jobs 8`).
    Integer,
}

/// The value kind an option accepts as a separate argument, or `None` when the
/// option never consumes a following argument.
fn value_kind(name: &str) -> Option<ValueKind> {
    match name {
        "--color" | "--ask" | "--quiet" | "--autounmask" | "--getbinpkg" => Some(ValueKind::YesNo),
        "--jobs" | "--load-average" => Some(ValueKind::Integer),
        _ => None,
    }
}

/// Pops the next argument as the option's value, but only when it plausibly is
/// one (a y/n word, or an integer). Anything else — an atom, `@set`, or another
/// option — is left in place, matching emerge's *optional*-value semantics
/// (e.g. `--ask --pretend` and `--jobs dev-libs/A` leave the value unset).
fn next_value<I: Iterator<Item = String>>(
    argv: &mut std::iter::Peekable<I>,
    kind: ValueKind,
) -> Option<String> {
    let plausible = match argv.peek() {
        Some(next) => match kind {
            ValueKind::YesNo => YesNo::parse(next).is_some(),
            ValueKind::Integer => next.parse::<u32>().is_ok(),
        },
        None => false,
    };
    if plausible { argv.next() } else { None }
}

/// Maps an action long option to its [`EmergeAction`].
fn action_for(name: &str) -> Option<EmergeAction> {
    Some(match name {
        "--sync" => EmergeAction::Sync,
        "--metadata" => EmergeAction::Metadata,
        "--search" => EmergeAction::Search,
        "--info" => EmergeAction::Info,
        "--depclean" => EmergeAction::Depclean,
        "--unmerge" => EmergeAction::Unmerge,
        "--prune" => EmergeAction::Prune,
        "--clean" => EmergeAction::Clean,
        "--rage-clean" => EmergeAction::RageClean,
        "--config" => EmergeAction::Config,
        "--list-sets" => EmergeAction::ListSets,
        "--check-news" => EmergeAction::CheckNews,
        "--regen" => EmergeAction::Regen,
        "--version" => EmergeAction::Version,
        "--help" => EmergeAction::Help,
        "--moo" => EmergeAction::Moo,
        _ => return None,
    })
}

impl EmergeRequest {
    /// Parses an emerge-style argument list (excluding argv[0]).
    pub fn parse<I, S>(args: I) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut parser = Parser {
            action: None,
            options: EmergeOptions::default(),
            raw_targets: Vec::new(),
            sets: Vec::new(),
        };

        let mut argv = args.into_iter().map(Into::into).peekable();
        while let Some(arg) = argv.next() {
            if let Some(long) = arg.strip_prefix("--") {
                let (name, value) = match long.split_once('=') {
                    Some((n, v)) => (format!("--{n}"), Some(v.to_string())),
                    None => (arg.clone(), None),
                };
                // A value-taking option with no `=value` may consume the next
                // argument (emerge accepts both `--color y` and `--color=y`).
                // The value is optional, so only a *plausible* next token is
                // consumed: a `y/n` word for the y/n options, a number for the
                // integer options. Anything else (an atom, `@set`, another
                // option) is left as a separate argument.
                let value = if value.is_none() {
                    match value_kind(&name) {
                        Some(kind) => next_value(&mut argv, kind),
                        None => None,
                    }
                } else {
                    value
                };
                parser.apply_long(&name, value.as_deref())?;
            } else if arg.starts_with('-') && arg.len() > 1 {
                parser.apply_short_cluster(&arg[1..])?;
            } else if let Some(set) = arg.strip_prefix('@') {
                parser.sets.push(set.to_string());
            } else {
                parser.raw_targets.push(arg);
            }
        }

        let action = parser.action.unwrap_or(EmergeAction::Merge);
        finish(action, parser.options, parser.raw_targets, parser.sets)
    }
}

fn finish(
    action: EmergeAction,
    options: EmergeOptions,
    raw_targets: Vec<String>,
    sets: Vec<String>,
) -> Result<EmergeRequest, CliError> {
    if action == EmergeAction::Search && raw_targets.is_empty() {
        return Err(CliError::MissingSearchTerm);
    }

    // Actions that take neither atoms nor free terms reject stray targets,
    // unless a package set was given (e.g. `--depclean @world`).
    if !action.takes_targets()
        && !action.takes_free_terms()
        && let Some(target) = raw_targets.first()
        && (sets.is_empty() || action == EmergeAction::Sync || action == EmergeAction::Info)
    {
        return Err(CliError::UnexpectedTarget {
            action,
            target: target.clone(),
        });
    }

    let targets = if action.takes_targets() {
        raw_targets
            .iter()
            .map(|target| {
                Atom::parse_with_options(target, DEPENDENCY_ATOM_OPTIONS).map_err(|err| {
                    CliError::InvalidTarget {
                        target: target.clone(),
                        reason: err.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    Ok(EmergeRequest {
        action,
        options,
        targets,
        raw_targets,
        sets,
    })
}
