use crate::atom::{Atom, DEPENDENCY_ATOM_OPTIONS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmergeAction {
    Merge,
    Sync,
    Search,
    Info,
    Depclean,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmergeOptions {
    pub noreplace: bool,
    pub update: bool,
    pub usepkg: bool,
    pub usepkgonly: bool,
    pub autounmask: Option<bool>,
    pub pretend: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergeRequest {
    pub action: EmergeAction,
    pub options: EmergeOptions,
    pub targets: Vec<Atom>,
    pub raw_targets: Vec<String>,
}

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
}

impl EmergeRequest {
    pub fn parse<I, S>(args: I) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut action = EmergeAction::Merge;
        let mut options = EmergeOptions::default();
        let mut raw_targets = Vec::new();

        for arg in args.into_iter().map(Into::into) {
            match arg.as_str() {
                "--sync" => action = EmergeAction::Sync,
                "--search" | "-s" => action = EmergeAction::Search,
                "--info" => action = EmergeAction::Info,
                "--depclean" => action = EmergeAction::Depclean,
                "--noreplace" | "-n" => options.noreplace = true,
                "--update" | "-u" => options.update = true,
                "--usepkg" | "-k" => options.usepkg = true,
                "--usepkgonly" | "-K" => {
                    options.usepkgonly = true;
                    options.usepkg = true;
                }
                "--pretend" | "-p" => options.pretend = true,
                "--verbose" | "-v" => options.verbose = true,
                "--autounmask=n" | "--autounmask=no" => options.autounmask = Some(false),
                "--autounmask=y" | "--autounmask=yes" => options.autounmask = Some(true),
                _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
                _ => raw_targets.push(arg),
            }
        }

        if matches!(action, EmergeAction::Search) && raw_targets.is_empty() {
            return Err(CliError::MissingSearchTerm);
        }

        if matches!(
            action,
            EmergeAction::Sync | EmergeAction::Info | EmergeAction::Depclean
        ) && let Some(target) = raw_targets.first()
        {
            return Err(CliError::UnexpectedTarget {
                action,
                target: target.clone(),
            });
        }

        let targets = if matches!(action, EmergeAction::Merge) {
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

        Ok(Self {
            action,
            options,
            targets,
            raw_targets,
        })
    }
}
