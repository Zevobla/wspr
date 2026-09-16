//! Turns finished audio chunks into dictation results off the capture loop:
//! one task owns the `Pipeline` and works through chunks in the order they
//! were recorded, so the hotkey loop keeps listening -- and, with auto-send,
//! keeps recording -- while a chunk transcribes.

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use whspr_config::Config;
use whspr_core::{AudioBuffer, Pipeline, RefineContext};

use super::WorkerEvent;

/// One finished stretch of dictation, ready to transcribe.
pub(super) struct Chunk {
    /// The trimmed 16 kHz mono clip (see `capture_plan::trim_captured`).
    pub audio: AudioBuffer,
    /// The app that had focus when the hotkey went down, as refiner context.
    pub app_name: Option<String>,
    /// The settings in force when the chunk was captured.
    pub config: Config,
}

/// Spawns the chunk-processing task and returns the sender that feeds it.
/// Each chunk's outcome is reported on `output`; the task ends once every
/// sender is dropped or the app stops listening.
pub(super) fn spawn(
    pipeline: Pipeline,
    mut output: mpsc::Sender<WorkerEvent>,
) -> UnboundedSender<Chunk> {
    let (chunks, mut incoming) = unbounded_channel::<Chunk>();
    tokio::spawn(async move {
        while let Some(chunk) = incoming.recv().await {
            let event = process(&pipeline, chunk).await;
            if output.send(event).await.is_err() {
                break;
            }
        }
    });
    chunks
}

/// Transcribes and refines one chunk into the event the app should see.
async fn process(pipeline: &Pipeline, chunk: Chunk) -> WorkerEvent {
    let Chunk {
        audio,
        app_name,
        config,
    } = chunk;
    let duration_secs = audio.duration_secs();
    // Fingerprint the speaker over the whole clip *before* `pipeline.run`
    // consumes `audio` (it takes it by value), reusing the file path's exact
    // logic. `None` whenever attribution isn't possible; transcription
    // proceeds unaffected either way.
    let embedding = crate::transcribe_file::compute_embedding(&audio, &config).await;
    let ctx = RefineContext {
        app_name,
        instructions: Some(whspr_refine::effective_instructions(
            config.refine_settings.instructions.as_deref(),
        )),
        ..Default::default()
    };
    match pipeline.run(audio, &ctx).await {
        Ok(text) => WorkerEvent::Completed {
            text,
            duration_secs,
            embedding,
        },
        Err(error) => WorkerEvent::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_core::testkit::{MockAsr, NoopRefiner};

    fn chunk() -> Chunk {
        let mut config = Config::default();
        // Keep the test off any locally installed speaker model.
        config.speaker.enabled = false;
        Chunk {
            audio: AudioBuffer::new(vec![0.1; 16000], 16000),
            app_name: None,
            config,
        }
    }

    #[tokio::test]
    async fn a_chunk_becomes_a_completed_dictation() {
        let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));

        match process(&pipeline, chunk()).await {
            WorkerEvent::Completed {
                text,
                duration_secs,
                embedding,
            } => {
                assert!(!text.is_empty());
                assert_eq!(duration_secs, 1.0);
                assert!(embedding.is_none());
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

}
