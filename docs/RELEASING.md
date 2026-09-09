# Releasing whspr

whspr ships as a macOS `.app` bundle (Apple Silicon), packaged as a `.dmg`
and a `.zip`. Releases are cut by pushing a version tag; CI does the rest.

## Cut a release

1. Bump the version in the root `Cargo.toml` (`[workspace.package] version`)
   if needed, and land it on `main`.
2. Tag and push:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

That tag push triggers `.github/workflows/release.yml` on a `macos-14`
(Apple Silicon) runner, which:

1. Installs Nix and runs `nix flake check` (the hermetic test gate).
2. Runs `scripts/bundle-macos.sh --version 0.1.0`, which builds the app
   (`nix build .#whspr-app`), generates `whspr.icns` from
   `crates/whspr-app/assets/icon.svg` at bundle time, assembles
   `whspr.app`, vendors its native dylibs (see below), and produces
   `whspr-0.1.0-macos.dmg` + `whspr-0.1.0-macos.zip`.
3. Creates a GitHub Release for the tag and uploads the `.dmg` and `.zip`.

The tag version (with the leading `v` stripped) becomes
`CFBundleShortVersionString` / `CFBundleVersion` in `Info.plist`.

## Build a bundle locally

```sh
./scripts/bundle-macos.sh                 # version from Cargo.toml
./scripts/bundle-macos.sh --version 0.1.0 # or pass one explicitly
```

Outputs land in `dist/` (git-ignored): `whspr.app`, `whspr-<v>-macos.dmg`,
`whspr-<v>-macos.zip`. Nothing generated (the `.icns`, PNGs, `.app`, `.dmg`,
`.zip`) is ever committed — the icon's only committed form is the vector
`crates/whspr-app/assets/icon.svg`, rasterized at bundle time.

`--binary <path>` reuses a prebuilt binary instead of running `nix build`.

## Self-contained bundle (vendored dylibs)

The `whspr-app` binary links native dynamic libraries that are **not** present
on a clean end-user Mac:

- `libonnxruntime.1.17.1.dylib` and `libsherpa-onnx-c-api.dylib` — the
  ONNX Runtime + sherpa-onnx libs behind speaker diarization
  (`whspr-diarize` → `sherpa-rs`), linked by `@rpath`. Nix's `fixupPhase`
  strips the build-time `LC_RPATH`, so the binary ships with `@rpath` deps
  and **no** rpath to resolve them.
- `libiconv.2.dylib` (which re-exports `libcharset.1.dylib`) — linked by an
  absolute `/nix/store/…` path that only exists on the build machine.

Left alone, the launched `.app` `dyld`-crashes (`Library not loaded`) before a
window ever appears. So after assembling the bundle, `bundle-macos.sh` copies
every non-system dependency into `whspr.app/Contents/Frameworks`, rewrites each
reference to `@rpath/<name>`, adds an `@executable_path/../Frameworks` rpath to
the executable, and ad-hoc re-signs the dylibs and the bundle (rewriting
install-names invalidates any existing signature, which macOS then refuses to
load). The result runs standalone on a machine with no Nix and no dev tools.

The dylibs are sourced at bundle time from the `sherpa-rs` download cache
(`~/Library/Caches/sherpa-rs/…`) and the Nix store the build used — none of
them is committed, matching the "nothing generated is committed" rule above.
The headless gate (`cargo build` / `cargo test` / `nix flake check`) never
launches the GUI, so it does not catch this — hence the vendoring lives in the
bundle step.

## The styled disk image

The `.dmg` opens as a "poster" drag-to-install window rather than a bare list
of files. Its background is a red field with the whspr wordmark, a
`Drag whspr into Applications.` headline, two outlined icon wells joined by an
arrow, and a footer — the same Modernist treatment as the app icon.

Like the icon, the background is **vector-in-git, pixels-at-bundle-time**: the
only committed source is `crates/whspr-app/assets/dmg/background.svg` (authored
on the @2x 1440×960 canvas so the disk image's absolute coordinates map 1:1).
At bundle time `bundle-macos.sh` renders it with the same `resvg` + pinned
Archivo ExtraBold face used for the icon into `background.png` (720×480) and
`background@2x.png` (1440×960) under the scratch dir — no raster image is ever
committed. The footer's build string is a `__WHSPR_VERSION__` placeholder the
script substitutes with the release version before rendering.

The volume deliberately contains **only** `whspr.app`, an `Applications`
symlink, and the hidden `.background/` — no README, license, or uninstaller —
so the arrow reads one way. `bundle-macos.sh` stages those, builds a read-write
image, and drives Finder over AppleScript into icon view (no toolbar/status
bar, a 720×480 window, 128 pt icons, the background picture, and the two icons
positioned at 1x points `(190, 317)` and `(530, 317)` so they land inside the
drawn wells), then detaches and `hdiutil convert`s to the final compressed
`.dmg`. The Finder step is best-effort: on a headless session with no Finder it
is skipped and a valid — if unstyled — `.dmg` still ships.

## The unsigned-app caveat

By default the release is **unsigned and un-notarized**. macOS Gatekeeper
will refuse to open it on first launch ("whspr can't be opened because Apple
cannot check it for malicious software"). Users work around it once, either
way:

- Right-click **whspr.app** → **Open**, then confirm; or
- clear the quarantine flag:

  ```sh
  xattr -dr com.apple.quarantine /Applications/whspr.app
  ```

The release notes generated by CI include this instruction automatically
whenever the build is unsigned.

## Adding signing + notarization later

The release workflow already contains an optional sign-and-notarize step. It
runs **only** when the signing secrets are present (it gates on
`env.MACOS_CERTIFICATE != ''`); until then, releases ship unsigned and CI
does not fail.

To enable it, add these repository secrets (Settings → Secrets and variables
→ Actions):

| Secret                  | What it is                                                        |
| ----------------------- | ----------------------------------------------------------------- |
| `MACOS_CERTIFICATE`      | base64 of your Developer ID Application `.p12` (`base64 -i cert.p12`) |
| `MACOS_CERTIFICATE_PWD`  | password for that `.p12`                                          |
| `MACOS_SIGN_IDENTITY`    | e.g. `Developer ID Application: Your Name (TEAMID)`               |
| `MACOS_NOTARY_APPLE_ID`  | Apple ID email used for notarization                             |
| `MACOS_NOTARY_TEAM_ID`   | your Apple Developer Team ID                                     |
| `MACOS_NOTARY_PWD`       | an app-specific password for notarytool                          |

With those set, the next tag push imports the certificate into a temporary
keychain, `codesign --deep --options runtime` the app, re-packages it,
submits the `.dmg` to `xcrun notarytool`, and staples the ticket. The release
notes then drop the unsigned caveat.

> Accessibility and Input Monitoring (for the global hotkey and text
> injection) are **not** declarable in `Info.plist` — macOS prompts the user
> for them at runtime in System Settings → Privacy & Security. Microphone
> and Apple Events usage strings are declared in the bundle's `Info.plist`.
