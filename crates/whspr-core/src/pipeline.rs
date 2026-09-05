use std::time::Duration;

use crate::error::Result;
use crate::traits::{AsrBackend, TextRefiner, TextSink};
use crate::types::{AsrOptions, AudioBuffer, PipelineState, RefineContext, Transcript};

/// Called on every pipeline state transition. Kept as a plain callback rather
/// than a channel so callers that don't care can simply omit it.
pub type StateCallback = Box<dyn Fn(PipelineState) + Send + Sync>;

/// Default cap on how long the postprocessing (refine) step may run before
/// the pipeline gives up on it. Generous enough for a real cloud LLM call
/// under normal conditions, short enough that a stalled/hung refiner never
/// stalls a live dictation turn indefinitely (see `Pipeline::with_refine_timeout`).
const DEFAULT_REFINE_TIMEOUT: Duration = Duration::from_secs(30);

/// Orchestrates a single dictation turn: transcribe -> refine -> (optionally)
/// inject. Owns its backends so callers can swap implementations freely
/// (mock, local, cloud) without the pipeline itself changing.
pub struct Pipeline {
    asr: Box<dyn AsrBackend>,
    refiner: Box<dyn TextRefiner>,
    sink: Option<Box<dyn TextSink>>,
    on_state: Option<StateCallback>,
    refine_timeout: Duration,
}

impl Pipeline {
    pub fn new(asr: Box<dyn AsrBackend>, refiner: Box<dyn TextRefiner>) -> Self {
        Self {
            asr,
            refiner,
            sink: None,
            on_state: None,
            refine_timeout: DEFAULT_REFINE_TIMEOUT,
        }
    }

    pub fn with_sink(mut self, sink: Box<dyn TextSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn with_state_callback(mut self, cb: StateCallback) -> Self {
        self.on_state = Some(cb);
        self
    }

    /// Overrides the default 30s cap (`DEFAULT_REFINE_TIMEOUT`) on the
    /// refine step. A refiner that's still running past this deadline
    /// doesn't fail the turn - `run`/`run_with_transcript` fall back to the
    /// raw ASR text instead, so a hung/slow postprocessing backend can
    /// never stall dictation forever.
    pub fn with_refine_timeout(mut self, timeout: Duration) -> Self {
        self.refine_timeout = timeout;
        self
    }

    fn report(&self, state: PipelineState) {
        if let Some(cb) = &self.on_state {
            cb(state);
        }
    }

    /// Runs one full turn and returns the final (refined) text. Injects it
    /// via the configured `TextSink` as a side effect if one is set. If the
    /// refiner is still running past `refine_timeout` (see
    /// `with_refine_timeout`), falls back to the raw ASR text instead of
    /// failing the turn.
    pub async fn run(&self, audio: AudioBuffer, ctx: &RefineContext) -> Result<String> {
        let (_transcript, refined) = self.run_with_transcript(audio, ctx).await?;
        Ok(refined)
    }

    /// Like `run`, but also returns the full ASR `Transcript` (including
    /// per-segment timing) alongside the refined text. `run` alone discards
    /// segment timing once it flattens the result down to a single
    /// `String`; callers that need both the clean final text and
    /// per-segment timestamps (e.g. exporting an SRT/VTT file) want this
    /// instead. See `run`'s doc comment for the refine-timeout fallback
    /// behavior.
    pub async fn run_with_transcript(
        &self,
        audio: AudioBuffer,
        ctx: &RefineContext,
    ) -> Result<(Transcript, String)> {
        self.report(PipelineState::Transcribing);
        let transcript = self.asr.transcribe(&audio, &AsrOptions::default()).await?;

        self.report(PipelineState::Refining);
        let refined = match tokio::time::timeout(
            self.refine_timeout,
            self.refiner.refine(&transcript.text, ctx),
        )
        .await
        {
            Ok(result) => result?,
            Err(_elapsed) => {
                // Postprocessing took too long (O-07): don't stall - or
                // fail - the whole turn over a slow/hung refiner. Fall back
                // to the raw ASR text, same as a no-op refiner would have
                // produced.
                self.report(PipelineState::Error);
                transcript.text.clone()
            }
        };

        if let Some(sink) = &self.sink {
            self.report(PipelineState::Injecting);
            sink.insert(&refined)?;
        }

        self.report(PipelineState::Idle);
        Ok((transcript, refined))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{MockAsr, NoopRefiner};

    #[tokio::test]
    async fn runs_mock_asr_through_noop_refiner() {
        let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));
        let audio = AudioBuffer::new(vec![0.0; 16_000], 16_000);

        let result = pipeline
            .run(audio, &RefineContext::default())
            .await
            .unwrap();

        assert_eq!(result, MockAsr::default().canned.text);
    }

    #[tokio::test]
    async fn run_with_transcript_returns_transcript_alongside_refined_text() {
        let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));
        let audio = AudioBuffer::new(vec![0.0; 16_000], 16_000);

        let (transcript, refined) = pipeline
            .run_with_transcript(audio, &RefineContext::default())
            .await
            .unwrap();

        assert_eq!(transcript, MockAsr::default().canned);
        assert_eq!(refined, MockAsr::default().canned.text);
    }

    #[tokio::test]
    async fn reports_expected_state_transitions() {
        use std::sync::{Arc, Mutex};

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_cb = seen.clone();
        let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner))
            .with_state_callback(Box::new(move |s| seen_cb.lock().unwrap().push(s)));

        pipeline
            .run(
                AudioBuffer::new(vec![0.0; 100], 16_000),
                &RefineContext::default(),
            )
            .await
            .unwrap();

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                PipelineState::Transcribing,
                PipelineState::Refining,
                PipelineState::Idle
            ]
        );
    }
}
