#!/usr/bin/env bash
set -euo pipefail

REPO="lucasilverentand/pane"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)  TARGET="aarch64-apple-darwin" ;;
      x86_64) TARGET="x86_64-apple-darwin" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64)          TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64 | arm64) TARGET="aarch64-unknown-linux-gnu" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# Fetch latest release tag
echo "Fetching latest release..."
TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

if [ -z "$TAG" ]; then
  echo "Failed to fetch latest release tag" >&2
  exit 1
fi

echo "Installing pane $TAG ($TARGET)..."

ARCHIVE="pane-$TARGET.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ARCHIVE"

# Download and extract to a temp dir
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP/$ARCHIVE"
tar xzf "$TMP/$ARCHIVE" -C "$TMP"

# Install
mkdir -p "$INSTALL_DIR"
mv "$TMP/pane" "$INSTALL_DIR/pane"
chmod +x "$INSTALL_DIR/pane"

echo "Installed to $INSTALL_DIR/pane"

# Warn if INSTALL_DIR is not in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "Note: $INSTALL_DIR is not in your PATH."
  echo "Add it: export PATH=\"$INSTALL_DIR:\$PATH\""
fi
