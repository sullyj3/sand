# Maintainer: James Sully <sullyj3@gmail.com>
# Contributor: James Sully <sullyj3@gmail.com>
_pkgname=sand-timer
pkgname=${_pkgname}-git
pkgver=v0.7.0
pkgrel=1
pkgdesc="Command line countdown timers that don't take up a terminal."
arch=('x86_64')
url="https://github.com/sullyj3/sand"
license=('MIT')
groups=()
depends=('alsa-lib' 'glibc' 'gcc-libs')
optdepends=('systemd: for running as a systemd unit (recommended)')
makedepends=('git' 'cargo-nightly')
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}::git+https://github.com/sullyj3/sand.git")
options=(!debug)
sha256sums=('SKIP')

prepare() {
    cd "${srcdir}/${_pkgname}"
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

pkgver() {
    cd "$srcdir/${_pkgname}"
    git describe --long | sed 's/\([^-]*-\)g/r\1/;s/-/./g'
}

build() {
    cd "$srcdir/${_pkgname}"
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

package() {
    cd "$srcdir/${_pkgname}"
    make DESTDIR="${pkgdir}" PREFIX="/usr" install
}
