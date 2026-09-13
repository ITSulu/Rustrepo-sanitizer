#!/usr/bin/env bash
set -euo pipefail

version=${1:?usage: build-artifacts.sh VERSION OUTPUT_DIR [BINARY]}
out=${2:?usage: build-artifacts.sh VERSION OUTPUT_DIR [BINARY]}
binary=${3:-target/release/itsulu-repo-sanitizer}
platform=${RELEASE_PLATFORM:?RELEASE_PLATFORM is required}
arch=${RELEASE_ARCH:?RELEASE_ARCH is required}
name=rustrepo-sanitizer-${version}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid version" >&2; exit 2; }
mkdir -p "$out"
[[ -z "$(find "$out" -mindepth 1 -maxdepth 1 -print -quit)" ]] || { echo "output directory must be empty" >&2; exit 1; }
case "$arch" in x86_64|aarch64|amd64|arm64) ;; *) echo "unsupported architecture" >&2; exit 2 ;; esac
deb_arch=$arch
[[ "$arch" == x86_64 ]] && deb_arch=amd64
[[ "$arch" == aarch64 ]] && deb_arch=arm64
test -x "$binary"
actual=$($binary --version | awk '{print $NF}')
test "$actual" = "$version" || { echo "binary version $actual != $version" >&2; exit 1; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$binary" "$stage/itsulu-repo-sanitizer"
chmod 0755 "$stage/itsulu-repo-sanitizer"
tar -C "$stage" -czf "$out/${name}-${platform}-${arch}.tar.gz" itsulu-repo-sanitizer

formats=",${RELEASE_FORMATS:-},"
  if [[ "$formats" == *,arch,* ]]; then
    [[ "$arch" == x86_64 || "$arch" == aarch64 ]] || { echo "Arch packages require x86_64 or aarch64" >&2; exit 2; }
  command -v makepkg >/dev/null
  pkgroot="$stage/arch"; mkdir -p "$pkgroot/usr/bin"
  cp "$binary" "$pkgroot/usr/bin/itsulu-repo-sanitizer"
  cat > "$pkgroot/PKGBUILD" <<EOF
pkgname=rustrepo-sanitizer
pkgver=${version//./_}
pkgrel=1
pkgdesc='Create deterministic, sanitized AI review bundles from Git repositories'
arch=('$arch')
options=('!debug')
license=('Apache-2.0')
url='https://git.itsulu.com/itsulu/Rustrepo-sanitizer'
package() { install -Dm755 "\$startdir/usr/bin/itsulu-repo-sanitizer" "\$pkgdir/usr/bin/itsulu-repo-sanitizer"; }
EOF
  (cd "$pkgroot" && makepkg --nodeps --force >/dev/null)
  pkg=$(find "$pkgroot" -maxdepth 1 -type f -name '*.pkg.tar.*' -print -quit)
  test -n "$pkg"
  cp "$pkg" "$out/${name}-arch-${arch}.pkg.tar.zst"
fi

if [[ "$formats" == *,deb,* ]]; then
  command -v cargo-deb >/dev/null || { echo "cargo-deb is required for Debian packages" >&2; exit 1; }
  cargo deb --no-build --output "$out/${name}-linux-${deb_arch}.deb"
fi

if [[ "$formats" == *,rpm,* ]]; then
  command -v cargo-generate-rpm >/dev/null || { echo "cargo-generate-rpm is required for RPM packages" >&2; exit 1; }
  cargo generate-rpm --payload-compress gzip --output "$out/${name}-linux-${arch}.rpm"
fi

(cd "$out" && find . -maxdepth 1 -type f ! -name SHA256SUMS -printf '%f\n' | sort | xargs sha256sum > SHA256SUMS)
