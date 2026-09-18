mod build;
mod parse;

#[derive(Debug)]
pub struct Script(String);

parse::macros::make_pkgbuild_struct! {
    #[derive(Debug)]
    pub struct PkgBuild {
        required: [pkgname, pkgver, pkgrel, url],
        optional: [epoch, pkgdesc],
        multi: [license, source, makedepends],
        script: [package, build, prepare],
    }
}
