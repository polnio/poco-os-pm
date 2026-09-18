use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Args {
    #[clap(short, long)]
    pub target: PathBuf,
}

impl Args {
    pub fn parse() -> Self {
        clap::Parser::parse()
    }
}
