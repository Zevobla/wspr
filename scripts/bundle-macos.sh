#!/usr/bin/env bash
#
# bundle-macos.sh -- assemble a macOS `whspr.app` bundle (+ .dmg and .zip)
# for whspr-app. Runnable both locally and from CI (see
# .github/workflows/release.yml).
#
# What it does, and why:
#
#   * Builds the release binary with `nix build .#whspr-app` (or reuses a
#     prebuilt one passed via --binary).
#
#   * Generates `whspr.icns` FROM the vector source
#     `crates/whspr-app/assets/icon.svg` at bundle time. The repo
#     intentionally commits no raster/.icns (see .gitignore) -- the icon's
#     *shape* lives in git as a vector and only becomes pixels here. `sips`
#     cannot read SVG, so we rasterize the SVG to a 1024px PNG master with
#     `resvg` (pulled hermetically via `nix shell nixpkgs#resvg`), loading
#     the pinned Archivo ExtraBold face (`$ARCHIVO_DIR`, exposed by the
#     flake) so the wordmark "w" renders. `sips -z` then downsamples the
#     master into every iconset size and `iconutil -c icns` packs them.
#
#   * Assembles whspr.app/Contents/{MacOS,Resources,Info.plist,PkgInfo}.
#     A real bundle with a proper Info.plist is also what makes macOS apply
#     the system dark/light appearance to the iced GUI (a bare `cargo run`
#     binary doesn't get that).
#
#   * Produces a distributable: a .zip (via `ditto`) and a .dmg (via
#     `hdiutil`). UNSIGNED by default -- signing/notarization is layered on
#     in CI when the MACOS_* secrets are present (see release.yml).
#
# No build artifact produced here (.icns, .png, .app, .dmg, .zip) is ever
# committed; everything lands under the output dir (default: dist/), which
# .gitignore excludes.

set -euo pipefail

# --- macOS-only guard ------------------------------------------------------
# iconutil/sips/hdiutil/ditto/plutil are macOS system tools.
if [ "$(uname -s)" != "Darwin" ]; then
  echo "error: this script bundles a macOS .app and must run on macOS" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- arguments -------------------------------------------------------------
VERSION=""
PREBUILT_BINARY=""
OUT_DIR="$REPO_ROOT/dist"

usage() {
  cat >&2 <<'EOF'
Usage: bundle-macos.sh [options]

  -v, --version VERSION   App version (default: read from Cargo.toml).
                          A leading "v" (e.g. from a git tag v0.1.0) is stripped.
  -b, --binary PATH       Use a prebuilt whspr-app binary instead of `nix build`.
  -o, --out DIR           Output directory (default: <repo>/dist).
  -h, --help              Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    -v|--version) VERSION="${2:-}"; shift 2 ;;
    -b|--binary)  PREBUILT_BINARY="${2:-}"; shift 2 ;;
    -o|--out)     OUT_DIR="${2:-}"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

# --- version ---------------------------------------------------------------
# Default to the workspace version in Cargo.toml ([workspace.package]).
if [ -z "$VERSION" ]; then
  VERSION="$(grep -E '^version = ' "$REPO_ROOT/Cargo.toml" | head -1 \
    | sed -E 's/.*"([^"]+)".*/\1/')"
fi
# Tags are `vX.Y.Z`; the plist wants a bare X.Y.Z.
VERSION="${VERSION#v}"
if [ -z "$VERSION" ]; then
  echo "error: could not determine version" >&2
  exit 1
fi

echo "==> whspr macOS bundle: version $VERSION"

# --- locate / build the binary --------------------------------------------
if [ -n "$PREBUILT_BINARY" ]; then
  if [ ! -x "$PREBUILT_BINARY" ]; then
    echo "error: --binary '$PREBUILT_BINARY' is not an executable file" >&2
    exit 1
  fi
  BIN="$PREBUILT_BINARY"
  echo "==> using prebuilt binary: $BIN"
else
  echo "==> building release binary: nix build .#whspr-app"
  STORE_PATH="$(nix build "$REPO_ROOT#whspr-app" --no-link --print-out-paths)"
  BIN="$STORE_PATH/bin/whspr-app"
  if [ ! -x "$BIN" ]; then
    echo "error: expected binary not found at $BIN" >&2
    exit 1
  fi
  echo "==> built: $BIN"
fi

# --- locate the pinned Archivo ExtraBold face ------------------------------
# The icon.svg's "w" is set in Archivo ExtraBold. Prefer $ARCHIVO_DIR (set
# when running inside `nix develop`); otherwise ask the flake's devShell for
# it so the exact content-pinned face is used, identical to what the app
# renders with.
if [ -n "${ARCHIVO_DIR:-}" ] && [ -f "$ARCHIVO_DIR/Archivo-ExtraBold.ttf" ]; then
  FONT_FILE="$ARCHIVO_DIR/Archivo-ExtraBold.ttf"
else
  echo "==> resolving ARCHIVO_DIR from the flake devShell"
  RESOLVED_DIR="$(nix develop "$REPO_ROOT" --command sh -c 'printf %s "$ARCHIVO_DIR"')"
  FONT_FILE="$RESOLVED_DIR/Archivo-ExtraBold.ttf"
fi
if [ ! -f "$FONT_FILE" ]; then
  echo "error: Archivo ExtraBold face not found at $FONT_FILE" >&2
  exit 1
fi
echo "==> icon font: $FONT_FILE"

# --- work in a scratch dir -------------------------------------------------
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/whspr-bundle.XXXXXX")"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

SVG="$REPO_ROOT/crates/whspr-app/assets/icon.svg"
if [ ! -f "$SVG" ]; then
  echo "error: icon source not found at $SVG" >&2
  exit 1
fi

# --- rasterize the SVG -> 1024px PNG master via resvg ----------------------
# `--skip-system-fonts` + `--use-font-file` makes rendering deterministic:
# only the pinned Archivo face is in the DB, so the "w" (font-family="Archivo",
# font-weight=800) resolves to it regardless of what's installed on the host.
MASTER_PNG="$WORK_DIR/icon_1024.png"
echo "==> rasterizing $SVG -> 1024px PNG (resvg)"
nix shell nixpkgs#resvg --command resvg \
  --skip-system-fonts \
  --use-font-file "$FONT_FILE" \
  -w 1024 -h 1024 \
  "$SVG" "$MASTER_PNG"

# --- build the .iconset and pack the .icns ---------------------------------
ICONSET="$WORK_DIR/whspr.iconset"
mkdir -p "$ICONSET"
# Standard macOS iconset: each logical size at 1x and 2x (Retina).
#   name                 pixel size
for entry in \
  "icon_16x16.png:16" \
  "icon_16x16@2x.png:32" \
  "icon_32x32.png:32" \
  "icon_32x32@2x.png:64" \
  "icon_128x128.png:128" \
  "icon_128x128@2x.png:256" \
  "icon_256x256.png:256" \
  "icon_256x256@2x.png:512" \
  "icon_512x512.png:512" \
  "icon_512x512@2x.png:1024"; do
  name="${entry%:*}"
  size="${entry##*:}"
  sips -z "$size" "$size" "$MASTER_PNG" --out "$ICONSET/$name" >/dev/null
done

ICNS="$WORK_DIR/whspr.icns"
echo "==> packing iconset -> whspr.icns (iconutil)"
iconutil -c icns "$ICONSET" -o "$ICNS"

# --- assemble the .app bundle ----------------------------------------------
mkdir -p "$OUT_DIR"
APP="$OUT_DIR/whspr.app"
rm -rf "$APP"
CONTENTS="$APP/Contents"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

# Binary: copy out of the (read-only) nix store and make it writable +
# executable so codesign (in CI) and the user can run it.
cp "$BIN" "$CONTENTS/MacOS/whspr"
chmod u+w,+x "$CONTENTS/MacOS/whspr"

cp "$ICNS" "$CONTENTS/Resources/whspr.icns"

# PkgInfo: the classic 8-byte type+creator ("APPL" + no-creator "????").
printf 'APPL????' > "$CONTENTS/PkgInfo"

# Info.plist. CFBundleIconFile is "whspr" (extension optional); macOS reads
# Resources/whspr.icns. The TCC usage strings cover the prompts whspr shows;
# Accessibility + Input Monitoring (global hotkey + text injection) are NOT
# declarable in Info.plist and are granted by the user at runtime in System
# Settings > Privacy & Security.
cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>whspr</string>
	<key>CFBundleDisplayName</key>
	<string>whspr</string>
	<key>CFBundleIdentifier</key>
	<string>com.zevobla.whspr</string>
	<key>CFBundleExecutable</key>
	<string>whspr</string>
	<key>CFBundleIconFile</key>
	<string>whspr</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${VERSION}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.productivity</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<!-- TCC usage strings. Microphone: whspr records the mic while the
	     hotkey is held. Apple Events: used to talk to the frontmost app.
	     Accessibility and Input Monitoring (global hotkey capture + text
	     injection) cannot be declared here -- macOS prompts for them at
	     runtime in System Settings > Privacy & Security. -->
	<key>NSMicrophoneUsageDescription</key>
	<string>whspr records your voice while you hold the dictation hotkey, then transcribes it to text.</string>
	<key>NSAppleEventsUsageDescription</key>
	<string>whspr inserts transcribed text into the app you are working in.</string>
</dict>
</plist>
PLIST

# Validate the plist -- fail loudly if we produced something malformed.
plutil -lint "$CONTENTS/Info.plist"

echo "==> assembled: $APP"

# --- distributables: .zip (ditto) + .dmg (hdiutil) -------------------------
ZIP="$OUT_DIR/whspr-${VERSION}-macos.zip"
DMG="$OUT_DIR/whspr-${VERSION}-macos.dmg"
rm -f "$ZIP" "$DMG"

# ditto preserves bundle attributes/symlinks better than `zip`.
echo "==> zipping -> $ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

echo "==> building dmg -> $DMG"
hdiutil create \
  -volname "whspr" \
  -srcfolder "$APP" \
  -ov -format UDZO \
  "$DMG" >/dev/null

echo ""
echo "==> done. Outputs:"
echo "      app: $APP"
echo "      zip: $ZIP"
echo "      dmg: $DMG"
