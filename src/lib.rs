pub mod atom;
pub mod cli;
pub mod config;
pub mod dbapi;
pub mod dep;
pub mod depgraph;
pub mod executor;
pub mod manifest;
pub mod matching;
pub mod profile;
pub mod repository;
pub mod resolver;
pub mod sets;
pub mod update;
pub mod util;
pub mod version;
pub mod xpak;

pub fn run<I, S>(args: I) -> Result<cli::EmergeRequest, cli::CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::EmergeRequest::parse(args)
}
