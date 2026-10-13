#!/bin/bash
# Vytvoří TomasekConvert.app a .dmg z přeložené binárky.
#   make-app.sh <cesta-k-binárce> <verze> <výstupní.dmg>
set -euo pipefail
BIN="$1"; VERSION="$2"; OUT="$3"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="dist/TomasekConvert.app"
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/TomasekConvert"; chmod +x "$APP/Contents/MacOS/TomasekConvert"

# ikona .icns z assets/icon.png
ICONSET="$(mktemp -d)/icon.iconset"; mkdir -p "$ICONSET"
for s in 16 32 64 128 256; do
  sips -z $s $s "$ROOT/assets/icon.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) "$ROOT/assets/icon.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>TomasekConvert</string>
  <key>CFBundleDisplayName</key><string>TomasekConvert</string>
  <key>CFBundleIdentifier</key><string>com.tomasekconvert.app</string>
  <key>CFBundleExecutable</key><string>TomasekConvert</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key><array>
    <dict>
      <key>CFBundleTypeName</key><string>Any file</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key><array><string>public.item</string><string>public.folder</string></array>
    </dict>
  </array>
</dict></plist>
PLIST

# ad-hoc podpis (bez Apple Developer účtu; při prvním spuštění: pravé tlačítko → Otevřít)
codesign --force --deep -s - "$APP" || true
hdiutil create -volname "TomasekConvert" -srcfolder "$APP" -ov -format UDZO "$OUT"
