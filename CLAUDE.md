# whspr

A desktop voice-dictation app (a functional Wispr Flow clone): hold a hotkey,
speak, get clean text injected into whatever app has focus.

## Architecture

Eight crates in a single Cargo workspace (`crates/*`):

- **whspr-core** — the spine. Domain types (`AudioBuffer`, `Transcript`,
  `AsrOptions`, `RefineContext`, `PipelineState`, `WhsprError`), the four
  traits every backend implements, and the `Pipeline` orchestrator. Depends
  on nothing else in the workspace; everything else depends on it. Also
  hosts `testkit` (feature-gated): `MockAsr` + `NoopRefiner`, shared test
  doubles so backend crates and the CLI don't each reinvent one.
- **whspr-asr** — ASR backends: `WhisperLocal` (whisper-rs), `OpenAiAsr`,
  `DeepgramAsr`. Stubs for now (`todo!()` bodies); compiles without
  whisper-rs until a backend opts it in.
- **whspr-refine** — text cleanup backends: `NoopRefiner` (real, the
  default), `OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal` (llama-cpp-2).
  Stubs compile without llama-cpp-2.
- **whspr-audio** — capture/decode/resample: `decode_wav`,
  `resample_to_16k_mono`, `start_capture`/`CaptureHandle`. Stubs compile
  without hound/rubato/cpal.
- **whspr-inject** — `GlobalHotkeyListener` + `EnigoTextSink`. Stubs compile
  without global-hotkey/enigo/arboard.
- **whspr-config** — `Config`, `AsrChoice`, `RefineChoice`, `load()` (returns
  defaults today; real file discovery is a later addition).
- **whspr-app** — the iced GUI (Hub + Flow Bar). Currently a placeholder
  binary; `iced` isn't wired in yet.
- **whspr-cli** — binary name `whspr`. `whspr --version` and
  `whspr transcribe <FILE> [--asr ID] [--refine ID]`. Already wired
  end-to-end against `whspr-core`'s mock pipeline, so the E2E harness
  (`crates/whspr-cli/tests/e2e.rs`, `assert_cmd` + `predicates`) is real from
  day one. Real backend selection lands later.

### The four core traits (all in `whspr-core`)

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

trait HotkeyListener: Send + Sync {
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<HotkeyEvent>;
}

trait TextSink: Send + Sync {
    fn insert(&self, text: &str) -> Result<()>;
}
```

### The pipeline

`Pipeline::new(Box<dyn AsrBackend>, Box<dyn TextRefiner>)`, optionally
`.with_sink(Box<dyn TextSink>)` and `.with_state_callback(...)`.
`pipeline.run(audio, &ctx).await` drives transcribe -> refine -> (optional)
inject, reporting `PipelineState` transitions through the callback.

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
- Heavy backend deps (whisper-rs, llama-cpp-2, iced, cpal, global-hotkey,
  enigo, arboard) are declared in `[workspace.dependencies]` but deliberately
  *not* pulled into any crate yet, so `cargo build --workspace` stays fast
  and doesn't need system C libs that aren't installed. Opt them into your
  own crate's `[dependencies]` when you actually implement against them.

## Commit hygiene

- One coherent change per commit: a function, a module, a single logical
  unit. Never `git add -A` a pile of unrelated work into one bunch commit —
  stage specific paths (`git add <paths>`) so each commit is independently
  reviewable.
- Conventional-commit messages: `type(scope): summary`, e.g.
  `feat(asr): backend stubs`, `test(core): mock/noop testkit`,
  `build(nix): flake devShell + crane packaging`. Common types: `feat`,
  `fix`, `test`, `build`, `docs`, `chore`, `refactor`.
- Commit as you go, not in one batch at the end — if you can't describe a
  commit in one short sentence, it's probably more than one commit.
- This applies to every team, at every level: foremen enforce it on the
  Haiku workers they delegate to, the same way it's enforced on you.

## Build & test

```sh
nix develop                              # enter the dev shell (rustc/cargo/clippy/rustfmt + system libs)
cargo build --workspace                  # should always succeed fast
cargo test --workspace                   # core pipeline test + config test + cli e2e test
cargo run -p whspr-cli -- --version
cargo run -p whspr-cli -- transcribe /dev/null   # prints the mock transcript
nix flake check                          # evaluates cargoTest via crane
```
