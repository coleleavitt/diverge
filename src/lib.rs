pub mod atom;
pub mod cli;
pub mod config;
pub mod dep;
pub mod matching;
pub mod profile;
pub mod resolver;
pub mod util;
pub mod version;

pub fn run<I, S>(args: I) -> Result<cli::EmergeRequest, cli::CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::EmergeRequest::parse(args)
}
