mod cli;
mod metrics;
mod report;
mod stand;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use whspr_asr::WhisperLocal;
use whspr_core::{testkit::MockAsr, AsrBackend, AsrOptions, AudioBuffer};
use whspr_audio::decode_wav;

use cli::Args;
use report::{CaseResult, Report};
use stand::StandSet;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Validate arguments
    args.validate()
        .map_err(|e| anyhow::anyhow!(e))?;

    // Load the stand set
    let stand_set_path = args.stand_set.join("эталоны.json");
    let stand_set = StandSet::load(&stand_set_path)?;

    // Create the ASR backend
    let backend: Box<dyn AsrBackend> = match args.asr.as_str() {
        "mock" => Box::new(MockAsr::default()),
        "whisper-local" => {
            let model_path = args.model.ok_or_else(|| {
                anyhow::anyhow!("model path required for whisper-local backend")
            })?;
            Box::new(WhisperLocal::new(model_path))
        }
        other => {
            return Err(anyhow::anyhow!(
                "unknown ASR backend: {} (supported: mock, whisper-local)",
                other
            ))
        }
    };

    // Prepare ASR options
    let opts = AsrOptions {
        language: Some(args.language.clone()),
    };

    // Process each case
    let audio_dir = args.stand_set.join("аудио");
    let mut case_results = Vec::new();

    for case in &stand_set.cases {
        let audio_path = audio_dir.join(&case.file);

        // Decode the audio file
        let audio = match decode_wav(&audio_path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("warning: failed to decode {}: {}", case.file, e);
                continue;
            }
        };

        // Transcribe
        let transcript = match backend.transcribe(&audio, &opts).await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("warning: transcription failed for {}: {}", case.file, e);
                continue;
            }
        };

        // Compute metrics
        let wer = metrics::wer(&transcript.text, &case.reference);
        let cer = metrics::cer(&transcript.text, &case.reference);

        case_results.push(CaseResult {
            file: case.file.clone(),
            criterion: case.criterion.clone(),
            wer,
            cer,
        });
    }

    // Generate report
    let report = Report::from_case_results(case_results);

    // Output
    if args.json {
        let json = report.format_json()?;
        println!("{}", json);
    } else {
        println!("{}", report.format_text());
    }

    Ok(())
}
