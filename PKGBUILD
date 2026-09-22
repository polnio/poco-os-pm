pkgname=poco-os-pm
pkgver=1.0.0
pkgrel=1
epoch=
pkgdesc="A simple package manager for the Poco OS"
url="https://github.com/polnio/poco-os-pm"
license=('MIT')
makedepends=(cargo rustc)
source=(
    "https://github.com/polnio/poco-os-pm/archive/refs/tags/$pkgver.tar.gz"
)

prepare() {
  cd "$srcdir/$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target host-tuple
}

build() {
  cd "$srcdir/$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  echo "Installing $pkgname at $pkgdir"
  mkdir -p "$pkgdir/usr/bin/"
  install -Dm755 -t "$pkgdir/usr/bin/" "target/release/$pkgname"
}
