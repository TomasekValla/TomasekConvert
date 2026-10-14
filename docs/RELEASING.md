# Releasing, installers and updates

## Cutting a release

1. Bump `version` in `Cargo.toml`, update `CHANGELOG.md`.
2. `git tag v2.2.0 && git push origin v2.2.0`
3. `.github/workflows/release.yml` builds all five targets (Windows x64 / ARM64, Linux x64 / ARM64, macOS arm64),
   creates installers and `.sha256` files and attaches them to a GitHub Release.

## What each platform gets

| Platform | Artifacts | How |
|---|---|---|
| Windows | `TomasekConvert-Setup-<arch>.exe` (Inno Setup, `installer/windows/TomasekConvert.iss`), portable `.exe` | the installer asks for: desktop icon, run at startup (optionally quietly in the tray), “Open with”, right-click menu |
| Linux | `.tar.gz` with `install.sh`, `.deb` (`cargo-deb`) | `install.sh` asks about menu entry / autostart; the `.deb` depends on `imagemagick` (qpdf is bundled) |
| macOS | `.dmg` with `TomasekConvert.app` (`installer/macos/make-app.sh`) | ad-hoc signed; see below |

The same switches are available inside the app (*Settings → System integration*), so portable users can set them up too.

## Updates

Repository: `TomasekValla/TomasekConvert` (`REPO` in `src/update.rs`). Nothing contacts the internet unless the user
enables *Check for updates at startup* (first-run setup / Settings) or presses *Check now*.

How the in-app updater works (`src/update.rs`):

1. `GET https://api.github.com/repos/<REPO>/releases/latest`, newer tag → picks the asset for this platform and the
   `<asset>.sha256` next to it (the release workflow creates both – **keep these exact names**):
   `TomasekConvert-Setup-<arch>.exe`, `TomasekConvert-portable-windows-<arch>.exe`, `TomasekConvert-portable-linux-<arch>.tar.gz`
   (portable files carry `portable` in the name since 2.2.1. The updater of 2.2.1+ also accepts the old names
   `TomasekConvert-windows-<arch>.exe` / `TomasekConvert-linux-<arch>.tar.gz`. A 2.2.0 portable copy only knows the old names, so for it
   the 2.2.1 update falls back to "open the release page" – attach old-named copies to a release if those should update in place.)
2. Downloads only from `https://github.com/<REPO>/releases/download/…`, verifies SHA-256 (no checksum → no update).
3. Install: **Windows installed copy** (`unins000.exe` next to the exe) runs the Setup silently
   (`/SILENT /CLOSEAPPLICATIONS /RELAUNCH=1|tray`; the script relaunches the app; settings live in
   `%LOCALAPPDATA%\TomasekConvert` and stay); **portable exe / Linux binary in a writable folder** is renamed to `.old`
   and replaced, then restarted; **.deb / macOS .app** only open the release page.
4. *Install automatically* (opt-in): downloads in the background and installs when the window is hidden in the tray and
   no conversion runs; with an open window the user gets a button.

Possible later steps: winget / Scoop / Homebrew / AUR manifests, cargo-dist (installers + `axoupdater`), macOS in-place update.

## Signing (recommended before a wider release)

- **Windows**: an Authenticode certificate (or Azure Trusted Signing) removes the SmartScreen warning.
- **macOS**: Developer ID + notarisation (`codesign`, `notarytool`); without it Gatekeeper requires right-click → Open.
- Publish the `.sha256` files (already generated) so users can verify downloads.

## Known platform gaps

- macOS “Open With” / drag onto the Dock icon is delivered through Apple Events, which the current windowing layer does not
  forward – open files from inside the app until this is implemented.
- Tray icon: Windows only for now (`tray-icon` + Win32). On macOS/Linux a hidden egui window may not receive repaint
  requests (needs a real test), Linux additionally needs StatusNotifier (`ksni`).
- ImageMagick is not bundled on Linux/macOS (no official portable build): the system one is used, otherwise the built-in
  converter handles common formats (PNG, JPEG, GIF, BMP, TIFF, WebP, ICO, QOI, TGA, PNM).
- Linux: one `.deb` / tarball built on Ubuntu 22.04 (glibc 2.35). A Flatpak or AppImage would cover more distributions.
