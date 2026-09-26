#!/bin/sh
# Fetch the LAML interpreter that Keplr ships with.
#
# Keplr's event service is a LAML program, so a Keplr install has to carry the
# interpreter with it. This script puts it where the runtime resolver looks, and
# is safe to run repeatedly.
#
#   scripts/fetch-laml.sh
#   LAML_VERSION=4.1.0 scripts/fetch-laml.sh
#
# Two sources, in order:
#   1. a prebuilt release asset for this platform, when the LAML release has one
#   2. a build from the pinned LAML tag, which needs git and a C++20 compiler
#
# The runtime also accepts KEPLR_LAML=/path/to/laml and a `laml` on PATH, so this
# is a convenience for packaging rather than a requirement for development.

set -eu

VERSION="${LAML_VERSION:-4.1.0}"
REPO="${LAML_REPO:-NaveenSingh9999/LAML}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${LAML_DEST:-$ROOT/assets/laml}"
WORK="${LAML_WORK:-$ROOT/target/laml-build}"

asset_name() {
  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
  esac
  case "$os" in
    linux) echo "laml-linux-$arch" ;;
    darwin) echo "laml-macos-$arch" ;;
    *) echo "" ;;
  esac
}

asset="$(asset_name)"
target="${DEST}/${asset}"

if [ -x "$target" ] && "$target" version 2>/dev/null | grep -q "$VERSION"; then
  echo "laml $VERSION already at $target"
  exit 0
fi

mkdir -p "$DEST"

download() {
  url="https://github.com/${REPO}/releases/download/v${VERSION}/${asset}"
  echo "fetching $url"
  tmp="${target}.download"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$tmp" "$url"
  else
    wget -q -O "$tmp" "$url"
  fi
  chmod 755 "$tmp"
  mv "$tmp" "$target"
}

if ! download 2>/dev/null; then
  rm -f "${target}.download"
  echo "no prebuilt $asset in LAML v${VERSION}, building from source"
  if ! command -v git >/dev/null 2>&1; then
    echo "git is required to build LAML from source" >&2
    exit 1
  fi
  mkdir -p "$WORK"
  if [ ! -d "$WORK/LAML/.git" ]; then
    git clone --depth 1 --branch "v${VERSION}" "https://github.com/${REPO}.git" "$WORK/LAML"
  fi
  make -C "$WORK/LAML/ng" -j"$(nproc 2>/dev/null || echo 2)"
  cp "$WORK/LAML/ng/laml" "$target"
  chmod 755 "$target"
fi

"$target" version >/dev/null
echo "laml $VERSION installed at $target"
