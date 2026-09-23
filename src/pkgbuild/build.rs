use super::{PkgBuild, Script};
use crate::args::Args;
#[cfg(feature = "progress")]
use crate::util::ProgressReader;
use anyhow::{Context as _, Result};
use std::hash::Hasher;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use ureq::ResponseExt as _;

#[derive(Default, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
enum StateStep {
    #[default]
    None,
    Downloaded,
    Prepared,
    Built,
    Packaged,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct State {
    step: StateStep,
    makedepends: Vec<String>,
    sources: Vec<String>,
    prepare_script: u64,
    build_script: u64,
    package_script: u64,
}

impl PkgBuild {
    pub fn build(&self, args: &Args) -> Result<()> {
        let pkg_dir =
            find_pkg_dir(&self.pkgname, &self.pkgver).context("Failed to find source dir")?;

        std::fs::create_dir_all(&pkg_dir).context("Failed to create package dir")?;
        std::fs::create_dir_all(&args.target).context("Failed to create destination dir")?;

        let mut state = self.fetch_state_or_default(&pkg_dir)?;
        let result = self.try_build(&pkg_dir, args, &mut state);

        let _ = self.save_state(&pkg_dir, &state);
        result
    }

    fn try_build(&self, pkg_dir: &Path, args: &Args, state: &mut State) -> Result<()> {
        let dst_dir = args.target.canonicalize().unwrap();
        let manifest_dir = args.manifest.parent().unwrap();

        self.check_makedepends()?;
        if state.step < StateStep::Downloaded {
            self.extract_sources(pkg_dir)?;
            state.step = StateStep::Downloaded;
        }
        if state.step < StateStep::Prepared {
            self.prepare.run_script(pkg_dir, &dst_dir, manifest_dir)?;
            state.step = StateStep::Prepared;
        }
        if state.step < StateStep::Built {
            self.build.run_script(pkg_dir, &dst_dir, manifest_dir)?;
            state.step = StateStep::Built;
        }
        if state.step < StateStep::Packaged {
            self.package.run_script(pkg_dir, &dst_dir, manifest_dir)?;
            state.step = StateStep::Packaged;
        }
        Ok(())
    }

    fn fetch_state_or_default(&self, pkg_dir: &Path) -> Result<State> {
        let state_path = pkg_dir.join("state");
        let file = match std::fs::File::open(&state_path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(err) => return Err(err.into()),
        };
        let mut buf = [0; 1024];
        let (mut state, _) = postcard::from_io::<State, _>((file, &mut buf))?;

        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(self.prepare.0.as_bytes());
        let prepare_hash = hasher.finish();

        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(self.build.0.as_bytes());
        let build_hash = hasher.finish();

        let mut hasher = std::hash::DefaultHasher::new();
        hasher.write(self.package.0.as_bytes());
        let package_hash = hasher.finish();

        if state.step >= StateStep::Built && state.makedepends != self.makedepends {
            state.step = StateStep::Downloaded;
            state.makedepends = self.makedepends.clone();
        }
        if state.step >= StateStep::Packaged && state.package_script != package_hash {
            state.step = StateStep::Built;
            state.package_script = package_hash;
        }
        if state.step >= StateStep::Built && state.build_script != build_hash {
            state.step = StateStep::Prepared;
            state.build_script = build_hash;
        }
        if state.step >= StateStep::Prepared && state.prepare_script != prepare_hash {
            state.step = StateStep::Downloaded;
            state.prepare_script = prepare_hash;
        }
        if state.step >= StateStep::Downloaded && state.sources != self.source {
            state.step = StateStep::None;
            state.sources = self.source.clone();
        }

        Ok(state)
    }

    fn save_state(&self, pkg_dir: &Path, state: &State) -> Result<()> {
        let state_path = pkg_dir.join("state");
        let file = std::fs::File::create(&state_path)?;
        postcard::to_io(state, file)?;
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

    fn extract_sources(&self, pkg_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(pkg_dir).context("Failed to create source dir")?;
        for source in &self.source {
            let response = ureq::get(source)
                .call()
                .context("Failed to download source")?;
            #[cfg(feature = "progress")]
            let size: usize = response
                .headers()
                .get("Content-Length")
                .context("No Content-Length header")
                .and_then(|s| s.to_str().map_err(Into::into))
                .and_then(|s| s.parse().map_err(Into::into))
                .context("Failed to parse Content-Length header")?;
            let path = response.get_uri().path().to_owned();
            let mut path = Path::new(&path);
            let mut body = response.into_body();
            #[cfg(feature = "progress")]
            let (sender, receiver) = std::sync::mpsc::channel();
            #[cfg(feature = "progress")]
            let mut reader: Box<dyn Read> = Box::new(ProgressReader::new(body.as_reader(), sender));
            #[cfg(not(feature = "progress"))]
            let mut reader: Box<dyn Read> = Box::new(body.as_reader());
            #[cfg(feature = "progress")]
            let progress_task = std::thread::spawn(move || {
                let bar = indicatif::ProgressBar::new(size as u64);
                bar.set_style(
                    indicatif::ProgressStyle::with_template(
                        "{wide_bar} {bytes}/{total_bytes} ({eta})",
                    )
                    .unwrap(),
                );
                let mut progress = 0;
                while let Ok(n) = receiver.recv() {
                    progress += n;
                    bar.set_position(progress as u64);
                }
            });

            loop {
                #[cfg(feature = "bz2")]
                if path.extension() == Some("bz2".as_ref()) {
                    let decoder = bzip2::read::BzDecoder::new(reader);
                    reader = Box::new(decoder);
                    path = Path::new(path.file_stem().unwrap());
                    continue;
                }
                #[cfg(feature = "gz")]
                if path.extension() == Some("gz".as_ref()) {
                    let decoder = flate2::read::GzDecoder::new(reader);
                    reader = Box::new(decoder);
                    path = Path::new(path.file_stem().unwrap());
                    continue;
                }
                #[cfg(feature = "xz")]
                if path.extension() == Some("xz".as_ref()) {
                    let decoder = xz2::read::XzDecoder::new(reader);
                    reader = Box::new(decoder);
                    path = Path::new(path.file_stem().unwrap());
                    continue;
                }
                #[cfg(feature = "tar")]
                if path.extension() == Some("tar".as_ref()) {
                    let mut archive = tar::Archive::new(reader);
                    archive.unpack(pkg_dir).context("Failed to unpack source")?;
                    break;
                }
                anyhow::bail!("Unknown source extension");
            }
            #[cfg(feature = "progress")]
            let _ = progress_task.join();
        }
        Ok(())
    }
}

impl Script {
    fn run_script(&self, pkg_dir: &Path, dst_dir: &Path, manifest_dir: &Path) -> Result<()> {
        let mut child = std::process::Command::new("/bin/sh")
            .current_dir(manifest_dir)
            .env("srcdir", pkg_dir.join("sources"))
            .env("pkgdir", dst_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        let mut stdin = child.stdin.take().context("Failed to open stdin")?;
        std::thread::scope(|s| {
            s.spawn(|| {
                stdin.write_all(self.0.as_bytes())?;
                drop(stdin); // Dunno why it is not automatically dropped
                Ok::<_, std::io::Error>(())
            });
            child.wait()?;
            anyhow::Ok(())
        })?;
        Ok(())
    }
}

fn find_pkg_dir(name: &str, version: &str) -> Option<PathBuf> {
    dirs::cache_dir().or_else(dirs::home_dir).map(|mut dir| {
        dir.push(format!("{}/{}-{}", env!("CARGO_PKG_NAME"), name, version));
        dir
    })
}
