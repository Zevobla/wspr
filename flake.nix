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

        commonArgs = {
          inherit src;
          strictDeps = true;
          # The root Cargo.toml is workspace-only (no [package]), so crane
          # can't infer a name/version from it; name the whole-workspace
          # deps build explicitly instead of letting it fall back silently.
          pname = "whspr-workspace";
          version = "0.1.0";

          nativeBuildInputs = [ pkgs.pkg-config ];

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
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        whspr-cli = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "whspr-cli";
          cargoExtraArgs = "-p whspr-cli";
          doCheck = false;
        });
      in
      {
        packages = {
          whspr-cli = whspr-cli;
          default = whspr-cli;
        };

        checks = {
          inherit whspr-cli;

          workspace-test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ whspr-cli ];
          packages = [ toolchain pkgs.pkg-config ];
        };
      });
}
