#!/bin/sh
# hauntty installer — downloads the latest prebuilt binary for your platform.
set -eu

REPO="winterweken/hauntty"
BIN="hauntty"
INSTALL_DIR="${HAUNTTY_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) os_part="apple-darwin" ;;
  Linux)  os_part="unknown-linux-gnu" ;;
  *) echo "unsupported OS: $os" >&2; exit 1 ;;
esac

case "$arch" in
  arm64|aarch64) arch_part="aarch64" ;;
  x86_64|amd64)  arch_part="x86_64" ;;
  *) echo "unsupported arch: $arch" >&2; exit 1 ;;
esac

target="${arch_part}-${os_part}"

echo "Finding latest release of $REPO..."
tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | head -1 | cut -d '"' -f 4)"
if [ -z "$tag" ]; then
  echo "could not determine latest release tag" >&2
  exit 1
fi

asset="${BIN}-${tag}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${tag}/${asset}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $asset..."
curl -fsSL "$url" -o "$tmp/$asset"
tar xzf "$tmp/$asset" -C "$tmp"

mkdir -p "$INSTALL_DIR"
mv "$tmp/${BIN}-${tag}-${target}/${BIN}" "$INSTALL_DIR/${BIN}"
chmod +x "$INSTALL_DIR/${BIN}"

echo "Installed ${BIN} ${tag} to ${INSTALL_DIR}/${BIN}"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) : ;;
  *) echo "Note: add $INSTALL_DIR to your PATH." ;;
esac
