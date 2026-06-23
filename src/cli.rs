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
    Search,
    Info,
    Depclean,
    Unmerge,
    Prune,
    Clean,
    Config,
    ListSets,
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
            Self::Merge | Self::Unmerge | Self::Prune | Self::Clean | Self::Config
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
    // y/n-valued options.
    pub ask: YesNo,
    pub quiet: YesNo,
    pub autounmask: YesNo,
    pub getbinpkg: YesNo,
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
            "--ask" => opt.ask = yes_no(value)?,
            "--quiet" => opt.quiet = yes_no(value)?,
            "--autounmask" => opt.autounmask = yes_no(value)?,
            "--getbinpkg" => opt.getbinpkg = yes_no(value)?,
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

/// Maps an action long option to its [`EmergeAction`].
fn action_for(name: &str) -> Option<EmergeAction> {
    Some(match name {
        "--sync" => EmergeAction::Sync,
        "--search" => EmergeAction::Search,
        "--info" => EmergeAction::Info,
        "--depclean" => EmergeAction::Depclean,
        "--unmerge" => EmergeAction::Unmerge,
        "--prune" => EmergeAction::Prune,
        "--clean" => EmergeAction::Clean,
        "--config" => EmergeAction::Config,
        "--list-sets" => EmergeAction::ListSets,
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

        for arg in args.into_iter().map(Into::into) {
            if let Some(long) = arg.strip_prefix("--") {
                let (name, value) = match long.split_once('=') {
                    Some((n, v)) => (format!("--{n}"), Some(v.to_string())),
                    None => (arg.clone(), None),
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
