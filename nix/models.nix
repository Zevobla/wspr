# Reproducible fetch of the whisper.cpp GGML model that WhisperLocal (the
# whisper-rs ASR backend) loads. Pinning it by content hash means
# `nix build .#whisper-model` / `nix develop` always resolve to the exact
# same bytes, so no one needs a manual model download to get a working
# WhisperLocal.
{ pkgs }:

let
  # Pyannote segmentation-3.0 ONNX checkpoint sherpa-onnx's diarizer uses to
  # find speaker turns. Upstream ships it inside a tar.bz2 alongside scripts
  # and docs we don't need; fetchzip unpacks it and strips the single
  # top-level dir, leaving model.onnx (among other files) directly in the
  # output.
  sherpa-segmentation-src = pkgs.fetchzip {
    url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
    hash = "sha256-hqaCTZJKZp6IHxYzgVBd9Bss6wC1qg+edB/v10BT1tA=";
  };

  # wespeaker CAM++ speaker-embedding ONNX checkpoint -- the default choice
  # sherpa-onnx's diarizer uses to tell speakers apart. Upstream's own
  # release tag ("speaker-recongition-models") is misspelled; kept verbatim
  # below since it's their typo, not ours.
  sherpa-embedding-campplus-src = pkgs.fetchurl {
    url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_CAM%2B%2B.onnx";
    hash = "sha256-xG+tELX4HhqkpgwWJxQghXcJNlUHbFRQ+MRp5SLsVO8=";
  };
in

{
  # "base" multilingual model (~148MB) -- the smallest ggml checkpoint that
  # still supports every language, rather than the even-smaller English-only
  # variants.
  whisper-model = pkgs.fetchurl {
    url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
    hash = "sha256-YO1bw90U7qhWST0zQ0m0BXgt3K8AKNS130CINF+6Lv4=";
  };

  # Single directory holding the sherpa-onnx diarization checkpoints,
  # renamed to what the diarize crate expects, so `whspr diarize` finds them
  # reproducibly with no manual download.
  speaker-models = pkgs.runCommand "speaker-models" { } ''
    mkdir -p $out
    cp ${sherpa-segmentation-src}/model.onnx $out/segmentation.onnx
    cp ${sherpa-embedding-campplus-src} $out/embedding-campplus.onnx
  '';
}
