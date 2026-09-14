{
  description = "whspr - desktop voice dictation";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, fenix, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        toolchain = fenix.packages.${system}.stable.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        # crane's cleanCargoSource keeps only Rust/Cargo files, which drops
        # several kinds of source this build needs, so each is allowed back in:
        #   * crates/whspr-app/assets/icon.svg -- the vector whspr-app's build
        #     script rasterizes into the window icon.
        #   * crates/whspr-asr/src/apple_speech.mm -- the Objective-C++ shim over
        #     Apple's on-device speech recognizer that whspr-asr's build.rs
        #     compiles with `cc` on macOS.
        #   * crates/whspr-refine/src/apple_foundation.swift -- the Swift shim
        #     over Apple's Foundation Models LLM that whspr-refine's build.rs
        #     compiles with swiftc on macOS.
        # Each is read by a build script in the sandboxed crane build (`nix
        # build`/`nix flake check`). The Archivo fonts don't need to be
        # in-source: build.rs reads them from ARCHIVO_DIR (a Nix store path).
        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (lib.hasSuffix ".svg" path)
            || (lib.hasSuffix ".mm" path)
            || (lib.hasSuffix ".swift" path)
            || (craneLib.filterCargoSources path type);
          name = "source";
        };

        # nixpkgs dropped the old per-framework `darwin.apple_sdk.frameworks.*`
        # stubs (https://nixos.org/manual/nixpkgs/stable/#sec-darwin-legacy-frameworks);
        # the whole SDK — headers for every framework we need (AudioUnit,
        # CoreAudio, AppKit, Metal, ...) — now comes from this one derivation.
        darwinFrameworks = lib.optionals pkgs.stdenv.isDarwin [ pkgs.apple-sdk ];

        linuxLibs = lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          alsa-lib
          libxkbcommon
          wayland
          vulkan-loader
          libGL
          # tray-icon's Linux backend (via ksni) draws its status icon
          # through the GTK/appindicator stack, not raw wayland/x11.
          gtk3
          libayatana-appindicator
        ]);

        # whisper-rs (asr) and llama-cpp-2 (refine) compile native C/C++ via
        # cmake and generate bindings via bindgen, which needs cmake/clang as
        # build tools plus a real libclang (and, on darwin, the SDK sysroot
        # so it can find system headers) at bindgen-run time.
        nativeCTools = [ pkgs.cmake pkgs.llvmPackages.clang ];

        libclangPath = "${pkgs.llvmPackages.libclang.lib}/lib";

        bindgenExtraClangArgs = lib.optionalString pkgs.stdenv.isDarwin
          "-isysroot ${pkgs.apple-sdk}/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";

        # The three static Archivo faces whspr-app embeds (see
        # crates/whspr-app/src/theme/fonts.rs and build.rs, which rasterizes
        # the app icon with ExtraBold). Archivo is a general-purpose OFL
        # font, not whspr's own source, so it's pinned by content hash from
        # the designer's upstream repo and fetched hermetically here rather
        # than vendored as .ttf files in git. Bytes verified identical to
        # what was previously vendored.
        archivoRegular = pkgs.fetchurl {
          url = "https://raw.githubusercontent.com/Omnibus-Type/Archivo/master/fonts/ttf/Archivo-Regular.ttf";
          sha256 = "sha256-Wfv9ipu0EwG/6VDnsJf9QtUq8iSgd2HiwbLJWaTJz5o=";
        };
        archivoSemiBold = pkgs.fetchurl {
          url = "https://raw.githubusercontent.com/Omnibus-Type/Archivo/master/fonts/ttf/Archivo-SemiBold.ttf";
          sha256 = "sha256-ZZi27YFhWHZ6djO9+wHXja9UyrTNTZ08wM/ZlfRyMIM=";
        };
        archivoExtraBold = pkgs.fetchurl {
          url = "https://raw.githubusercontent.com/Omnibus-Type/Archivo/master/fonts/ttf/Archivo-ExtraBold.ttf";
          sha256 = "sha256-n/9nkirivDdqnlxtZE7r+MWxf2Kg845fAW5LfjkDZi8=";
        };

        # Single directory holding all three faces under the exact
        # filenames crates/whspr-app/build.rs expects, so ARCHIVO_DIR can
        # point straight at it -- same shape as the model-dir env vars
        # (WHISPER_MODEL_PATH/SPEAKER_MODEL_DIR) used to work before this
        # repo moved models to bring-your-own.
        archivoDir = pkgs.runCommand "archivo-fonts" { } ''
          mkdir -p $out
          cp ${archivoRegular} $out/Archivo-Regular.ttf
          cp ${archivoSemiBold} $out/Archivo-SemiBold.ttf
          cp ${archivoExtraBold} $out/Archivo-ExtraBold.ttf
        '';

        # yt-dlp: nixpkgs lags yt-dlp's release train by months, and an
        # outdated yt-dlp fails on YouTube's current formats + caption lists
        # (the "Requested format is not available" / empty-captions symptom).
        # Pin the official self-contained macOS release (universal2) so dev and
        # the bundled .app both ship a current yt-dlp. Bump version + hash to
        # update -- find the SRI with `nix hash file yt-dlp_macos`.
        ytdlpBundled = pkgs.stdenvNoCC.mkDerivation {
          pname = "yt-dlp";
          version = "2026.08.19";
          src = pkgs.fetchurl {
            url = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_macos";
            hash = "sha256-DxkrfsFHq2KIiF1jUdmrZzZ2QAKbQ3dXbvRt15z3sgI=";
          };
          dontUnpack = true;
          dontFixup = true;
          installPhase = ''
            runHook preInstall
            mkdir -p $out/bin
            cp $src $out/bin/yt-dlp
            chmod +x $out/bin/yt-dlp
            runHook postInstall
          '';
        };

        # Apple's Foundation Models framework (the on-device system LLM) is
        # Swift-only and macOS-26. nixpkgs pins Swift 5.10.1 (all channels),
        # which can't parse its Swift-6 module interface, so we fetch the exact
        # swift.org release toolchain that built it (6.3.2) and unpack it. The
        # component .pkg's Payload is a gzip'd cpio (NOT pbzx). whspr-refine's
        # build.rs compiles the shim with this against `apple-sdk_26`; nothing
        # here touches the main Rust/C build, which stays on `apple-sdk` (14.4).
        # Referenced only through the darwin-only env vars below, so non-darwin
        # evals never realize it.
        swiftToolchain = pkgs.stdenvNoCC.mkDerivation {
          pname = "swift-org-toolchain";
          version = "6.3.2";
          src = pkgs.fetchurl {
            url = "https://download.swift.org/swift-6.3.2-release/xcode/swift-6.3.2-RELEASE/swift-6.3.2-RELEASE-osx.pkg";
            hash = "sha256-Pdjac2MYtvCl18AbA53YUR95lMq6v5sQU03OGxnM3Kc=";
          };
          nativeBuildInputs = [ pkgs.xar pkgs.cpio pkgs.gzip ];
          unpackPhase = ''
            runHook preUnpack
            xar -xf "$src"
            runHook postUnpack
          '';
          installPhase = ''
            runHook preInstall
            mkdir -p "$out"
            gzip -dc swift-6.3.2-RELEASE-osx-package.pkg/Payload | (cd "$out" && cpio -idm)
            runHook postInstall
          '';
          dontFixup = true;
        };

        # The macOS-26 SDK path whspr-refine's Swift shim compiles against.
        macos26Sdk = "${pkgs.apple-sdk_26}/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";

        # Env vars that point whspr-refine's build.rs (and the whspr-app/whspr-cli
        # build scripts) at the Swift toolchain + macOS-26 SDK. Empty off darwin,
        # where the shim compiles out and the refiner is simply unavailable.
        appleFmEnv = lib.optionalAttrs pkgs.stdenv.isDarwin {
          WHSPR_SWIFTC = "${swiftToolchain}/usr/bin/swiftc";
          WHSPR_MACOS26_SDK = macos26Sdk;
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          # The root Cargo.toml is workspace-only (no [package]), so crane
          # can't infer a name/version from it; name the whole-workspace
          # deps build explicitly instead of letting it fall back silently.
          pname = "whspr-workspace";
          version = "0.1.0";

          nativeBuildInputs = [ pkgs.pkg-config ] ++ nativeCTools;

          # The full dependency set the eventual GUI/audio/ASR/refine
          # backends need, so no later team has to touch this file. Only
          # whspr-cli is actually built here today; it doesn't link most of
          # these yet, but declaring them up front keeps this the one place
          # system libs get added.
          buildInputs = [
            pkgs.ffmpeg
            pkgs.whisper-cpp
            pkgs.llama-cpp
          ] ++ linuxLibs ++ darwinFrameworks;

          LIBCLANG_PATH = libclangPath;
          BINDGEN_EXTRA_CLANG_ARGS = bindgenExtraClangArgs;

          # crates/whspr-app/build.rs reads this at `cargo build` time (to
          # copy the three faces into OUT_DIR for `include_bytes!` and to
          # load ExtraBold into usvg's fontdb for icon rasterization), so it
          # has to be visible to the crane build itself, not just devShell.
          ARCHIVO_DIR = "${archivoDir}";
        } // appleFmEnv;

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        whspr-cli = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "whspr-cli";
          cargoExtraArgs = "-p whspr-cli";
          doCheck = false;
          # The crate's [[bin]] is named `whspr`, not `whspr-cli`; without
          # this, `nix run .#whspr-cli` tries to exec a `whspr-cli` binary
          # that doesn't exist.
          meta.mainProgram = "whspr";
        });

        whspr-app = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "whspr-app";
          cargoExtraArgs = "-p whspr-app";
          doCheck = false;
        });
      in
      {
        packages = {
          whspr-cli = whspr-cli;
          whspr-app = whspr-app;
          # Pinned current yt-dlp (see `ytdlpBundled`); the macOS bundle script
          # copies `nix build .#yt-dlp` into the .app's Resources.
          yt-dlp = ytdlpBundled;
          # whspr-app (the iced GUI) is the actual product; whspr-cli stays
          # available as `nix build .#whspr-cli` for the headless binary.
          default = whspr-app;
        };

        checks = {
          inherit whspr-cli whspr-app;

          workspace-test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        devShells.default = pkgs.mkShell ({
          inputsFrom = [ whspr-cli ];
          # inputsFrom only carries over buildInputs/nativeBuildInputs, not
          # arbitrary env vars, so cmake/clang and LIBCLANG_PATH/
          # BINDGEN_EXTRA_CLANG_ARGS are repeated here explicitly.
          # deno is a JavaScript runtime yt-dlp shells out to for YouTube's
          # "nsig" challenge; without one, format URLs are throttled/missing and
          # audio downloads fail ("n challenge solving failed" / "The page needs
          # to be reloaded"). With deno on PATH yt-dlp solves it anonymously, so
          # "Transcribe here" works without borrowed browser cookies. The macOS
          # bundle must ship deno beside yt-dlp for the same reason.
          #
          # typst compiles the note desk's exported `.typ` document to PDF
          # (`typst compile in.typ out.pdf`); the note desk shells out to it,
          # matching the yt-dlp/ffmpeg pattern. The bundle must ship it too.
          packages = [ toolchain pkgs.pkg-config ] ++ nativeCTools
            ++ lib.optionals pkgs.stdenv.isDarwin [ ytdlpBundled pkgs.deno pkgs.typst ];

          LIBCLANG_PATH = libclangPath;
          BINDGEN_EXTRA_CLANG_ARGS = bindgenExtraClangArgs;

          # inputsFrom doesn't carry over commonArgs' env vars either, so
          # ARCHIVO_DIR is repeated here the same way LIBCLANG_PATH/
          # BINDGEN_EXTRA_CLANG_ARGS are above.
          ARCHIVO_DIR = "${archivoDir}";
        } // appleFmEnv);
      });
}
