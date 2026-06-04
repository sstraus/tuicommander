#!/bin/sh
# vim: set expandtab ts=2 sw=2 :
set -e

REPO="${TUICOMMANDER_REPO:-sstraus/tuicommander}"
API_URL="https://api.github.com/repos/$REPO/releases/latest"

OSX_HOME="${TUICOMMANDER_HOME:-/Applications}"

if [ $# -gt 0 ] && [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
    echo "usage: $0 [version]"
    exit 1
fi
if [ $# -eq 1 ]; then
    WANTED_VER="$1"
    shift $((OPTIND - 1))
fi

get_download_url() {
  curl -s "$API_URL" \
    | grep -o "\"browser_download_url\": *\"[^\"]*${1}\"" \
    | head -1 \
    | grep -o 'https://[^"]*'
}

install_macos() {
  local APP_HOME
  local APP_VER

  if [ -d "$OSX_HOME/TUICommander.app" ]; then
    APP_HOME="$OSX_HOME/TUICommander.app"
  else
    # might not be in the usual place, but if the user has set up tuic we can find it from that
    if [ -e $(command -v tuic) ]; then
      local TUIC=$(readlink $(command -v tuic))
      APP_HOME=${TUIC%/Contents/MacOS/tuic}
    fi
  fi

  if [ -n "$APP_HOME" ]; then
    # verify it actually looks like the app
    if [ -f "$APP_HOME/Contents/Info.plist" ]; then
      APP_VER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "$APP_HOME/Contents/Info.plist")
      if [ -n "$APP_VER" ]; then
        echo "Found ${APP_VER} currently installed"
      fi
    fi
  fi

  # get the latest version from GitHub
  # following is a bit of a hack to get the version to keep script external tool dependencies to built-in only
  local LATEST_VER="unknown"
  LATEST_VER=$(curl -sS --fail-with-body -L \
      -H "Accept: application/vnd.github+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "$API_URL" \
      | grep "tag_name" \
      | sed 's/^.*"v\([^"]*\)",$/\1/')
  if [ $? -ne 0 ]; then
    echo "Unable to get the latest version from GitHub"
    exit 1
  fi
  if [ -z "$LATEST_VER" ]; then
    echo "Unable to determine latest version from GitHub"
    exit 1
  fi
  echo "Found ${LATEST_VER} as latest release version"

  if [ -n "$WANTED_VER" ]; then
    echo "Looking to install ${WANTED_VER} instead"
    # TODO: should sanity check whether WANTED_VER actually exists -- but for now, it will just fail below
    LATEST_VER="$WANTED_VER"
  fi

  if [ "$LATEST_VER" == "$APP_VER" ]; then
    echo "No need to update"
    exit 0
  fi
exit

  local ASSET="TUICommander_aarch64.app.tar.gz"
  local URL="https://github.com/$REPO/releases/download/v$LATEST_VER/$ASSET"
  echo "Downloading $ASSET..."
  curl -LSs "$URL" \
    | tar xz -C "$OSX_HOME"
  if [ $? -ne 0 ]; then
    echo "Unable to download the latest version from GitHub"
    exit 1
  fi
  echo "Installed to ${OSX_HOME}/TUICommander.app"
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

exit 0
