# Maintainer: Your Name <your.email@example.com>
pkgname=barstocker
pkgver=1.0.0
pkgrel=1
pkgdesc="A status widget for stocks and cryptocurrencies using Tiingo and Kraken APIs"
arch=('x86_64')
url="https://github.com/its-a-unixsystem/barstocker"
license=('MIT')
depends=()  # No runtime dependencies for the binary (most dependencies are built via Cargo)
makedepends=('rust' 'cargo' 'git')
source=("git+https://github.com/its-a-unixsystem/barstocker.git")
sha256sums=('SKIP')  # For git sources, you can use SKIP

build() {
    cd "$srcdir/$pkgname"
    # Build the binary in release mode
    cargo build --release
}

package() {
    cd "$srcdir/$pkgname"
    # Install the binary into /usr/bin
    install -Dm755 "target/release/stocker" "$pkgdir/usr/bin/barstocker"
    
    # Optionally, install additional files such as the README and LICENSE:
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
