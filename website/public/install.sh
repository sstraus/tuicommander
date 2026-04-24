#!/bin/sh
set -e

REPO="sstraus/tuicommander"
API_URL="https://api.github.com/repos/$REPO/releases/latest"

get_download_url() {
  curl -s "$API_URL" \
    | grep -o "\"browser_download_url\": *\"[^\"]*${1}\"" \
    | head -1 \
    | grep -o 'https://[^"]*'
}

install_macos() {
  ASSET="TUICommander_aarch64.app.tar.gz"
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
  echo "Downloading $ASSET..."
  curl -LSs "$URL" | tar xz -C /Applications
  echo "Installed to /Applications/TUICommander.app"
  echo "Tip: you can also install via Homebrew: brew install sstraus/tap/tuicommander"
}

install_linux() {
  if command -v dpkg > /dev/null 2>&1; then
    echo "Detected dpkg — installing .deb package..."
    URL=$(get_download_url 'amd64\.deb')
    if [ -z "$URL" ]; then echo "Error: .deb asset not found in latest release"; exit 1; fi
    TMP=$(mktemp /tmp/tuicommander-XXXXXX.deb)
    curl -fsSL "$URL" -o "$TMP"
    sudo dpkg -i "$TMP"
    rm -f "$TMP"
  elif command -v rpm > /dev/null 2>&1; then
    echo "Detected rpm — installing .rpm package..."
    URL=$(get_download_url 'x86_64\.rpm')
    if [ -z "$URL" ]; then echo "Error: .rpm asset not found in latest release"; exit 1; fi
    TMP=$(mktemp /tmp/tuicommander-XXXXXX.rpm)
    curl -fsSL "$URL" -o "$TMP"
    sudo rpm -i "$TMP"
    rm -f "$TMP"
  else
    echo "No dpkg or rpm found — installing AppImage..."
    URL=$(get_download_url 'amd64\.AppImage')
    if [ -z "$URL" ]; then echo "Error: AppImage asset not found in latest release"; exit 1; fi
    DEST="${HOME}/.local/bin/TUICommander.AppImage"
    mkdir -p "$(dirname "$DEST")"
    curl -fsSL "$URL" -o "$DEST"
    chmod +x "$DEST"
    echo "Installed to $DEST"
    echo "Make sure ~/.local/bin is in your PATH"
    return
  fi
  echo "TUICommander installed successfully!"
}

OS=$(uname -s)
echo "Installing TUICommander for $OS..."

case "$OS" in
  Darwin) install_macos ;;
  Linux)  install_linux ;;
  *)      echo "Unsupported OS: $OS. Download manually from https://github.com/$REPO/releases"; exit 1 ;;
esac
