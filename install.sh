#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Vaibhav
#
# bookmark installer — works on Termux (Android aarch64), Linux x86_64/aarch64.
#
#   curl -fsSL https://github.com/thisisforlearn/bookmark/releases/latest/download/install.sh | bash
#
# Options:
#   --dev            install from a local build (target/release/bookmark)
#   --yes            non-interactive, no prompts
#   --uninstall      remove the binary and (optionally) data
#   --version VER    install a specific release tag
set -euo pipefail

REPO="thisisforlearn/bookmark"
VERSION="latest"
DEV=0
ASSUME_YES=0
UNINSTALL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --dev) DEV=1 ;;
    --yes|-y) ASSUME_YES=1 ;;
    --uninstall) UNINSTALL=1 ;;
    --version) shift; VERSION="${1:-latest}" ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

say()  { printf '\033[1;34m▸\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

is_termux() { [ -n "${PREFIX:-}" ] && case "$PREFIX" in *com.termux*) return 0;; esac; return 1; }

# --- install location --------------------------------------------------------
if is_termux; then
  BIN_DIR="$PREFIX/bin"
  STATE_DIR="$HOME/.bookmark"
else
  BIN_DIR="${HOME}/.local/bin"
  STATE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/bookmark"
fi
mkdir -p "$BIN_DIR"
BIN_PATH="$BIN_DIR/bookmark"

if [ "$UNINSTALL" = 1 ]; then
  say "removing $BIN_PATH"
  rm -f "$BIN_PATH"
  if [ "$ASSUME_YES" = 1 ]; then
    say "removing data at $STATE_DIR"
    rm -rf "$STATE_DIR"
  else
    printf 'Remove your data at %s too? [y/N] ' "$STATE_DIR"
    read -r ans || ans=n
    case "$ans" in y|Y) rm -rf "$STATE_DIR"; say "data removed";; esac
  fi
  say "done"
  exit 0
fi

# --- architecture ------------------------------------------------------------
detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os:$arch" in
    Linux:x86_64|Linux:amd64) echo "x86_64-unknown-linux-musl" ;;
    Linux:aarch64|Linux:arm64) echo "aarch64-unknown-linux-musl" ;;
    Linux:armv7*|Linux:armv8l) echo "armv7-unknown-linux-musleabihf" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    *) echo "" ;;
  esac
}

TARGET="$(detect_target)"

if [ "$DEV" = 1 ]; then
  SRC="$(cd "$(dirname "$0")" && pwd)/target/release/bookmark"
  [ -x "$SRC" ] || die "no local build at $SRC — run: cargo build --release"
  say "installing local build"
  install -m 0755 "$SRC" "$BIN_PATH"
else
  [ -n "$TARGET" ] || die "unsupported platform: $(uname -s) $(uname -m)"
  if [ "$VERSION" = "latest" ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
  else
    BASE="https://github.com/$REPO/releases/download/$VERSION"
  fi
  ASSET="bookmark-$TARGET"
  say "downloading $ASSET"
  TMP="$(mktemp)"
  trap 'rm -f "$TMP" "$TMP.sum"' EXIT
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$BASE/$ASSET" -o "$TMP" || die "download failed"
    curl -fsSL "$BASE/$ASSET.sha256" -o "$TMP.sum" || die "checksum download failed"
  else
    wget -qO "$TMP" "$BASE/$ASSET" || die "download failed"
    wget -qO "$TMP.sum" "$BASE/$ASSET.sha256" || die "checksum download failed"
  fi
  EXPECTED="$(awk '{print $1}' "$TMP.sum")"
  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL="$(sha256sum "$TMP" | awk '{print $1}')"
  else
    ACTUAL="$(shasum -a 256 "$TMP" | awk '{print $1}')"
  fi
  [ "$EXPECTED" = "$ACTUAL" ] || die "checksum mismatch (expected $EXPECTED, got $ACTUAL)"
  say "checksum ok"
  install -m 0755 "$TMP" "$BIN_PATH"
fi

say "installed $BIN_PATH"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) warn "$BIN_DIR is not on your PATH — add: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

# --- initialise --------------------------------------------------------------
if [ ! -f "$STATE_DIR/bookmark.db" ]; then
  say "initialising data directory"
  BOOKMARK_CONFIG="${BOOKMARK_CONFIG:-$HOME/.config/bookmark/config.toml}" "$BIN_PATH" init || warn "init failed; run it manually"
else
  say "existing data found at $STATE_DIR"
fi

# --- Termux niceties ---------------------------------------------------------
if is_termux; then
  if [ "$ASSUME_YES" = 0 ]; then
    printf 'Install Termux:Boot hook + wake-lock? [y/N] '
    read -r ans || ans=n
  else
    ans=y
  fi
  case "$ans" in
    y|Y)
      "$BIN_PATH" service install || warn "could not install boot hook"
      command -v termux-wake-lock >/dev/null 2>&1 && termux-wake-lock || true
      say "boot hook installed (install the Termux:Boot app once)"
      ;;
  esac
fi

echo
say "done. start it with:"
echo "    bookmark serve --host 127.0.0.1 --port 8080"
echo
echo "  then open http://127.0.0.1:8080"
echo "  for LAN/Tailscale:  bookmark serve --host 0.0.0.0 --port 8080"
