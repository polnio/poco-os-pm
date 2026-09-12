pkgname=poco-os-pm
pkgver=0.1.0
pkgrel=1
epoch=
pkgdesc="A simple package manager for the Poco OS"
url="https://github.com/polnio/poco-os-pm"
license=('MIT')
makedepends=(cargo rustc)
source=(
    "https://github.com/polnio/poco-os-pm/archive/refs/tags/v$pkgver.tar.gz"
)

prepare() {
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target host-tuple
}

build() {
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release
}

package() {
  install -Dm755 -t "$pkgdir/usr/bin/" "target/release/$pkgname"
}
