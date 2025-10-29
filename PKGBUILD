# Maintainer: James Sully <sullyj3@gmail.com>
# Contributor: James Sully <sullyj3@gmail.com>
_pkgname=sand-timer
pkgname=${_pkgname}-git
pkgver=v0.5.0
pkgrel=1
pkgdesc="Command line countdown timers that don't take up a terminal."
arch=('x86_64')
url="https://github.com/sullyj3/sand"
license=('MIT')
groups=()
depends=('systemd' 'libnotify')
makedepends=('git' 'rust' 'cargo')
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
    cargo build --release
}

package() {
    cd "$srcdir/${_pkgname}"
    install -Dm755 target/release/sand ${pkgdir}/usr/bin/sand

    install -Dm644 README.md "${pkgdir}/usr/share/doc/${_pkgname}/README.md"
    install -Dm644 LICENSE "${pkgdir}/usr/share/licenses/${_pkgname}/LICENSE"

    install -Dm644 resources/systemd/sand.socket "${pkgdir}/usr/lib/systemd/user/sand.socket"
    install -Dm644 resources/systemd/sand.service "${pkgdir}/usr/lib/systemd/user/sand.service"

    install -Dm644 resources/timer_sound.flac "${pkgdir}/usr/share/${_pkgname}/timer_sound.flac"
}
