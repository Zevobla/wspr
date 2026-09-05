# Reproducible fetch of the whisper.cpp GGML model that WhisperLocal (the
# whisper-rs ASR backend) loads. Pinning it by content hash means
# `nix build .#whisper-model` / `nix develop` always resolve to the exact
# same bytes, so no one needs a manual model download to get a working
# WhisperLocal.
{ pkgs }:

{
  # "base" multilingual model (~148MB) -- the smallest ggml checkpoint that
  # still supports every language, rather than the even-smaller English-only
  # variants.
  whisper-model = pkgs.fetchurl {
    url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
    hash = "sha256-YO1bw90U7qhWST0zQ0m0BXgt3K8AKNS130CINF+6Lv4=";
  };
}
