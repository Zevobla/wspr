# whspr

A desktop voice-dictation app — a functional [Wispr Flow](https://wispr.ai)
clone: hold a hotkey, speak, and clean, LLM-refined text lands wherever your
cursor is.

**License:** [Apache-2.0](LICENSE)

## Status

whspr is early and under active development. This README describes exactly
what's real today versus what's designed but not yet implemented — nothing
below is aspirational unless it's explicitly marked "planned."

**Works today**

- A compiling, tested 15-crate Cargo workspace (`cargo build --workspace` /
  `cargo test --workspace`, all green; `cargo run -p whspr-check` is the
  automated acceptance gate all of this is held to).
- `whspr-core`: the domain types, the five backend traits (`AsrBackend`,
  `TextRefiner`, `HotkeyListener`, `TextSink`, `Diarizer`), and the
  `Pipeline` orchestrator (transcribe → refine → inject) are real, with
  passing unit tests.
- `whspr-cli` (the `whspr` binary): `whspr transcribe <FILE>` runs the
  **real** `Pipeline` end-to-end against a **real** ASR backend — the
  no-flag default is `WhisperLocal` (local whisper.cpp via `whisper-rs`),
  which reads `[whisper].model_path` (or the `WHISPER_MODEL_PATH`
  environment variable) for a GGML model file you've downloaded — whspr is
  bring-your-own-model and never ships or fetches one for you.
  `--asr openai` / `--asr deepgram` / `--asr apple-speech` (macOS on-device)
  select cloud/on-device backends instead; `--asr mock` is an explicit,
  documented opt-in to a canned transcript, used by the test suite and
  `whspr-check` so neither needs a real model file on disk. `--refine`
  works the same way (`noop` / `openai` / `anthropic` / `llama-local` /
  `apple-foundation`). Also real: `transcribe-batch` (a directory of .wav
  files), `diarize` (speaker fingerprinting, see below), `stats`
  (per-utterance history), `uninstall`, and SRT/VTT subtitle export
  (`--format srt|vtt`; see `crates/whspr-cli/src/subtitles.rs`).
- `whspr-config`: `Config` loads from `config.toml` in the platform config
  directory (e.g. `~/.config/whspr/config.toml` on Linux), overlaid on
  compiled-in defaults, and writes those defaults out on first run so
  there's a real, editable file waiting for the user. The `Config` struct
  itself has no environment-variable override, by design — the config
  file is the only way to change a *setting*. (A few backends separately
  fall back to a specific env var when their own config field is unset —
  `WHISPER_MODEL_PATH`, `SPEAKER_MODEL_DIR` — see Settings below.)
  Secrets are the exception to plaintext: on macOS and Windows the app
  moves saved API keys and the HuggingFace token into the OS keychain
  (Keychain / Credential Manager), and both the app and the CLI read them
  from there first (criterion P-06). On Linux the only compiled-in
  keychain backend forgets entries on reboot, so secrets stay in
  `config.toml` there.
- `whspr-asr`: `WhisperLocal`, `OpenAiAsr`, `DeepgramAsr`, and (macOS only)
  `AppleSpeech` (the OS's on-device `SFSpeechRecognizer`) are real, tested
  `AsrBackend` implementations.
- `whspr-refine`: `NoopRefiner` (pass text through unchanged, the default),
  `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal` (local llama.cpp via
  `llama-cpp-2`), and (macOS 26+ with Apple Intelligence, when built with
  the shim) `AppleFoundation` are all real, tested `TextRefiner`
  implementations. Every choice is wrapped in a `NormalizingRefiner`
  decorator that layers rule-based number/date/time normalization, a
  macro/dictionary substitution table (including a sandboxed LuaJIT
  scripting layer for `lua:`-prefixed macros), dedup, and paragraph breaks
  on top.
- `whspr-audio`: mic capture on the selected device (multi-channel input
  downmixed to mono), WAV decoding, resampling to 16kHz mono, input gain,
  a light noise-reduction chain (80 Hz high-pass plus a noise gate that
  only engages when the clip has real silence), RMS-energy silence
  trimming, a preroll monitor that keeps the last moments before the
  hotkey (E-10), Bluetooth/virtual device filters, and polling-based
  device hotplug detection. All real and exercised by the live hotkey
  path.
- `whspr-inject`: the global hotkey listener and text injection
  (clipboard-paste-first, with a synthetic-typing fallback, debounce, and
  clipboard save/restore) are real `HotkeyListener`/`TextSink`
  implementations. On macOS it also asks the Accessibility API whether
  the focused element is editable, so text isn't pasted into a button or
  list.
- `whspr-diarize` + speaker fingerprinting: a real, tested, bring-your-own-
  model diarization backend — see "Original feature: speaker
  fingerprinting" below.
- `whspr-app`: a real, ~14,000-line `iced` 0.14 desktop GUI — not a
  placeholder. The Hub has Dictate / History / Speakers / Models
  (HuggingFace download via `whspr-hf`) / Settings / Note desk / link
  import (`whspr-import`: yt-dlp + ffmpeg) screens, a system tray
  (macOS/Windows), a background worker running the real hotkey → mic
  capture → `Pipeline` → inject loop, hotkey rebinding, close-to-tray,
  sound cues, log rotation, and a headless screenshot dev path.
- `whspr-hf`: the in-app HuggingFace client (browser OAuth sign-in, browse
  a curated whisper.cpp model set with a "fits your machine" badge,
  download, list installed) the Models tab uses to point
  `whisper.model_path` at a real file without hand-editing config.
- `whspr-import`: media-import orchestration — shells out to `yt-dlp`/
  `ffmpeg` to pull published captions instantly, or download audio and
  transcribe it, from a URL (a lecture, podcast, or video).
- `whspr-typst`: a real, tested in-process Typst compiler (with bundled
  base fonts, so it works offline) and notes template. The Hub's Note desk
  uses it for "Export PDF": the desk's `.typ` source is compiled to PDF
  inside the app, with no external `typst` binary.
- `whspr-setup`: a standalone Windows installer — a real `iced` GUI
  (Install → Installing → Done, with a Failure state) that performs a
  genuine per-user install (copies an embedded app payload into
  `%LOCALAPPDATA%\whspr`, creates Start-menu/Desktop shortcuts, writes the
  autostart registry value). It also builds and renders on macOS (a no-op
  install path) so the shared UI stays testable everywhere. See
  `docs/RELEASING.md` for how (and whether) it ships today.
- `whspr-bench`: a CLI that benchmarks ASR backends (e.g. `WhisperLocal`
  vs `MockAsr`) over a set of audio fixtures.
- `whspr-check`: an independent acceptance checker
  (`cargo run -p whspr-check`) that scores this repo against a curated
  subset of the project's acceptance criteria — the gate every claim in
  this README is held to. It only ever reports PASS for something it
  actually verified.
- A Nix flake: `nix develop` gives a working dev shell (Rust toolchain,
  ffmpeg, whisper.cpp, llama.cpp, cmake/clang + libclang for bindgen);
  `nix build` builds the `whspr` binary; `nix flake check` builds it in
  release mode and runs the whole test suite inside the sandbox.

**Planned / not yet implemented**

- Focused-field detection and active-window context on Windows and
  Linux: both are macOS-only today and fall back to the normal behavior
  (paste, no app context) elsewhere.
- History encryption and keychain-held secrets on Linux: the compiled-in
  keychain backend there doesn't survive a reboot, so both stay off
  rather than risk losing keys or making history unreadable.
- A rendered Typst preview in the Note desk: `whspr-typst` can compile an
  SVG preview, but the desk's preview pane is native `iced` widgets and has
  no image surface to show it yet.
- Linux system tray (the tray module is implemented for macOS/Windows
  only; see `crates/whspr-app/src/tray.rs`'s module doc for why).
- Signed/notarized macOS releases — the release workflow supports it, but
  ships **unsigned** until signing secrets are configured (see
  `docs/RELEASING.md`).
- Windows CI (`.github/workflows/windows.yml`) and both Windows release
  jobs are authored but **dormant** — they can't run until GitHub Actions
  billing is restored on this repo (see `docs/RELEASING.md`).
- Speaker diarization on aarch64-windows: `sherpa-rs` ships no prebuilt
  native library there, so `SherpaDiarizer::new` returns an honest
  "unavailable" error on that target instead of degrading silently.

## Architecture

The runtime pipeline is four stages, each behind its own crate and trait:

```
capture (whspr-audio)  →  ASR (whspr-asr)  →  refine / LLM (whspr-refine)  →  inject (whspr-inject)
   mic → 16kHz f32 PCM     AsrBackend trait     TextRefiner trait              TextSink trait
   real                     real backends        real backends                real
```

`whspr-core::Pipeline` owns this flow today end to end: `transcribe()` →
`refine()` → optionally `sink.insert()`, reporting `PipelineState`
transitions (`Idle` / `Recording` / `Transcribing` / `Refining` /
`Injecting` / `Error`) as it goes. `whspr-cli` and `whspr-app`'s worker
both wire real capture/ASR/refine/inject implementations into it. A fifth,
independent trait (`Diarizer`) and crate (`whspr-diarize`) power the
separate speaker-fingerprinting feature described below — `Pipeline` never
touches either.

The workspace is 15 crates:

| Crate | Role | Status |
|---|---|---|
| `whspr-core` | Domain types, the 5 traits (`AsrBackend`, `TextRefiner`, `HotkeyListener`, `TextSink`, `Diarizer`), `Pipeline` orchestrator | Real, tested |
| `whspr-asr` | ASR backends: `WhisperLocal`, `OpenAiAsr`, `DeepgramAsr`, `AppleSpeech` | Real, tested |
| `whspr-refine` | Refine backends: `NoopRefiner`, `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`, `AppleFoundation` + `NormalizingRefiner` rule-based decorator | Real, tested |
| `whspr-audio` | Capture / WAV decode / resample to 16kHz mono / silence trim / preroll buffer | Real, tested (preroll not wired into live capture yet) |
| `whspr-inject` | Global hotkey listener + text injection (clipboard-paste-first, typing fallback) | Real, tested |
| `whspr-diarize` | Speaker-turn segmentation + embedding extraction (sherpa-onnx) | Real, tested; no prebuilt native lib on aarch64-windows |
| `whspr-config` | `Config` + every settings section, TOML file load/save | Real, tested |
| `whspr-hf` | In-app HuggingFace model browse/download client | Real, tested |
| `whspr-import` | yt-dlp/ffmpeg media import (captions + audio) | Real, tested |
| `whspr-typst` | In-process Typst compiler + notes template → SVG/PDF | Real, tested; powers the Note desk's PDF export |
| `whspr-app` | Desktop GUI (`iced`): Hub, tray, background worker | Real, ~14k lines |
| `whspr-cli` | CLI binary (`whspr`): transcribe / transcribe-batch / diarize / stats / uninstall | Real, tested end-to-end |
| `whspr-setup` | Windows installer GUI (`iced`) + embedded payload | Real, tested; not yet wired into release CI (see `docs/RELEASING.md`) |
| `whspr-bench` | ASR backend benchmarking CLI | Real |
| `whspr-check` | Automated acceptance checker (`cargo run -p whspr-check`) | Real |

## Swapping models / backends (local ↔ cloud)

whspr never hard-codes a specific model or vendor. Two traits in
`whspr-core` define the seam every backend implements:

```rust
#[async_trait]
trait AsrBackend: Send + Sync {
    async fn transcribe(&self, audio: &AudioBuffer, opts: &AsrOptions) -> Result<Transcript>;
    fn id(&self) -> &'static str;
}

#[async_trait]
trait TextRefiner: Send + Sync {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String>;
    fn id(&self) -> &'static str;
}
```

`Pipeline` is constructed with `Box<dyn AsrBackend>` and `Box<dyn
TextRefiner>` — it has no idea whether it's talking to a local whisper.cpp
model, a cloud API, or (in tests) a canned mock. Swapping local ↔ cloud is
a matter of constructing it with a different concrete type; `whspr-cli`'s
`--asr`/`--refine` flags (and `whspr-config`'s `AsrChoice`/`RefineChoice`
config defaults, which `whspr-app`'s Settings screen writes to the same
fields) select the concrete type at runtime, e.g.:

```rust
// fully local (real today, once [whisper].model_path / [refine_settings].llama_model_path point at downloaded models)
Pipeline::new(Box::new(WhisperLocal::new(model_path)), Box::new(LlamaLocal::new(llm_path)));

// fully cloud (real today, given [api_keys].openai in config)
Pipeline::new(Box::new(OpenAiAsr::new(api_key)), Box::new(OpenAiRefiner::new(api_key, model)));

// deterministic offline stand-in: what whspr-cli builds for --asr mock (used by tests / whspr-check)
Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));
```

## Build & run

Requires either [Nix](https://nixos.org) with flakes enabled, or a Rust
1.97+ (edition 2021) toolchain installed manually.

**One command (Nix):**

```sh
nix build          # builds ./result/bin/whspr
./result/bin/whspr --version
```

**Step by step:**

```sh
nix develop                                        # dev shell: Rust toolchain + ffmpeg/whisper.cpp/llama.cpp/cmake/clang
cargo build --workspace                             # builds all 15 crates
cargo test --workspace                              # pipeline + config + CLI/diarize e2e tests
cargo run -p whspr-check                            # the acceptance gate (should report 0 fail)
cargo run -p whspr-cli -- --version                 # -> whspr 0.1.0
cargo run -p whspr-cli -- transcribe path/to/file.wav --asr mock  # runs the real pipeline end-to-end; --asr mock needs no model file configured
nix flake check                                     # builds whspr-cli (release) + runs the full test suite in a sandbox
```

Every command above was run and verified on aarch64-darwin before writing
this README.

## Dependencies

If you use `nix develop` / `nix build`, all of this is handled for you —
listed here so you know what's actually needed if you set up a toolchain by
hand:

- Rust (edition 2021; pinned via [fenix](https://github.com/nix-community/fenix) in the flake)
- `ffmpeg` — shelled out to by `whspr-import`'s media-download path (alongside `yt-dlp`)
- `cmake`, `clang`/`libclang` — build whisper.cpp/llama.cpp's native C/C++ trees and generate their bindgen bindings
- whisper.cpp / llama.cpp / sherpa-onnx — linked via `whisper-rs`, `llama-cpp-2`, and `sherpa-rs` respectively (`whspr-asr`, `whspr-refine`, `whspr-diarize`); sherpa-onnx ships no prebuilt native library on aarch64-windows
- On Linux: `alsa-lib`, `libxkbcommon`, `wayland`, `vulkan-loader`, `libGL`
  (for audio capture and the `iced` GUI)
- On macOS: the unified `apple-sdk` package (AudioUnit, CoreAudio, AppKit,
  Metal, etc.)

## Settings

`whspr-config::Config` loads from `config.toml` in the platform config
directory (e.g. `~/.config/whspr/config.toml` on Linux,
`~/Library/Application Support/whspr/config.toml` on macOS), overlaid on
the compiled-in defaults below — the file is written with these defaults
on first run. The `Config` struct itself has no environment-variable
override, by design — the config file is the only way to change a
*setting*. Separately, a couple of backends fall back to a specific env
var only when their own config field is unset: `WHISPER_MODEL_PATH`
(`[whisper].model_path`) and `SPEAKER_MODEL_DIR` (`[speaker].model_dir`);
`WHSPR_DIARIZE_MOCK` is a test-only hook, not a setting at all (see
"Offline by default" below).

The **Status** column is load-bearing: "wired" means changing the value
(in the Hub or the config file) visibly changes behavior, verified by
finding the non-config, non-Settings-screen code that reads it. "wired
(macOS)" or "wired (macOS/Windows)" means it only has an effect on those
platforms.

### Top-level

| Key | Default | Status | Meaning |
|---|---|---|---|
| `asr` | `whisper-local` | wired | Default ASR backend (`whisper-local`\|`open-ai`\|`deepgram`\|`apple-speech`\|`mock`); overridden per-run by `--asr`. |
| `refine` | `noop` | wired | Default refiner (`noop`\|`open-ai`\|`anthropic`\|`llama-local`\|`apple-foundation`); overridden per-run by `--refine`. |
| `language` | `None` | wired | BCP47 language hint for ASR; overridden per-run by `--language`. |
| `hotkey` | `None` (platform default) | wired | Push-to-talk hotkey combo label (e.g. `"Ctrl+Shift+D"`). |
| `api_keys` | `{}` | wired | `[api_keys]` table: cloud backend id -> API key. Plaintext fallback only: on macOS/Windows the app migrates entries into the OS keychain and blanks them here. |

### `[whisper]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `whisper.model_path` | `None` | wired | Path to a GGML model file for `WhisperLocal`. |

### `[speaker]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `speaker.enabled` | `true` | wired | Turns the whole diarization feature on/off. |
| `speaker.model_dir` | `None` | wired | Directory of sherpa-onnx model files for `whspr diarize`. |
| `speaker.similarity_threshold` | `0.7` | wired | Minimum cosine similarity to match an already-enrolled speaker. |
| `speaker.embedding_model` | `cam-plus-plus` | wired | Embedding model choice (`cam-plus-plus`\|`eres2net`). |

### `[normalize]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `normalize.numbers` | `true` | wired | Spell-out numbers -> digits. |
| `normalize.dates` | `true` | wired | Normalize recognized dates to `YYYY-MM-DD`. |
| `normalize.times` | `true` | wired | Normalize recognized times to 24-hour `HH:MM`. |
| `normalize.numbers_format` | `digits` | wired | How normalized numbers/dates/times render. |
| `normalize.macros` | `{}` | wired | Trigger phrase -> expansion (a `lua:`-prefixed value runs as a sandboxed LuaJIT script). |
| `normalize.paragraph_break` | `true` | wired | Insert paragraph breaks on long pauses. |
| `normalize.punctuation_toggle` | `true` | wired | Auto-punctuation cleanup. |
| `normalize.dictionary` | `{}` | wired | Trigger term -> replacement (finer-grained than `macros`). |
| `normalize.formulas` | `true` | wired | Recognize spoken arithmetic/symbols and rewrite them symbolically. |

### `[language-settings]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `language_settings.language_switch` | `true` | wired | Auto-detect the recognition language per utterance. |
| `language_settings.fixed_language` | `None` | wired | Fixed language code used when `language_switch` is `false`. |

### `[device]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `device.input_device` | `None` | wired | Selected input device name (`None` = host default), used by both the hotkey and the Record button. If it disappears, capture falls back to the default mic and the Hub shows a notice. |
| `device.device_hotplug` | `true` | wired | Polls the input-device list every 2 s to refresh Settings and catch a disconnected device. |
| `device.active_window` | `true` | wired (macOS) | Passes the frontmost app's name to the refiner as context. |
| `device.bluetooth_source` | `true` | wired | Lists Bluetooth inputs (AirPods, headsets) in the device picker. |
| `device.virtual_source` | `true` | wired | Lists virtual/loopback inputs (BlackHole, Loopback, aggregate devices) in the device picker. |
| `device.tray_static` | `false` | wired | Keeps the tray on its idle icon instead of showing recording/done states. |

### `[autostart]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `autostart.enabled` | `false` | wired | Launch-at-login; toggling it writes/removes a real OS autostart entry. |

### `[sound]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `sound.enabled` | `true` | wired | Start/stop sound cues. |

### `[injection]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `injection.pre_paste_delay_ms` | `0` | wired | Pause before the paste keystroke, for slow-to-focus target apps. |

### `[privacy]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `privacy.mic_privacy` | `true` | wired | On: the mic is open only while the hotkey is held. Off: an idle preroll stream keeps the moment before the press, so the first word isn't clipped. |
| `privacy.history_encryption` | `false` | wired (macOS/Windows) | Encrypts `history.jsonl` lines with ChaCha20-Poly1305, key in the OS keychain; toggling converts the existing file. The CLI reads and writes the same format. |
| `privacy.cookies_browser` | `None` | wired | Browser whose cookies media import may borrow for gated videos. |

### `[capture]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `capture.refine_timeout_ms` | `30000` | wired | Timeout for the refine step. |
| `capture.auto_send` | `false` | wired | During a long hold, a 1.5 s pause in speech inserts what was said so far and keeps recording. |
| `capture.input_field_detection` | `true` | wired (macOS) | If the focused element is clearly not editable, the text goes to the clipboard with a notice instead of being pasted. Needs Accessibility permission. |
| `capture.noise_suppression` | `false` | wired | 80 Hz high-pass plus a noise gate on the captured clip (no ML, no spectral subtraction). |
| `capture.input_gain` | `1.0` | wired | Linear gain on the captured clip, clamped to full scale. |
| `capture.vad_threshold` | `0.01` | wired | RMS level for trimming leading/trailing silence (app and CLI) and for `auto_send`'s pause detection. |
| `capture.translate` | `false` | wired | Translate transcribed text via the ASR backend. |
| `capture.shorten` | `false` | wired | Drops parenthetical fillers ("ну,", "like,", "sort of") and stutters, and asks LLM refiners to be concise; `--shorten` overrides it in the CLI. Hesitations like "um"/"эээ" are removed regardless. |

### `[huggingface]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `huggingface.token` | `None` | wired | OAuth access token for HuggingFace downloads. Plaintext fallback only: moved into the OS keychain on macOS/Windows. |
| `huggingface.models_dir` | `None` | wired | Directory downloaded model files are placed in. |
| `huggingface.model_dirs` | `[]` | wired | Extra directories scanned for already-installed model files. |
| `huggingface.oauth_client_id` | `None` | wired | OAuth app client id for HuggingFace sign-in. |

### `[refine_settings]`

| Key | Default | Status | Meaning |
|---|---|---|---|
| `refine_settings.openai_model` | `gpt-4o-mini` | wired | Model id for `OpenAiRefiner`. |
| `refine_settings.anthropic_model` | `claude-3-5-sonnet-20241022` | wired | Model id for `AnthropicRefiner`. |
| `refine_settings.llama_model_path` | `None` | wired | GGUF model path for `LlamaLocal`. |
| `refine_settings.instructions` | `None` | wired | Extra cleanup instructions layered onto the refiners' shared defaults. |

Every setting above is now read somewhere; the few marked with a platform
work only there (see Planned).

## Original feature: speaker fingerprinting

**whspr can tell *who* is speaking, not just *what* was said.** Point it at a
recording with more than one voice and it segments the audio into speaker
turns (via a real speaker-change segmentation model, not naive VAD
chunking), extracts one embedding vector per turn, and matches each
embedding against a persisted, cross-recording database of enrolled
speakers by cosine similarity — reusing an existing speaker's identity if
a turn's voice matches closely enough, or enrolling a brand-new speaker if
it doesn't. The database (`speakers.json`) is rewritten, not appended, on
every scan, so a speaker enrolled from one recording is recognized again
in a completely different one later. The GUI (Hub → Speakers) exposes this
as a "Diarize a recording..." file picker, a menu to choose which
speaker-embedding model to use, and an editable, renameable list of every
speaker enrolled so far, each showing how many scans it's appeared in.
This is squarely beyond what a dictation tool does: `whspr transcribe`
answers "what was said"; `whspr diarize` answers "who said it, and have we
heard this voice before" — speaker identity that a multi-person meeting or
interview recording needs and a single-speaker dictation pipeline has no
reason to build.

### Try it

```sh
whspr diarize <FILE> [--model-dir <DIR>] [--embedding cam-plus-plus|eres2net] [--language <LANG>] [--json]
```

- `<FILE>` — a WAV file (mono or multi-channel; it's decoded and downmixed/resampled to 16kHz mono like every other whspr audio input).
- `--embedding` — which speaker-embedding model to load: `cam-plus-plus` (WeSpeaker CAM++, the default) or `eres2net` (3D-Speaker ERes2Net). Falls back to the config file's `[speaker].embedding-model` if omitted.
- `--model-dir` — directory containing the sherpa-onnx model files (see "Offline by default" below). Falls back to `[speaker].model-dir` in the config file, then to the `SPEAKER_MODEL_DIR` environment variable.
- `--language` — a BCP47 hint, accepted for consistency with `transcribe`; diarization's segmentation/embedding models are acoustic, not text-based, so this isn't consumed yet — it's plumbed through for a future word-level who-said-what alignment.
- `--json` — print a JSON array of `{start_secs, end_secs, speaker, score}` instead of plain-text `[start-end] SpeakerN` lines.

Every run matches its turns against `speakers.json` in the platform data
directory (`~/Library/Application Support/whspr` on macOS,
`~/.local/share/whspr` on Linux) and rewrites it, so identity persists
*across* invocations, not just within one scan. With no model directory
resolvable, `whspr diarize` refuses rather than guessing — this is what a
real, unconfigured run looks like today:

```
$ whspr diarize a.wav --json --data-dir /tmp/demo
Error: no speaker model available: speaker diarization needs a model directory. Pass --model-dir, set [speaker].model_dir in the config, or set the SPEAKER_MODEL_DIR environment variable, then try again.
```

To exercise the persisted match-or-enroll database logic without
downloading model files, the deterministic test suite (and this demo) use
the explicit `WHSPR_DIARIZE_MOCK=1` opt-in — never implicit, and never
silent (see "Offline by default" below). Run against two different files
sharing a `--data-dir`, actually run today:

```
$ WHSPR_DIARIZE_MOCK=1 whspr diarize a.wav --json --data-dir /tmp/demo
WARNING: WHSPR_DIARIZE_MOCK is set -- using a synthetic mock diarizer. Speaker turns are FABRICATED, not real. This is a test/development affordance; unset it for real diarization.
[{"end_secs":2.5,"score":0.949999988079071,"speaker":"463b7575-d444-46b1-8090-e9ec674ced12","start_secs":0.0},{"end_secs":5.0,"score":0.9200000166893005,"speaker":"1b876984-1a48-41b3-8fba-44dfc84d6610","start_secs":2.5}]
$ WHSPR_DIARIZE_MOCK=1 whspr diarize b.wav --json --data-dir /tmp/demo
WARNING: WHSPR_DIARIZE_MOCK is set -- using a synthetic mock diarizer. Speaker turns are FABRICATED, not real. This is a test/development affordance; unset it for real diarization.
[{"end_secs":2.5,"score":0.949999988079071,"speaker":"463b7575-d444-46b1-8090-e9ec674ced12","start_secs":0.0},{"end_secs":5.0,"score":0.9200000166893005,"speaker":"1b876984-1a48-41b3-8fba-44dfc84d6610","start_secs":2.5}]
```

Both runs assign the same two speaker ids (v4 UUIDs, minted at first
enrollment — not sequential "Speaker N" labels) and `speakers.json` ends
up with both files' paths recorded under each profile's `scans` list — the
persisted match-or-enroll logic is real; only the *voices* in this demo
are fabricated. This exact scenario (two files, one data dir, matching ids
across runs) is also covered by an automated test:
`diarize_persists_speaker_matches_across_runs` in
`crates/whspr-cli/tests/diarize_e2e.rs`.

### Offline by default

`whspr diarize` never touches the network, and it never fabricates results
silently. With no model directory resolvable (from `--model-dir`,
`[speaker].model-dir`, or the `SPEAKER_MODEL_DIR` environment variable), it
refuses with an error (see above) instead of falling back to a fake
backend. The GUI behaves the same way: Hub → Speakers shows a "needs a
speaker model" prompt (`state.needs_speaker_model`, driven by
`whspr-app/src/speakers.rs`'s `run_diarize_scan`) rather than enrolling
fabricated speakers. The *only* way to get synthetic output, in the CLI or
the tests, is the explicit `WHSPR_DIARIZE_MOCK=1` environment variable — it
exists solely for the deterministic test suite and prints the loud
FABRICATED warning shown above on every use; a real user never sets it.

For real acoustic diarization, whspr is bring-your-own-model: it doesn't
fetch or ship these checkpoints for you. Download `segmentation.onnx`
plus whichever embedding checkpoint(s) you want into one directory —
`crates/whspr-diarize/src/lib.rs`'s crate-level doc comment has the exact
upstream URLs and required filenames — then point `SPEAKER_MODEL_DIR` (or
`--model-dir`) at it. Feed it a real multi-speaker recording (not a
silent test fixture — the real segmentation model finds zero turns in
silence, which is why the demo above deliberately uses the mock opt-in) to
see genuine speaker-turn segmentation and voice-based re-identification.
On aarch64-windows, `sherpa-rs` ships no prebuilt native library at all,
so `SherpaDiarizer::new` returns an honest "unavailable" error on that
target too, rather than silently degrading.

### Tests

- `crates/whspr-diarize/src/lib.rs` — unit tests for `SherpaDiarizer`'s model-dir resolution precedence, segment-range clamping, and missing-model-file error paths.
- `crates/whspr-config/src/speaker/mod.rs` — unit tests for `SpeakerDb::match_or_enroll` (UUID assignment, matching an existing speaker, an orthogonal embedding creating a new speaker), `rename`, and save/load round-tripping.
- `crates/whspr-cli/tests/diarize_e2e.rs` — `assert_cmd`-driven end-to-end tests: mock-backend labeling (`diarize_with_mock_backend_prints_speaker_labeled_turns`), cross-run persistence (`diarize_persists_speaker_matches_across_runs`), nonexistent-model-dir/unknown-embedding error paths (`diarize_with_nonexistent_model_dir_fails_with_clear_error`, `diarize_with_unknown_embedding_choice_fails_with_clear_error`), and the `SPEAKER_MODEL_DIR` fallback (`diarize_falls_back_to_speaker_model_dir_env_var`).
- `crates/whspr-app/src/speakers.rs` — tests covering `run_diarize_scan`'s refusal with no model available (`run_diarize_scan_refuses_without_a_model`) and when the feature is disabled (`run_diarize_scan_refuses_when_disabled`), for the GUI's background diarization task.
- `whspr_core::testkit::MockDiarizer` — the shared, deterministic double all of the above (and the `WHSPR_DIARIZE_MOCK` demo above) run against; two orthogonal canned embeddings so matching-vs-enrolling is exercised meaningfully with no real model files.

All of the above run under `cargo test --workspace` and stay green with no model files, no network, and no GPU.

### Config toggle

`whspr-config::SpeakerSettings` has an `enabled: bool` field (default
`true`) that turns the whole feature off. Both entry points check it
before doing anything else: `whspr-cli`'s `diarize_cmd::run` bails out
with a clear, non-zero-exit error ("speaker fingerprinting is disabled
([speaker].enabled = false in config); enable it to run `whspr
diarize`") before decoding any audio, and `whspr-app`'s
`speakers::run_diarize_scan` refuses the same way before ever spawning
the blocking diarization task, surfacing the failure through the Hub's
existing "Diarization failed: ..." status line. Setting `enabled =
false` in `config.toml` genuinely disables `whspr diarize` end to end,
in both the CLI and the GUI — covered by a dedicated unit test in each
crate (`diarize_cmd::tests::run_refuses_when_speaker_disabled`,
`speakers::tests::run_diarize_scan_refuses_when_disabled`).

### Doesn't touch base dictation

The feature is also structurally isolated, independent of that toggle:
`whspr transcribe` builds its `Pipeline` from an `AsrBackend` and a
`TextRefiner` only — it never constructs a `Diarizer`, never touches
`whspr-diarize` or `SpeakerDb`, and has no code path into either.
Diarization lives entirely behind its own subcommand and GUI section;
nothing about running it (or not) changes `transcribe`'s behavior,
dependencies, or test results.

## Contributing

See [CLAUDE.md](CLAUDE.md) for the crate/trait architecture in more depth,
the branch/merge protocol, and commit hygiene rules.
