//! Manual, real-model verification harness for `SherpaDiarizer`.
//!
//! Deliberately **not** part of `cargo test --workspace` — it needs real
//! multi-gigabyte-adjacent ONNX model files and a real WAV file on disk,
//! neither of which the workspace test suite may depend on. Run it by hand:
//!
//! ```sh
//! cargo run -p whspr-diarize --example verify -- <model_dir> <wav_file>
//! ```
//!
//! `model_dir` must contain `segmentation.onnx` and `embedding.onnx` (see
//! the crate's top-level doc comment for exactly where those come from and
//! how to obtain them). `wav_file` must be 16kHz mono PCM (matching
//! `sherpa_rs::read_audio_file`'s requirement, which mirrors what the rest
//! of whspr always hands a `Diarizer`).
//!
//! Prints each detected turn's timing + embedding dimensionality, and the
//! pairwise cosine similarity between every turn's embedding so you can
//! eyeball whether turns from the same speaker cluster together (high
//! similarity) and turns from different speakers don't (low similarity).

use whspr_core::{AudioBuffer, Diarizer};
use whspr_diarize::SherpaDiarizer;

fn main() {
    let mut args = std::env::args().skip(1);
    let model_dir = args.next().expect("usage: verify <model_dir> <wav_file>");
    let wav_path = args.next().expect("usage: verify <model_dir> <wav_file>");

    let (samples, sample_rate) =
        sherpa_rs::read_audio_file(&wav_path).expect("failed to read wav file");
    let audio = AudioBuffer::new(samples, sample_rate);
    println!(
        "loaded {} ({:.2}s @ {}Hz)",
        wav_path,
        audio.duration_secs(),
        sample_rate
    );

    let diarizer = SherpaDiarizer::new(&model_dir).expect("failed to load models from model_dir");
    let turns = diarizer.diarize(&audio).expect("diarize() failed");

    println!("found {} turn(s):", turns.len());
    for (i, turn) in turns.iter().enumerate() {
        println!(
            "  [{i}] {:.2}s -> {:.2}s  embedding_dim={}  score={:.2}",
            turn.start_secs,
            turn.end_secs,
            turn.embedding.len(),
            turn.score
        );
    }

    if turns.len() > 1 {
        println!("pairwise cosine similarity (expect same-speaker turns to cluster higher):");
        for i in 0..turns.len() {
            for j in (i + 1)..turns.len() {
                let sim = whspr_core::cosine_similarity(&turns[i].embedding, &turns[j].embedding);
                println!("  [{i}] <-> [{j}]: {sim:.3}");
            }
        }
    }
}
