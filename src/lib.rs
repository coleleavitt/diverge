pub mod atom;
pub mod cli;
pub mod config;
pub mod dbapi;
pub mod dep;
pub mod depgraph;
pub mod executor;
pub mod gpkg;
pub mod manifest;
pub mod matching;
pub mod news;
pub mod profile;
pub mod repository;
pub mod resolver;
pub mod session;
pub mod sets;
pub mod sync;
pub mod update;
pub mod util;
pub mod vardb;
pub mod version;
pub mod xpak;

use std::path::PathBuf;

/// Top-level dispatch error.
#[derive(Debug)]
pub enum RunError {
    Cli(cli::CliError),
    Session(session::SessionError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli(e) => write!(f, "{e:?}"),
            Self::Session(e) => write!(f, "{e}"),
        }
    }
}

/// Parses an emerge-style argument list into a request (no execution).
pub fn parse<I, S>(args: I) -> Result<cli::EmergeRequest, cli::CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::EmergeRequest::parse(args)
}

/// Parses and runs an emerge request end-to-end against a config root.
///
/// The roots default to the host `/` but honor `PORTAGE_CONFIGROOT` and `ROOT`
/// from the environment so tests (and prefix installs) can point at an isolated
/// tree. Currently dispatches the merge action's `--pretend` plan; mutating
/// actions are gated until the executor wiring is complete.
pub fn run<I, S>(args: I) -> Result<(), RunError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let request = cli::EmergeRequest::parse(args).map_err(RunError::Cli)?;

    let config_root =
        std::env::var_os("PORTAGE_CONFIGROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let eroot = std::env::var_os("ROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from);

    let session = session::Session::load(&config_root, &eroot).map_err(RunError::Session)?;
    print!("{}", session.dispatch(&request));
    Ok(())
}
