# whspr

A desktop voice-dictation app (a functional Wispr Flow clone): hold a hotkey,
speak, get clean text injected into whatever app has focus.

## Architecture

15 crates in a single Cargo workspace (`crates/*`):

- **whspr-core** — the spine. Domain types (`AudioBuffer`, `Transcript`,
  `AsrOptions`, `RefineContext`, `PipelineState`, `WhsprError`,
  `SpeakerTurn`), the five traits every backend implements (`AsrBackend`,
  `TextRefiner`, `HotkeyListener`, `TextSink`, `Diarizer`), and the
  `Pipeline` orchestrator. Depends on nothing else in the workspace;
  everything else depends on it. Also hosts `testkit` (feature-gated):
  `MockAsr` + `NoopRefiner` + `MockDiarizer`, shared test doubles so
  backend crates, the CLI, and the GUI don't each reinvent one.
- **whspr-asr** — ASR backends: `WhisperLocal` (whisper-rs), `OpenAiAsr`,
  `DeepgramAsr`, `AppleSpeech` (macOS on-device `SFSpeechRecognizer`). Real,
  tested implementations, not stubs.
- **whspr-refine** — text cleanup backends: `NoopRefiner` (real, the
  default), `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`
  (llama-cpp-2), `AppleFoundation` (macOS 26+ Apple Intelligence, when
  built with the shim). All real, tested `TextRefiner` implementations,
  wrapped in a `NormalizingRefiner` decorator (rule-based number/date/time
  normalization, macro/dictionary substitution — including a sandboxed
  LuaJIT scripting layer for `lua:`-prefixed macros — dedup, paragraph
  breaks).
- **whspr-audio** — capture/decode/resample: `decode_wav`,
  `resample_to_16k_mono`, `start_capture`/`CaptureHandle`,
  `trim_silence`/`trim_silence_default`, `PrerollBuffer`. All real, tested;
  `PrerollBuffer` (pre-trigger sample retention) is implemented and
  unit-tested but not yet called from the live capture path.
- **whspr-inject** — `GlobalHotkeyListener` + `EnigoTextSink`
  (clipboard-paste-first with a synthetic-typing fallback, debounce,
  clipboard save/restore). Real, tested.
- **whspr-diarize** — `SherpaDiarizer`: sherpa-onnx-backed speaker-turn
  segmentation + embedding extraction (the `Diarizer` implementation).
  Real, tested; `sherpa-rs` ships no prebuilt native library on
  aarch64-windows, so construction returns an honest error there.
- **whspr-config** — `Config` and every settings section (`whisper`,
  `speaker`, `normalize`, `language`/`language_settings`, `device`,
  `autostart`, `sound`, `injection`, `privacy`, `capture`, `huggingface`,
  `refine_settings`, top-level `hotkey`/`api_keys`). `load()` reads
  `config.toml` from the platform config directory, overlaid on compiled-in
  defaults, and writes those defaults out on first run. The config file is
  the *only* override mechanism — no environment variables, by design.
- **whspr-hf** — in-app HuggingFace OAuth sign-in + model browse/download
  client, so the GUI's Models tab can populate `whisper.model_path`
  without hand-editing config. Depends only on `whspr-core`.
- **whspr-import** — media-import orchestration: shells out to `yt-dlp`/
  `ffmpeg` to pull published captions instantly, or download audio for
  transcription, from a URL. A library; no `iced` dependency.
- **whspr-typst** — an in-process Typst compiler world (`typst-assets` base
  faces, fully offline) plus a generic lecture-notes template: `.typ`
  generation, SVG preview, PDF export. Real, tested. `whspr-app`'s
  `note_export.rs` builds the Note desk's own `.typ` layout and compiles it
  to PDF through `whspr_typst::export_pdf` — no system `typst` binary.
- **whspr-app** — the iced 0.14 GUI (Hub: Dictate / History / Speakers /
  Models / Settings / Note desk / link import; system tray on
  macOS/Windows, deliberately not implemented on Linux — see
  `tray.rs`'s module doc). A real, ~14k-line implementation with a
  background worker (`worker.rs`) running the real hotkey → mic capture →
  `Pipeline` → inject loop — not a placeholder.
- **whspr-cli** — binary name `whspr`. `whspr --version`,
  `whspr transcribe <FILE> [--asr ID] [--refine ID]`, `transcribe-batch`,
  `diarize`, `stats`, `uninstall`. Real backend selection: the no-flag
  `--asr` default is real `WhisperLocal`; `--asr mock` is an explicit,
  documented opt-in used by the e2e harness
  (`crates/whspr-cli/tests/*.rs`, `assert_cmd` + `predicates`) and by
  `whspr-check`.
- **whspr-setup** — a standalone Windows installer: a real iced GUI
  (Install → Installing → Done/Failure) performing a genuine per-user
  install of an embedded app payload (`%LOCALAPPDATA%\whspr`, Start-menu/
  Desktop shortcuts, autostart registry value). Also builds and renders on
  macOS (a no-op install path) so the shared UI stays testable everywhere.
  Not yet wired into `release.yml` — see `docs/RELEASING.md`.
- **whspr-bench** — a CLI that benchmarks ASR backends (e.g.
  `WhisperLocal` vs `MockAsr`) over a set of audio fixtures.
- **whspr-check** — an independent acceptance checker
  (`cargo run -p whspr-check`) that scores the repo against a curated
  subset of the project's acceptance criteria. Only ever reports PASS for
  something it actually verified; unverified criteria land in an explicit
  "not yet automated" bucket instead of being guessed at.

### The five core traits (all in `whspr-core`)

```rust
#[async_trait]
trait AsrBackend: Send + Sync {
    async fn transcribe(&self, audio: &AudioBuffer, opts: &AsrOptions) -> Result<Transcript>;

    /// Default delegates to `transcribe` and ignores `progress`; only
    /// backends that can actually report progress (e.g. local whisper)
    /// need override it.
    async fn transcribe_with_progress(
        &self,
        audio: &AudioBuffer,
        opts: &AsrOptions,
        progress: tokio::sync::mpsc::UnboundedSender<u8>,
    ) -> Result<Transcript> {
        let _ = progress;
        self.transcribe(audio, opts).await
    }

    fn id(&self) -> &'static str;
}

#[async_trait]
trait TextRefiner: Send + Sync {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String>;
    fn id(&self) -> &'static str;
}

trait HotkeyListener: Send + Sync {
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<HotkeyEvent>;
}

trait TextSink: Send + Sync {
    fn insert(&self, text: &str) -> Result<()>;
}

/// Not async: diarization backends are CPU-bound native inference calls
/// (sherpa-onnx FFI), not I/O-bound network calls. Callers that need this
/// off the async runtime's thread wrap a call in `spawn_blocking` themselves.
trait Diarizer: Send + Sync {
    fn diarize(&self, audio: &AudioBuffer) -> Result<Vec<SpeakerTurn>>;
    fn id(&self) -> &'static str;
}
```

### The pipeline

`Pipeline::new(Box<dyn AsrBackend>, Box<dyn TextRefiner>)`, optionally
`.with_sink(Box<dyn TextSink>)`, `.with_state_callback(...)`, and
`.with_language(...)`. `pipeline.run(audio, &ctx).await` (or
`run_with_transcript` for the timing-annotated `Transcript`) drives
transcribe -> refine -> (optional) inject, reporting `PipelineState`
transitions through the callback. Diarization is a separate, independent
path (`Diarizer` + `whspr-diarize` + `SpeakerDb`) that `Pipeline` never
constructs or touches.

## Branch/merge protocol

- `main` is **oracle-only**. No one else ever checks out, commits to, or
  merges `main`.
- Each team works on its own `team/<name>` branch (typically in its own git
  worktree) and **never** touches another team's branch.
- Commit every logical step, with a clear message. The oracle reviews and
  merges branches into `main`; teams do not merge anything themselves.
- **Never edit** the root `Cargo.toml`'s `[workspace.dependencies]` or
  `flake.nix`. Every dependency the project will need is already declared in
  `[workspace.dependencies]` — opt in from your own crate with
  `some-dep.workspace = true` (add `features = [...]` on your own line if you
  need more than the workspace default). If you genuinely need a new
  workspace dependency or system library that isn't already there, **do not
  add it yourself** — note it in your final report to the oracle instead.
- Heavy backend deps (whisper-rs, llama-cpp-2, sherpa-rs, mlua, iced, cpal,
  global-hotkey, enigo, arboard) are declared in `[workspace.dependencies]`
  and opted into only by the crate that actually implements against them
  (whspr-asr, whspr-refine, whspr-diarize, whspr-app, whspr-audio,
  whspr-inject). Don't pull one into a crate that doesn't need it: they are
  why `cargo build --workspace` needs cmake/libclang and therefore the nix
  dev shell.

## Commit hygiene

- Commit at the **smallest coherent step**, not per-feature or per-crate:
  add a struct in one commit, its first method in the next, each test in
  its own commit, a doc comment separately. If a single commit's diff spans
  multiple functions or a whole module at once, it's almost certainly
  several commits. Never `git add -A` a pile of unrelated work into one
  bunch commit — stage specific paths (`git add <paths>`) so each commit is
  independently reviewable.
- Prefer many small commits over few large ones. This is graded: AD-04
  penalizes a large fraction of code landing in one commit; AD-07 wants
  tests committed alongside the code they cover, not lumped into a separate
  bulk commit at the end.
- Rule of thumb: if the one-line commit subject needs "and" or a comma to
  describe it, split it into more commits.
- Conventional-commit messages: `type(scope): summary`, e.g.
  `feat(asr): backend stubs`, `test(core): mock/noop testkit`,
  `build(nix): flake devShell + crane packaging`. Common types: `feat`,
  `fix`, `test`, `build`, `docs`, `chore`, `refactor`.
- Commit as you go, not in one batch at the end.
- Commits are unsigned: append `--no-gpg-sign` to every `git commit` (the
  global `commit.gpgsign` setting stays untouched — never edit git config
  to work around this).
- This applies to every team, at every level: foremen enforce it on the
  Haiku workers they delegate to, the same way it's enforced on you.

## Build & test

Every `cargo`/`nix` command below must run through the Nix dev shell
(`nix develop -c <command>`, or `nix develop` first to enter it
interactively) — the toolchain (cmake, libclang, the pinned Rust) only
exists there.

```sh
nix develop                                                              # enter the dev shell (rustc/cargo/clippy/rustfmt + system libs)
nix develop -c cargo build --workspace                                   # should always succeed fast
nix develop -c cargo test --workspace                                    # full workspace test suite
nix develop -c cargo run -p whspr-cli -- --version
nix develop -c cargo run -p whspr-cli -- transcribe /dev/null --asr mock # prints the mock transcript, no model required
nix flake check                                                          # evaluates cargoTest via crane
```

**Acceptance gate** (run before calling anything done):

```sh
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo run -p whspr-check                                  # must report 0 fail
```

No source file should exceed 600 lines (AA-06) — split a growing module
into a submodule (see e.g. `whspr-cli`'s `transcribe_cmd.rs`/
`diarize_cmd.rs`/`stats_cmd.rs` split out of `main.rs`, or `whspr-config`'s
per-section files) before it crosses that line.
