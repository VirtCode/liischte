_name="liischte"
pkgname="$_name-git"

pkgver=r32.6c400e2
pkgrel=1

pkgdesc="a blazingly fast wayland bar for my personal use"
url="https://github.com/VirtCode/$_name"
license=(GPL-3.0)
arch=(x86_64)

depends=('libxkbcommon' 'libpipewire' 'wayland')
makedepends=('cargo-nightly' 'clang' 'modemmanager')

source=("$_name::git+$url.git")
md5sums=('SKIP')

pkgver() {
    cd $_name

    printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
    cd $_name

    export RUSTUP_TOOLCHAIN=nightly
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd $_name

    export RUSTUP_TOOLCHAIN=nightly
    cargo build --frozen --release
}

package() {
    cd $_name

    install -Dm0755 -t "$pkgdir/usr/bin/" "target/release/$_name"
    install -Dm644 LICENSE.md "$pkgdir/usr/share/licenses/$_name/LICENSE.md"
}
