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

- A compiling, tested 8-crate Cargo workspace (`cargo build --workspace` /
  `cargo test --workspace`, all green).
- `whspr-core`: the domain types, the four backend traits (`AsrBackend`,
  `TextRefiner`, `HotkeyListener`, `TextSink`), and the `Pipeline`
  orchestrator (transcribe → refine → inject) are real, with passing unit
  tests.
- `whspr-cli` (the `whspr` binary): `whspr --version`, and
  `whspr transcribe <FILE>`, which runs the **real** `Pipeline` end-to-end —
  but currently against a **mock** ASR backend and a no-op refiner. It does
  not read or transcribe your audio file yet; it always returns a canned
  transcript. This exists to prove the pipeline wiring and give the project
  a real, testable end-to-end path before real backends land.
- `whspr-config`: `Config`, `AsrChoice`, `RefineChoice` types exist and
  `load()` returns their defaults. No file-based config yet — see Settings
  below.
- `whspr-asr`: `WhisperLocal` (local whisper.cpp via `whisper-rs`),
  `OpenAiAsr`, and `DeepgramAsr` are real, tested `AsrBackend`
  implementations — not stubs.
- `whspr-refine`: `NoopRefiner` (pass raw text through unchanged, the
  default), `OpenAiRefiner`, `AnthropicRefiner`, and `LlamaLocal` (local
  llama.cpp via `llama-cpp-2`) are all real, tested `TextRefiner`
  implementations.
- `whspr-audio`: mic capture, WAV decoding, resampling to 16kHz mono, and
  silence trimming are real and exercised by the real pipeline.
- `whspr-inject`: the global hotkey listener and text injection
  (clipboard-paste-first, with debounce and clipboard save/restore) are
  real `HotkeyListener`/`TextSink` implementations.
- A Nix flake: `nix develop` gives a working dev shell (Rust toolchain,
  ffmpeg, whisper.cpp, llama.cpp, cmake/clang + libclang for bindgen);
  `nix build` builds the `whspr` binary; `nix flake check` builds it in
  release mode and runs the whole test suite inside the sandbox.

**Planned / not yet implemented**

- Wiring `whspr-cli`'s `--asr` / `--refine` flags to actually select a
  backend (they're accepted today but ignored — every run uses the mock
  pipeline).
- On-disk config file loading (`whspr-config::load()` always returns
  defaults today; no file discovery, no env overrides yet).
- The GUI (`whspr-app`): currently a placeholder binary that prints
  `whspr gui (todo)` and exits. No window, no Hub, no Flow Bar yet.

## Architecture

The intended runtime pipeline is four stages, each behind its own crate and
trait:

```
capture (whspr-audio)  →  ASR (whspr-asr)  →  refine / LLM (whspr-refine)  →  inject (whspr-inject)
   mic → 16kHz f32 PCM     AsrBackend trait     TextRefiner trait              TextSink trait
   real                     real backends        real backends                real
```

`whspr-core::Pipeline` owns this flow today for the ASR → refine → inject
part; capture, ASR, refine, and inject are all real, tested
implementations, wired together by the caller (e.g. `whspr-cli`, or
`whspr-app`'s worker): `transcribe()` → `refine()` → optionally
`sink.insert()`, reporting `PipelineState` transitions (`Idle` /
`Recording` / `Transcribing` / `Refining` / `Injecting` / `Error`) as it
goes.

The workspace is 8 crates:

| Crate | Role | Status |
|---|---|---|
| `whspr-core` | Domain types, the 4 traits, `Pipeline` orchestrator | Real, tested |
| `whspr-asr` | ASR backends (`WhisperLocal`, `OpenAiAsr`, `DeepgramAsr`) | Real, tested |
| `whspr-refine` | Refine backends (`NoopRefiner`, `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`) | Real, tested |
| `whspr-audio` | Capture / WAV decode / resample to 16kHz mono | Real, tested |
| `whspr-inject` | Global hotkey listener + text injection | Real, tested |
| `whspr-config` | `Config`, `AsrChoice`, `RefineChoice`, `load()` | Real, minimal (defaults only) |
| `whspr-app` | Desktop GUI (Hub + Flow Bar, planned on `iced`) | Placeholder binary |
| `whspr-cli` | CLI binary (`whspr`) | Real, mock-backed end-to-end |

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
model, a cloud API, or (in tests) a canned mock. Swapping local ↔ cloud is a
matter of constructing it with a different concrete type, e.g.:

```rust
// fully local (once WhisperLocal / LlamaLocal are implemented)
Pipeline::new(Box::new(WhisperLocal::new(model_path)), Box::new(LlamaLocal::new(llm_path)));

// fully cloud (once OpenAiAsr / OpenAiRefiner are implemented)
Pipeline::new(Box::new(OpenAiAsr::new(api_key)), Box::new(OpenAiRefiner::new(api_key, model)));

// today, actually: the mock pipeline whspr-cli builds
Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));
```

`whspr-config`'s `AsrChoice` / `RefineChoice` enums are the intended
selector for this — the idea is that the choice becomes a config value, not
a recompile. That wiring (config value → concrete backend → `Pipeline`) is
not connected yet; see Status and Settings.

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
cargo build --workspace                             # builds all 8 crates
cargo test --workspace                              # pipeline + config + CLI e2e tests
cargo run -p whspr-cli -- --version                 # -> whspr 0.1.0
cargo run -p whspr-cli -- transcribe path/to/file    # runs the pipeline end-to-end; prints a mock transcript today
nix flake check                                     # builds whspr-cli (release) + runs the full test suite in a sandbox
```

Every command above was run and verified on aarch64-darwin before writing
this README.

## Dependencies

If you use `nix develop` / `nix build`, all of this is handled for you —
listed here so you know what's actually needed if you set up a toolchain by
hand:

- Rust (edition 2021; pinned via [fenix](https://github.com/nix-community/fenix) in the flake)
- `ffmpeg`, `whisper-cpp`, `llama-cpp` — not linked by any crate yet, but
  declared up front so the asr/audio/refine work doesn't need flake changes
  later
- `cmake`, `clang`/`libclang` — needed once `whisper-rs` / `llama-cpp-2` are
  wired in (they compile native C/C++ via cmake and generate bindings via
  bindgen)
- On Linux: `alsa-lib`, `libxkbcommon`, `wayland`, `vulkan-loader`, `libGL`
  (for audio capture and the planned `iced` GUI)
- On macOS: the unified `apple-sdk` package (AudioUnit, CoreAudio, AppKit,
  Metal, etc.)

## Settings

`whspr-config::Config` exists today with these fields; `load()` currently
always returns the defaults below — there is no config file, env var, or
CLI flag that changes them yet.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `asr` | `AsrChoice` (`whisper-local` \| `open-ai` \| `deepgram`) | `whisper-local` | Which ASR backend to use (selection not wired to `Pipeline` yet) |
| `refine` | `RefineChoice` (`noop` \| `open-ai` \| `anthropic` \| `llama-local`) | `noop` | Which refiner to use (selection not wired to `Pipeline` yet) |
| `language` | `Option<String>` | `None` | Language hint for ASR (not consumed anywhere yet) |

**Planned:** on-disk config file (via `figment`/`toml`), a platform config
directory (via `directories`), env var overrides, and wiring these values
into `whspr-cli` / `whspr-app` so they actually select a backend.

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
whspr diarize <FILE> [--model-dir <DIR>] [--embedding cam-plus-plus|eres2net] [--json]
```

- `<FILE>` — a WAV file (mono or multi-channel; it's decoded and downmixed/resampled to 16kHz mono like every other whspr audio input).
- `--embedding` — which speaker-embedding model to load: `cam-plus-plus` (WeSpeaker CAM++, the default) or `eres2net` (3D-Speaker ERes2Net). Falls back to the config file's `[speaker].embedding-model` if omitted.
- `--model-dir` — directory containing the sherpa-onnx model files (see "Offline by default" below). Falls back to `[speaker].model-dir` in the config file, then to the `SPEAKER_MODEL_DIR` environment variable.
- `--json` — print a JSON array of `{start_secs, end_secs, speaker, score}` instead of plain-text `[start-end] SpeakerN` lines.

Every run matches its turns against `speakers.json` in the platform data
directory (`~/Library/Application Support/whspr` on macOS,
`~/.local/share/whspr` on Linux — confirmed by resolving whspr's actual
`ProjectDirs` lookup) and rewrites it, so identity persists *across*
invocations, not just within one scan. Run it against two different
recordings of the same voice and the second run reuses the first run's
speaker id instead of minting a new one. Verified end to end (offline
mock backend — see below — two separate 1-second silent WAVs, same
`--data-dir`):

```
$ whspr diarize a.wav --json --data-dir /tmp/demo
[{"end_secs":2.5,"score":0.949999988079071,"speaker":"Speaker 1","start_secs":0.0},{"end_secs":5.0,"score":0.9200000166893005,"speaker":"Speaker 2","start_secs":2.5}]
$ whspr diarize b.wav --json --data-dir /tmp/demo
[{"end_secs":2.5,"score":0.949999988079071,"speaker":"Speaker 1","start_secs":0.0},{"end_secs":5.0,"score":0.9200000166893005,"speaker":"Speaker 2","start_secs":2.5}]
$ cat /tmp/demo/speakers.json   # both a.wav and b.wav recorded under each profile's "scans"
```

Both runs assign the same two speaker ids and `speakers.json` ends up with
both files' paths recorded under each profile's `scans` list — the
persisted match-or-enroll logic is real. (`--data-dir` is a hidden,
test-only flag that redirects `speakers.json` into a sandbox directory
like the one above; real usage omits it and lets `speakers.json` live in
the platform data directory.)

### Offline by default

`whspr diarize` never touches the network. If no model directory is
resolvable (from `--model-dir`, `[speaker].model-dir`, or the
`SPEAKER_MODEL_DIR` environment variable), it falls back to a
deterministic, model-free `MockDiarizer` that always returns the same two
canned turns/embeddings regardless of the input audio — that's the
backend the example above exercises. It proves the *database* logic
(matching, enrollment, persistence) for real, but since `MockDiarizer`
never actually listens to the audio, it isn't demonstrating real
acoustic voice recognition.

For real acoustic diarization, whspr is bring-your-own-model: it doesn't
fetch or ship these checkpoints for you. Download `segmentation.onnx`
plus whichever embedding checkpoint(s) you want into one directory —
`crates/whspr-diarize/src/lib.rs`'s crate-level doc comment has the exact
upstream URLs and required filenames — then point `SPEAKER_MODEL_DIR` (or
`--model-dir`) at it. Feed it a real multi-speaker recording (not a
silent test fixture — the real segmentation model finds zero turns in
silence, which is why the offline demo above deliberately uses the mock
fallback) to see genuine speaker-turn segmentation and voice-based
re-identification.

### Tests

- `crates/whspr-diarize/src/lib.rs` — unit tests for `SherpaDiarizer`'s model-dir resolution precedence, segment-range clamping, and missing-model-file error paths.
- `crates/whspr-config/src/speaker.rs` — unit tests for `SpeakerDb::match_or_enroll` (new speaker, matching speaker, orthogonal embedding creates a new speaker), `rename`, and save/load round-tripping.
- `crates/whspr-cli/tests/diarize_e2e.rs` — `assert_cmd`-driven end-to-end tests: mock-backend labeling, cross-run persistence, nonexistent-model-dir/unknown-embedding error paths, and the `SPEAKER_MODEL_DIR` fallback.
- `crates/whspr-app/src/speakers.rs` — a test covering `run_diarize_scan`'s mock-fallback and `SPEAKER_MODEL_DIR`-fallback paths for the GUI's background diarization task.
- `whspr_core::testkit::MockDiarizer` — the shared, deterministic double all of the above (and the demo above) run against; two orthogonal canned embeddings so matching-vs-enrolling is exercised meaningfully with no real model files.

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
