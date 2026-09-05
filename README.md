# whspr

A desktop voice-dictation app — a functional [Wispr Flow](https://wispr.ai)
clone: hold a hotkey, speak, and clean, LLM-refined text lands wherever your
cursor is.

**License:** [GNU AGPL-3.0](LICENSE)

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
- `whspr-refine`: `NoopRefiner` (pass raw text through unchanged) is real
  and is the default refiner.
- A Nix flake: `nix develop` gives a working dev shell (Rust toolchain,
  ffmpeg, whisper.cpp, llama.cpp, cmake/clang + libclang for bindgen);
  `nix build` builds the `whspr` binary; `nix flake check` builds it in
  release mode and runs the whole test suite inside the sandbox.

**Planned / not yet implemented**

- Real ASR backends — `WhisperLocal` (local whisper.cpp), `OpenAiAsr`,
  `DeepgramAsr` exist as typed structs in `whspr-asr` implementing
  `AsrBackend`, but their bodies are `todo!()`.
- Real refine backends — `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`
  (local llama.cpp) exist as typed structs in `whspr-refine` implementing
  `TextRefiner`, but their bodies are `todo!()`.
- Audio capture, WAV decoding, and resampling (`whspr-audio`) — function
  signatures are settled, bodies are `todo!()`.
- Global hotkey listening and text injection (`whspr-inject`) — trait impls
  exist, bodies are `todo!()`.
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
   [planned]                [stub backends]      NoopRefiner real,             [planned]
                                                   others stub
```

`whspr-core::Pipeline` owns this flow today for the ASR → refine → inject
part (capture is wired in by the caller, e.g. `whspr-cli` currently
synthesizes silent audio instead of recording): `transcribe()` →
`refine()` → optionally `sink.insert()`, reporting `PipelineState`
transitions (`Idle` / `Recording` / `Transcribing` / `Refining` /
`Injecting` / `Error`) as it goes.

The workspace is 8 crates:

| Crate | Role | Status |
|---|---|---|
| `whspr-core` | Domain types, the 4 traits, `Pipeline` orchestrator | Real, tested |
| `whspr-asr` | ASR backends (`WhisperLocal`, `OpenAiAsr`, `DeepgramAsr`) | Stubs (`todo!()`) |
| `whspr-refine` | Refine backends (`NoopRefiner`, `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`) | `NoopRefiner` real; rest stubs |
| `whspr-audio` | Capture / WAV decode / resample to 16kHz mono | Stubs (`todo!()`) |
| `whspr-inject` | Global hotkey listener + text injection | Stubs (`todo!()`) |
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

## Original feature

TBD — to be filled in once that feature lands.

## Contributing

See [CLAUDE.md](CLAUDE.md) for the crate/trait architecture in more depth,
the branch/merge protocol, and commit hygiene rules.
