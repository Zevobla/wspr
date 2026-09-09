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

# --- vendor non-system dylibs so the .app is self-contained ----------------
# The whspr-app binary links native dylibs that DON'T exist on a clean Mac:
#
#   * @rpath/libonnxruntime.1.17.1.dylib and @rpath/libsherpa-onnx-c-api.dylib
#     -- speaker diarization (whspr-diarize -> sherpa-rs). nix's fixupPhase
#     strips the build-time LC_RPATH that pointed into the sherpa-rs-sys build
#     sandbox, so the binary ends up with @rpath deps and ZERO LC_RPATH.
#   * /nix/store/.../libiconv.2.dylib (which re-exports libcharset.1.dylib)
#     -- an absolute path into THIS machine's nix store, absent everywhere the
#     app is actually installed.
#
# Either one makes the launched app dyld-crash ("Library not loaded") before a
# window ever shows -- and the headless gate (cargo build/test, nix flake
# check) never launches the GUI, so it doesn't catch it. We copy every
# non-system dependency into Contents/Frameworks, rewrite each reference to
# @rpath/<name>, and point the executable's rpath at Frameworks so dyld
# resolves the whole graph inside the bundle.
#
# Nothing here is committed: the dylibs are sourced at bundle time from the
# sherpa-rs download cache / the nix store the build we just ran used.
FRAMEWORKS="$CONTENTS/Frameworks"
mkdir -p "$FRAMEWORKS"

# A dependency is "system" (left untouched -- present on every macOS, served
# from the dyld shared cache) iff it lives under /usr/lib or /System.
is_system_dep() {
  case "$1" in
    /usr/lib/*|/System/*) return 0 ;;
    *) return 1 ;;
  esac
}

# The dependency install-names of a Mach-O, one per line, deduped. Only the
# tab-indented lines are dependencies: otool -L prints the file path (and, for
# universal2 dylibs like sherpa/onnx, a per-architecture "(architecture arm64):"
# banner) flush-left, and those must be skipped.
dep_names() {
  otool -L "$1" | awk '/^\t/ {print $1}' | sort -u
}

# Resolve the on-disk source for a dependency install-name:
#   * absolute paths (e.g. the /nix/store libiconv) ARE the source;
#   * @rpath/... names come from the sherpa-rs prebuilt download cache
#     (universal2), with the cargo target dirs as a fallback.
locate_dylib_source() {
  local dep="$1" name hit
  name="${dep##*/}"
  case "$dep" in
    /*) [ -f "$dep" ] && { printf '%s\n' "$dep"; return 0; } ;;
  esac
  for hit in "$HOME"/Library/Caches/sherpa-rs/*/*/sherpa-onnx-*/lib/"$name"; do
    [ -f "$hit" ] && { printf '%s\n' "$hit"; return 0; }
  done
  for hit in "$REPO_ROOT"/target/*/"$name" "$REPO_ROOT"/target/*/deps/"$name"; do
    [ -f "$hit" ] && { printf '%s\n' "$hit"; return 0; }
  done
  return 1
}

echo "==> vendoring non-system dylibs into Contents/Frameworks"

# Breadth-first over the binary and every dylib we copy, so transitive deps
# (sherpa-onnx-c-api -> onnxruntime, libiconv -> libcharset) come along too.
WORKLIST=("$CONTENTS/MacOS/whspr")
idx=0
while [ "$idx" -lt "${#WORKLIST[@]}" ]; do
  cur="${WORKLIST[$idx]}"
  idx=$((idx + 1))

  while IFS= read -r dep; do
    [ -z "$dep" ] && continue
    is_system_dep "$dep" && continue

    name="${dep##*/}"
    dest="$FRAMEWORKS/$name"

    if [ ! -f "$dest" ]; then
      if ! src="$(locate_dylib_source "$dep")"; then
        echo "error: cannot find dylib '$name' (needed by $cur)." >&2
        echo "       looked in the sherpa-rs cache" \
             "(~/Library/Caches/sherpa-rs/*/*/sherpa-onnx-*/lib) and" \
             "$REPO_ROOT/target/*; refusing to ship a broken bundle." >&2
        exit 1
      fi
      echo "    + $name"
      cp "$src" "$dest"
      chmod u+w "$dest"
      # Normalize the copy's own install id, and give it an @loader_path rpath
      # so its own @rpath deps resolve from the same Frameworks dir (unless it
      # already has one -- sherpa-onnx-c-api ships with @loader_path, and
      # re-adding it would error).
      install_name_tool -id "@rpath/$name" "$dest"
      if ! otool -l "$dest" | grep -q 'path @loader_path (offset'; then
        install_name_tool -add_rpath @loader_path "$dest"
      fi
      WORKLIST+=("$dest")
    fi

    # Repoint the referrer (binary or dylib) at the vendored copy.
    if [ "$dep" != "@rpath/$name" ]; then
      install_name_tool -change "$dep" "@rpath/$name" "$cur"
    fi
  done < <(dep_names "$cur")
done

# Point the executable's rpath at the bundled Frameworks so its @rpath deps
# (including everything we just rewrote to @rpath) resolve inside the .app.
install_name_tool -add_rpath @executable_path/../Frameworks "$CONTENTS/MacOS/whspr"

# install_name_tool invalidates any code signature, and macOS refuses to load
# a modified-but-signed Mach-O. Re-sign every bundled dylib (inner first),
# then seal the bundle -- which signs the main executable and records
# Frameworks in _CodeSignature/CodeResources. When CI has the signing secrets,
# release.yml re-signs the whole bundle for real on top of this.
echo "==> ad-hoc re-signing bundled dylibs + sealing the bundle"
for dylib in "$FRAMEWORKS"/*.dylib; do
  codesign --force --sign - "$dylib"
done
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "==> assembled: $APP"

# --- distributables: .zip (ditto) + .dmg (hdiutil) -------------------------
ZIP="$OUT_DIR/whspr-${VERSION}-macos.zip"
DMG="$OUT_DIR/whspr-${VERSION}-macos.dmg"
rm -f "$ZIP" "$DMG"

# ditto preserves bundle attributes/symlinks better than `zip`.
echo "==> zipping -> $ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

# --- rasterize the dmg poster background -> PNGs via resvg -----------------
# Same vector-in-git / pixels-at-bundle-time rule as the icon: the disk
# image's poster art lives as crates/whspr-app/assets/dmg/background.svg and
# only becomes PNGs here, rendered by the *same* resvg + pinned Archivo face
# (so `--skip-system-fonts --use-font-file "$FONT_FILE"` is identical). The
# SVG is authored on the @2x 1440x960 canvas, so we render it 1:1 for the
# Retina `background@2x.png` and at half size for the 1x `background.png`;
# Finder auto-picks the @2x variant from the same folder. The footer's build
# string is a __WHSPR_VERSION__ placeholder we substitute into a temp copy.
DMG_SVG="$REPO_ROOT/crates/whspr-app/assets/dmg/background.svg"
if [ ! -f "$DMG_SVG" ]; then
  echo "error: dmg background source not found at $DMG_SVG" >&2
  exit 1
fi
DMG_SVG_TMP="$WORK_DIR/background.svg"
sed "s/__WHSPR_VERSION__/${VERSION}/g" "$DMG_SVG" > "$DMG_SVG_TMP"

BG_PNG="$WORK_DIR/background.png"
BG_PNG_2X="$WORK_DIR/background@2x.png"
echo "==> rasterizing dmg background -> background.png (720x480) + @2x (1440x960)"
nix shell nixpkgs#resvg --command resvg \
  --skip-system-fonts \
  --use-font-file "$FONT_FILE" \
  -w 1440 -h 960 \
  "$DMG_SVG_TMP" "$BG_PNG_2X"
nix shell nixpkgs#resvg --command resvg \
  --skip-system-fonts \
  --use-font-file "$FONT_FILE" \
  -w 720 -h 480 \
  "$DMG_SVG_TMP" "$BG_PNG"

# --- styled drag-to-install dmg -------------------------------------------
# Build a "poster" disk image: the app and an /Applications alias sit inside
# two outlined wells drawn by the background, an arrow pointing from one to
# the other. Per the design, the volume holds ONLY whspr.app + the alias +
# the hidden .background/ (no README/license/uninstaller), so the arrow has a
# single reading.
#
# The recipe is the classic three-step Finder dance: (1) stage the contents
# and create a *read-write* image sized to fit; (2) attach it and drive Finder
# over AppleScript to set the icon-view layout (window size, 128pt icons, the
# background picture, and each icon's position inside its well), which Finder
# persists into the volume's .DS_Store; (3) detach and `convert` to the final
# compressed read-only .dmg.
echo "==> building styled dmg -> $DMG"

VOLNAME="whspr $VERSION"
STAGE="$WORK_DIR/dmg-root"
rm -rf "$STAGE"
mkdir -p "$STAGE/.background"
# ditto (not cp -R) so the ad-hoc signature + xattrs on the .app survive the
# copy intact; a broken signature would make macOS refuse to launch it.
ditto "$APP" "$STAGE/whspr.app"
ln -s /Applications "$STAGE/Applications"
cp "$BG_PNG" "$STAGE/.background/background.png"
cp "$BG_PNG_2X" "$STAGE/.background/background@2x.png"

# Read-write image, sized to the staged tree + slack for Finder's .DS_Store.
STAGE_KB="$(du -sk "$STAGE" | awk '{print $1}')"
SIZE_MB=$(( STAGE_KB / 1024 + 64 ))
RW_DMG="$WORK_DIR/whspr-rw.dmg"
rm -f "$RW_DMG"
hdiutil create \
  -srcfolder "$STAGE" \
  -volname "$VOLNAME" \
  -fs HFS+ \
  -format UDRW \
  -size "${SIZE_MB}m" \
  -ov "$RW_DMG" >/dev/null

# Detach any stale mount of the same name, then attach fresh read-write.
MOUNT="/Volumes/$VOLNAME"
[ -d "$MOUNT" ] && hdiutil detach "$MOUNT" -force >/dev/null 2>&1 || true
hdiutil attach "$RW_DMG" -readwrite -noautoopen >/dev/null

# Drive Finder to lay out the window. On a headless runner with no Finder
# this errors out fast; we swallow it (|| warn) and still ship a valid -- if
# unstyled -- dmg from the convert below. Coordinates are 1x points in the
# icon view: the wells' centres are (190,317) and (530,317), matching the
# background's drawn wells so the real icons land inside them.
osascript <<APPLESCRIPT || echo "warning: Finder layout skipped (no GUI session?); dmg will be unstyled"
tell application "Finder"
  tell disk "$VOLNAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    -- The path bar (a global View setting some users leave on) otherwise
    -- draws over the poster's footer. The pathbar-visible property is newer
    -- than toolbar/statusbar, so guard it: an older Finder that lacks it must
    -- not abort the whole layout. (No backticks/$() here -- this heredoc is
    -- unquoted for VOLNAME, so the shell would try to run them.)
    try
      set pathbar visible of container window to false
    end try
    set the bounds of container window to {200, 120, 920, 600}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 128
    set background picture of viewOptions to file ".background:background.png"
    set position of item "whspr.app" of container window to {190, 317}
    set position of item "Applications" of container window to {530, 317}
    update without registering applications
    delay 2
    close
  end tell
end tell
APPLESCRIPT

# Trim OS-generated cruft and flag the background folder hidden, so the mounted
# volume presents ONLY whspr.app + the Applications alias (plus the required
# hidden .DS_Store that stores this very layout). .fseventsd/.Trashes are
# recreated by macOS on write; removing them just before detach keeps the
# volume clean for anyone browsing with hidden files shown.
rm -rf "$MOUNT/.fseventsd" "$MOUNT/.Trashes"
chflags -h hidden "$MOUNT/.background" 2>/dev/null || true

# Flush the layout to disk, detach, and compress into the final read-only dmg.
sync
hdiutil detach "$MOUNT" >/dev/null 2>&1 || hdiutil detach "$MOUNT" -force >/dev/null
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG" >/dev/null

echo ""
echo "==> done. Outputs:"
echo "      app: $APP"
echo "      zip: $ZIP"
echo "      dmg: $DMG"
