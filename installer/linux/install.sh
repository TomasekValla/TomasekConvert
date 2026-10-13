#!/bin/sh
# Instalace TomasekConvert na Linuxu (bez práv správce do ~/.local, jako root do /usr/local).
#   ./install.sh               interaktivně
#   ./install.sh --yes         bez otázek (zástupce + Open With)
#   ./install.sh --uninstall   odinstalace
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ "$(id -u)" -eq 0 ]; then
  BIN=/usr/local/bin; APPS=/usr/share/applications; ICONS=/usr/share/icons/hicolor/256x256/apps
else
  BIN="$HOME/.local/bin"; APPS="$HOME/.local/share/applications"; ICONS="$HOME/.local/share/icons/hicolor/256x256/apps"
fi
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

if [ "$1" = "--uninstall" ]; then
  rm -f "$BIN/TomasekConvert" "$APPS/TomasekConvert.desktop" "$ICONS/tomasekconvert.png" "$AUTOSTART/TomasekConvert.desktop"
  echo "TomasekConvert removed."; exit 0
fi

ask() { # ask "otázka" default(y/n)
  [ "$1" = "--yes" ] && return 0
  if [ "$ASSUME_YES" = 1 ]; then return 0; fi
  printf "%s [%s] " "$2" "$3"; read a; a="${a:-$3}"
  case "$a" in y|Y|yes|YES|a|A) return 0;; *) return 1;; esac
}
[ "$1" = "--yes" ] && ASSUME_YES=1

mkdir -p "$BIN" "$APPS" "$ICONS"
install -m 755 "$HERE/TomasekConvert" "$BIN/TomasekConvert"
install -m 644 "$HERE/icon.png" "$ICONS/tomasekconvert.png"

if ask x "Add TomasekConvert to the application menu and the “Open with” list?" y; then
  sed "s|^Exec=.*|Exec=$BIN/TomasekConvert %F|" "$HERE/tomasekconvert.desktop" > "$APPS/TomasekConvert.desktop"
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" || true
fi
if [ "$ASSUME_YES" != 1 ] && ask x "Run TomasekConvert automatically at login?" n; then
  mkdir -p "$AUTOSTART"
  sed "s|^Exec=.*|Exec=$BIN/TomasekConvert|" "$HERE/tomasekconvert.desktop" > "$AUTOSTART/TomasekConvert.desktop"
fi
echo "Installed to $BIN/TomasekConvert"
echo "Tip: install ImageMagick from your package manager (apt/dnf/pacman) for rare image formats (HEIC, AVIF, PSD…); common ones work without it."
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Note: add $BIN to your PATH.";; esac
