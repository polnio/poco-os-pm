use super::{PkgBuild, Script};
use crate::args::Args;
use crate::util::ProgressReader;
use anyhow::{Context as _, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use ureq::ResponseExt as _;

impl PkgBuild {
    pub fn build(&self, args: &Args) -> Result<()> {
        std::fs::create_dir_all(&args.target).context("Failed to create destination dir")?;
        let dst_dir = args.target.canonicalize().unwrap();
        self.check_makedepends()?;
        let src_dir = self.extract_sources()?;
        Self::run_script(&self.prepare, &src_dir, &dst_dir)?;
        Self::run_script(&self.build, &src_dir, &dst_dir)?;
        Self::run_script(&self.package, &src_dir, &dst_dir)?;
        Ok(())
    }

    fn check_makedepends(&self) -> Result<()> {
        let path_env =
            std::env::var_os("PATH").context("Environment variable `PATH` does not exist")?;
        let paths = std::env::split_paths(&path_env).collect::<Vec<_>>();
        let missing = self
            .makedepends
            .iter()
            .filter(|dep| !paths.iter().any(|path| path.join(dep).exists()))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let mut error = String::from("Missing dependencies: ");
            for (i, dep) in missing.iter().enumerate() {
                if i != 0 {
                    error.push_str(", ");
                }
                error.push_str(dep);
            }
            anyhow::bail!(error);
        }
        Ok(())
    }

    fn run_script(script: &Script, src_dir: &Path, dst_dir: &Path) -> Result<()> {
        let mut child = std::process::Command::new("/bin/sh")
            .env("srcdir", src_dir)
            .env("pkgdir", dst_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        let mut stdin = child.stdin.take().context("Failed to open stdin")?;
        std::thread::scope(|s| {
            s.spawn(|| {
                stdin.write_all(script.0.as_bytes())?;
                drop(stdin); // Dunno why it is not automatically dropped
                Ok::<_, std::io::Error>(())
            });
            child.wait()?;
            anyhow::Ok(())
        })?;
        Ok(())
    }

    fn extract_sources(&self) -> Result<PathBuf> {
        let src_dir =
            find_src_dir(&self.pkgname, &self.pkgver).context("Failed to find source dir")?;
        if src_dir.exists() {
            return Ok(src_dir);
        }
        std::fs::create_dir_all(&src_dir).context("Failed to create source dir")?;
        for source in &self.source {
            let response = ureq::get(source)
                .call()
                .context("Failed to download source")?;
            let path = response.get_uri().path().to_owned();
            let mut path = Path::new(&path);
            let mut body = response.into_body();
            let mut reader: Box<dyn Read> = Box::new(body.as_reader());
            loop {
                if path.extension() == Some("gz".as_ref()) {
                    let decoder = flate2::read::GzDecoder::new(reader);
                    reader = Box::new(decoder);
                    path = Path::new(path.file_stem().unwrap());
                    continue;
                } else if path.extension() == Some("xz".as_ref()) {
                    let decoder = xz2::read::XzDecoder::new(reader);
                    reader = Box::new(decoder);
                    path = Path::new(path.file_stem().unwrap());
                    continue;
                } else if path.extension() == Some("tar".as_ref()) {
                    let mut archive = tar::Archive::new(reader);
                    archive
                        .unpack(&src_dir)
                        .context("Failed to unpack source")?;
                    break;
                }
                anyhow::bail!("Unknown source extension");
            }
        }
        Ok(src_dir)
    }
}

fn find_src_dir(name: &str, version: &str) -> Option<PathBuf> {
    dirs::cache_dir().or_else(dirs::home_dir).map(|mut dir| {
        dir.push(format!("{}/{}-{}", env!("CARGO_PKG_NAME"), name, version));
        dir
    })
}
