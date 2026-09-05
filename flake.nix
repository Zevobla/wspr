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

        src = craneLib.cleanCargoSource ./.;

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
        ]);

        # whisper-rs (asr) and llama-cpp-2 (refine) compile native C/C++ via
        # cmake and generate bindings via bindgen, which needs cmake/clang as
        # build tools plus a real libclang (and, on darwin, the SDK sysroot
        # so it can find system headers) at bindgen-run time.
        nativeCTools = [ pkgs.cmake pkgs.llvmPackages.clang ];

        libclangPath = "${pkgs.llvmPackages.libclang.lib}/lib";

        bindgenExtraClangArgs = lib.optionalString pkgs.stdenv.isDarwin
          "-isysroot ${pkgs.apple-sdk}/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";

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
        };

        inherit (import ./nix/models.nix { inherit pkgs; }) whisper-model speaker-models;

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
          whisper-model = whisper-model;
          speaker-models = speaker-models;
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

        devShells.default = pkgs.mkShell {
          inputsFrom = [ whspr-cli ];
          # inputsFrom only carries over buildInputs/nativeBuildInputs, not
          # arbitrary env vars, so cmake/clang and LIBCLANG_PATH/
          # BINDGEN_EXTRA_CLANG_ARGS are repeated here explicitly.
          packages = [ toolchain pkgs.pkg-config ] ++ nativeCTools;

          LIBCLANG_PATH = libclangPath;
          BINDGEN_EXTRA_CLANG_ARGS = bindgenExtraClangArgs;

          # Points WhisperLocal at the pinned, reproducibly-fetched ggml
          # model (see nix/models.nix) so it has a working model without
          # anyone downloading one by hand.
          WHISPER_MODEL_PATH = "${whisper-model}";

          # Points the diarizer at the pinned sherpa-onnx checkpoints (see
          # nix/models.nix), a directory containing segmentation.onnx and
          # embedding.onnx, so `whspr diarize` works with no manual download.
          SPEAKER_MODEL_DIR = "${speaker-models}";
        };
      });
}
