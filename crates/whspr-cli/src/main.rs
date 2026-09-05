use std::path::PathBuf;

use clap::{Parser, Subcommand};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AudioBuffer, Pipeline, RefineContext};

#[derive(Parser)]
#[command(name = "whspr", version, about = "whspr voice dictation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Transcribe an audio file through the dictation pipeline.
    Transcribe {
        file: PathBuf,

        /// ASR backend id (real backends wired up by a later team; ignored for now).
        #[arg(long)]
        asr: Option<String>,

        /// Refiner id (real backends wired up by a later team; ignored for now).
        #[arg(long)]
        refine: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Transcribe { file, asr, refine }) => {
            // Backend selection (whisper-rs, OpenAI, Anthropic, ...) lands
            // with the asr/refine teams; every choice resolves to the mock
            // pipeline for now so this binary already works end-to-end.
            let _ = (asr, refine);
            let _ = file; // the mock backend ignores audio content entirely

            let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner));
            let audio = AudioBuffer::new(vec![0.0; 16_000], 16_000);
            let output = pipeline.run(audio, &RefineContext::default()).await?;
            println!("{output}");
        }
        None => {
            eprintln!("no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`");
        }
    }

    Ok(())
}
