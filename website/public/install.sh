#!/bin/sh
# vim: set expandtab ts=2 sw=2 :
set -e

REPO="${TUICOMMANDER_REPO:-sstraus/tuicommander}"
API_URL="https://api.github.com/repos/$REPO"
RELEASE_URL="$API_URL/releases"

OSX_HOME="${TUICOMMANDER_HOME:-/Applications}"

if [ $# -gt 0 ] && [ "$1" == "-h" ] || [ "$1" == "--help" ]; then
    echo "usage: $0 [version]"
    exit 1
fi
if [ $# -eq 1 ]; then
    WANTED_VER="${1#v}"
    shift $((OPTIND - 1))
fi

get_download_url() {
  curl -s "$RELEASE_URL/latest" \
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
      APP_HOME="${TUIC%/Contents/MacOS/tuic}"
      OSX_HOME="${APP_HOME%/TUICommander.app}"
    fi
  fi

  if [ -n "$APP_HOME" ]; then
    # verify it actually looks like the app
    if [ -f "$APP_HOME/Contents/Info.plist" ]; then
      APP_VER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "$APP_HOME/Contents/Info.plist")
      if [ -n "$APP_VER" ]; then
        # 1.3.0-nightly.20260603.t2024
        # 1.3.0
        echo "Found ${APP_VER} currently installed"
      fi
    fi
  fi

  if [ -n "$APP_VER" ] && [ -z "$WANTED_VER" ]; then
    # determine if the current installed version is a nightly as
    # that has a slightly different version checking approach
    echo "$APP_VER" | grep -q "nightly"
    if [ $? -eq 0 ]; then
      echo "Found current version to be a nightly version"
      WANTED_VER="nightly"
    fi
  fi

  # get the latest version from GitHub
  local LATEST_VER="unknown"
  if [ "$WANTED_VER" == "nightly" ]; then
    # fetch the latest json for nightly so we can find the version string there
    # "version": "1.3.0-nightly.20260603.t2025",
    LATEST_VER=$(curl -sS --fail-with-body -L \
        "https://github.com/${REPO}/releases/download/nightly/latest.json" \
        | grep '"version"' \
        | sed 's/^.*": "\([^"]*\)",$/\1/')
    if [ $? -ne 0 ]; then
      echo "Unable to get the latest nightly version from GitHub"
      exit 1
    fi
    echo "Found $LATEST_VER as latest nightly"

    # nightly ver numbers can be slightly different between the actual app version found in Info.plist and the version found in latest.json
    # so we strip off the tail end of the version strings -- as nightly is supposed to be once a day, we should be ok
    LATEST_VER="${LATEST_VER%.t*}"
    APP_VER="${APP_VER%.t*}"
  elif [ -n "$WANTED_VER" ]; then
    # no need to check GitHub, the user specified the version they want
    LATEST_VER="v${WANTED_VER}"
    APP_VER="v${APP_VER}"
  else
    # v1.3.0 -> 1.3.0
    LATEST_VER=$(curl -sS --fail-with-body -L \
        -H "Accept: application/vnd.github+json" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        "$RELEASE_URL/latest" \
        | grep "tag_name" \
        | sed 's/^.*": "v\([^"]*\)",$/\1/')
    if [ $? -ne 0 ]; then
      echo "Unable to get the latest version from GitHub"
      exit 1
    fi
    echo "Found ${LATEST_VER} as latest release version"
    LATEST_VER="v${LATEST_VER}"
    APP_VER="v${APP_VER}"
    WANTED_VER="v${LATEST_VER}"
  fi

  if [ -z "$LATEST_VER" ]; then
    echo "Unable to determine latest version from GitHub"
    exit 1
  fi

  if [ "$LATEST_VER" == "$APP_VER" ]; then
    echo "No need to update"
    exit 0
  fi

  local ASSET="TUICommander_aarch64.app.tar.gz"
  local URL="https://github.com/$REPO/releases/download/$WANTED_VER/$ASSET"
  echo "Downloading and installing version ${LATEST_VER}"
  curl -LSs "$URL" \
    | tar xz -C "$OSX_HOME"
  if [ $? -ne 0 ]; then
    echo "Unable to download version ${WANTED_VER} from GitHub"
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
