mod args;
mod pkgbuild;

use anyhow::{Context as _, Result};

fn run() -> Result<()> {
    let args = args::Args::parse();
    let file = std::fs::File::open("PKGBUILD").context("Failed to open PKGBUILD")?;
    let buf = std::io::BufReader::new(file);
    let pkgbuild = pkgbuild::PkgBuild::parse(buf).context("Failed to parse PKGBUILD")?;
    pkgbuild.build(&args)?;
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}
