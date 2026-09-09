//! Media-import orchestration for whspr: turn a media URL (a lecture, a
//! podcast, a YouTube video or whole playlist) into either *published
//! captions* — instant, no model, no download — or a downloaded 16kHz mono
//! WAV ready for whisper.
//!
//! This crate is a **library**: it shells out to the `yt-dlp` and `ffmpeg`
//! command-line tools and parses their output. It deliberately pulls in no
//! network or media crate of its own — the tools do the heavy lifting, and
//! this crate is the typed, testable seam around them. UI is a later phase;
//! nothing here knows about iced.
//!
//! # The two import paths
//!
//! - **Captions (instant).** `resolve` runs `yt-dlp --dump-single-json` and
//!   reports what's available via `MediaInfo` — title, duration, chapters,
//!   and which human / auto-generated caption languages exist. If a caption
//!   track is present, `parse_captions` turns a downloaded WebVTT / srv /
//!   json3 track into a `whspr_core::Transcript` with per-segment
//!   timestamps, *without running any ASR model*.
//! - **Audio (whisper).** When there are no usable captions, `download_audio`
//!   extracts the audio-only stream and transcodes it to a 16kHz mono WAV.
//!   `download_to_audio` ties that to `whspr-audio`'s decode/resample so the
//!   caller gets a `whspr_core::AudioBuffer` to hand to whisper.
//!
//! # Tool discovery
//!
//! `resolve_tool` locates `yt-dlp`/`ffmpeg` with a fixed precedence — an
//! explicit env override, then a bundled copy inside the macOS `.app`
//! (`<exe_dir>/../Resources/<tool>`), then `PATH` — mirroring the spirit of
//! `whspr_asr::WhisperLocal::resolve_model_path`. The bundled location is
//! the contract a later app-bundling phase fills in.
//!
//! # Temp-file ownership
//!
//! `download_audio` returns the path to a freshly written WAV in a temporary
//! directory. **The caller owns that file and must delete it** after
//! transcribing — this crate does not track or reap it. That delete-after-use
//! policy is enforced at the app layer, not here, so the library stays a pure
//! "URL in, path out" function.

mod resolve;
mod tools;

pub use resolve::{Chapter, CookiesFrom, Lang, MediaInfo, Playlist, PlaylistEntry};
pub use tools::{resolve_tool, Tool};
