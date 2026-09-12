pkgname=poco-os-pm
pkgver=0.1.0
pkgrel=1
epoch=
pkgdesc="A simple package manager for the Poco OS"
url="https://github.com/polnio/poco-os-pm"
license=('MIT')
source=(
    "https://github.com/polnio/poco-os-pm/archive/refs/tags/v$pkgver.tar.gz"
)

build() {
  cargo build --release
}

package() {}
