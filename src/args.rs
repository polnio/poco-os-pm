use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Args {
    #[clap(short, long)]
    pub target: PathBuf,
    #[clap(short, long, default_value = "PKGBUILD")]
    pub manifest: PathBuf,
}

impl Args {
    pub fn parse() -> Self {
        let mut args: Self = clap::Parser::parse();
        if args.manifest.is_dir() {
            args.manifest.push("PKGBUILD");
        }
        args
    }
}
