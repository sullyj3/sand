# Maintainer: James Sully <sullyj3@gmail.com>
# Contributor: James Sully <sullyj3@gmail.com>
_pkgname=sand-timer
pkgname=${_pkgname}-git
pkgver=v0.6.0.r4.c4c5052
pkgrel=1
pkgdesc="Command line countdown timers that don't take up a terminal."
arch=('x86_64')
url="https://github.com/sullyj3/sand"
license=('MIT')
groups=()
depends=('systemd' 'libnotify')
makedepends=('git' 'cargo-nightly' 'alsa-lib')
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}::git+https://github.com/sullyj3/sand.git")
options=(!debug)
sha256sums=('SKIP')

pkgver() {
    cd "$srcdir/${_pkgname}"
    printf "%s" "$(git describe --long | sed 's/\([^-]*-\)g/r\1/;s/-/./g')"
}

build() {
    cd "$srcdir/${_pkgname}"
    make
}

package() {
    cd "$srcdir/${_pkgname}"
    make DESTDIR="${pkgdir}" PREFIX="/usr" install
}
