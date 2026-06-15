pub mod atom;
pub mod cli;
pub mod dep;
pub mod resolver;
pub mod version;

pub fn run<I, S>(args: I) -> Result<cli::EmergeRequest, cli::CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::EmergeRequest::parse(args)
}
