#!/bin/sh
set -e

REPO="jaducku/atom"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) OS_NAME="darwin" ;;
  Linux)  OS_NAME="linux" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH_NAME="x86_64" ;;
  arm64|aarch64) ARCH_NAME="aarch64" ;;
  *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ASSET_NAME="zpro-code-${OS_NAME}-${ARCH_NAME}"

# Get latest release tag
if [ -z "$VERSION" ]; then
  VERSION=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
  if [ -z "$VERSION" ]; then
    echo "Failed to fetch latest version. Set VERSION=v0.1.0 manually."
    exit 1
  fi
fi

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}.tar.gz"

echo "Installing zpro-code ${VERSION} (${OS_NAME}/${ARCH_NAME})..."
echo "  From: ${URL}"
echo "  To:   ${INSTALL_DIR}/zpro-code"

# Download and install
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -sL "$URL" -o "${TMPDIR}/${ASSET_NAME}.tar.gz"
tar xzf "${TMPDIR}/${ASSET_NAME}.tar.gz" -C "$TMPDIR"

if [ -w "$INSTALL_DIR" ]; then
  mv "${TMPDIR}/zpro-code" "${INSTALL_DIR}/zpro-code"
else
  echo "  Need sudo to install to ${INSTALL_DIR}"
  sudo mv "${TMPDIR}/zpro-code" "${INSTALL_DIR}/zpro-code"
fi

chmod +x "${INSTALL_DIR}/zpro-code"

echo ""
echo "Installed! Run: zpro-code"
