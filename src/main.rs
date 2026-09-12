mod pkgbuild;

fn main() -> Result<(), pkgbuild::Error> {
    let file = std::fs::File::open("PKGBUILD")?;
    let buf = std::io::BufReader::new(file);
    let pkgbuild = pkgbuild::PkgBuild::parse(buf)?;
    dbg!(pkgbuild);
    Ok(())
}
