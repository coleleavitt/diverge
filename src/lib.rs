pub mod atom;
pub mod cli;
pub mod color;
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

/// Parses and runs an emerge request end-to-end against a config root, printing
/// to stdout and returning the process exit code (0 on success).
///
/// Matches emerge's top-level behavior:
/// - **no arguments** -> print the usage banner and exit `1`;
/// - **`--help`/`-h`** -> print the usage banner and exit `0`;
/// - otherwise load the config root and dispatch the action.
///
/// The roots default to the host `/` but honor `PORTAGE_CONFIGROOT` and `ROOT`
/// from the environment so tests (and prefix installs) can point at an isolated
/// tree. Currently dispatches the merge action's `--pretend` plan; mutating
/// actions are gated until the executor wiring is complete.
pub fn run<I, S>(args: I) -> Result<i32, RunError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let argv: Vec<String> = args.into_iter().map(Into::into).collect();

    // No arguments at all: emerge prints usage and exits 1 (color = auto).
    if argv.is_empty() {
        print!(
            "{}",
            cli::usage_banner(color::should_colorize(color::ColorMode::Auto))
        );
        return Ok(1);
    }

    let request = cli::EmergeRequest::parse(argv).map_err(RunError::Cli)?;

    // Resolve the `--color y|n` policy (else auto) into a single colorize flag.
    let color_mode = match request.options.color {
        cli::YesNo::Yes => color::ColorMode::Always,
        cli::YesNo::No => color::ColorMode::Never,
        cli::YesNo::Unset => color::ColorMode::Auto,
    };
    let colored = color::should_colorize(color_mode);

    // `--help`/`-h` prints usage and exits 0, without loading any config.
    if request.action == cli::EmergeAction::Help {
        print!("{}", cli::usage_banner(colored));
        return Ok(0);
    }

    let config_root =
        std::env::var_os("PORTAGE_CONFIGROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let eroot = std::env::var_os("ROOT").map_or_else(|| PathBuf::from("/"), PathBuf::from);

    let session = session::Session::load(&config_root, &eroot).map_err(RunError::Session)?;
    print!("{}", session.dispatch(&request));
    Ok(0)
}
