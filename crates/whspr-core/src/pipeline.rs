use crate::error::Result;
use crate::traits::{AsrBackend, TextRefiner, TextSink};
use crate::types::{AsrOptions, AudioBuffer, PipelineState, RefineContext};

/// Called on every pipeline state transition. Kept as a plain callback rather
/// than a channel so callers that don't care can simply omit it.
pub type StateCallback = Box<dyn Fn(PipelineState) + Send + Sync>;

/// Orchestrates a single dictation turn: transcribe -> refine -> (optionally)
/// inject. Owns its backends so callers can swap implementations freely
/// (mock, local, cloud) without the pipeline itself changing.
pub struct Pipeline {
    asr: Box<dyn AsrBackend>,
    refiner: Box<dyn TextRefiner>,
    sink: Option<Box<dyn TextSink>>,
    on_state: Option<StateCallback>,
}

impl Pipeline {
    pub fn new(asr: Box<dyn AsrBackend>, refiner: Box<dyn TextRefiner>) -> Self {
        Self {
            asr,
            refiner,
            sink: None,
            on_state: None,
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

    fn report(&self, state: PipelineState) {
        if let Some(cb) = &self.on_state {
            cb(state);
        }
    }

    /// Runs one full turn and returns the final (refined) text. Injects it
    /// via the configured `TextSink` as a side effect if one is set.
    pub async fn run(&self, audio: AudioBuffer, ctx: &RefineContext) -> Result<String> {
        self.report(PipelineState::Transcribing);
        let transcript = self.asr.transcribe(&audio, &AsrOptions::default()).await?;

        self.report(PipelineState::Refining);
        let refined = self.refiner.refine(&transcript.text, ctx).await?;

        if let Some(sink) = &self.sink {
            self.report(PipelineState::Injecting);
            sink.insert(&refined)?;
        }

        self.report(PipelineState::Idle);
        Ok(refined)
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
