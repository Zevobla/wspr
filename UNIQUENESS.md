# Uniqueness report — whspr

This document records how whspr differs from existing open-source voice-dictation
tools and from other hackathon submissions, and which parts are our own
implementation versus third-party building blocks (AE cluster).

## Original feature (group AK)

whspr's headline original feature is **speaker fingerprinting / diarization**
(see `README.md` → "Original feature: speaker fingerprinting", crate
`whspr-diarize`). A multi-speaker recording is segmented into speaker turns,
each turn is embedded (CAM++/ERes2Net via sherpa-onnx), and embeddings are
matched by cosine similarity against a persisted, renameable speaker database
(`speakers.json`) that is shared across scans — so a speaker enrolled from one
recording is recognized in a completely different one later. This is not part of
the core dictation loop and is toggleable via `[speaker].enabled`. No other
tracked submission implements this; it is uncontested in the acceptance matrix.

## Own implementation of the core pipeline (AE-06)

The four pipeline stages are our own code behind trait seams in `whspr-core`,
not a wrapper around a single upstream app:

- **Capture / VAD** (`whspr-audio`): silence trimming, device enumeration and
  selection, an energy+hangover VAD, and a preroll ring buffer (`PrerollBuffer`,
  E-10) that retains pre-trigger samples so the first word is never clipped.
- **ASR** (`whspr-asr`): pluggable `AsrBackend` — `WhisperLocal` (whisper-rs),
  `OpenAiAsr`, `DeepgramAsr` — selected at runtime by config, plus a `MockAsr`
  test double.
- **Refinement** (`whspr-refine`): a `NormalizingRefiner` decorator chaining
  rule-based passes (numbers/dates/times, dedup, macros) around an LLM backend
  (`NoopRefiner`, `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`). Includes a
  sandboxed **LuaJIT** scripting layer for macros — a `lua:`-prefixed macro runs
  as a JIT-compiled script, which we have not seen in comparable tools.
- **Injection** (`whspr-inject`): clipboard save/restore paste with a graceful
  fallback to synthetic typing, pre-paste pause, and hotkey debounce.

## Third-party components (AE-04)

Reused libraries are standard, permissively licensed crates opted into per-crate
and pinned in `Cargo.lock`: whisper-rs, llama-cpp-2, sherpa-rs, cpal, rubato,
hound, global-hotkey, enigo, arboard, iced, tray-icon, rodio, mlua (LuaJIT),
serde, tokio, reqwest. Their licenses are permissive (no copyleft in the
dependency graph — criterion Z-08), and model weights are fetched separately and
never vendored into the repository (Z-12). We did not fork or copy another
project's source tree; the architecture (eight-crate workspace, trait-abstracted
per-OS seams, Nix/crane reproducible build) is our own design.

## Divergence from analogs

Unlike a typical single-binary Whisper wrapper, whspr is a Cargo **workspace**
with real crate boundaries so each pluggable piece is independently testable, is
**Nix/crane reproducible** end-to-end (including a hermetic CLI E2E suite in
`nix flake check`), and treats local *and* cloud backends for both ASR and
refinement as a runtime config choice rather than a compile-time decision.
