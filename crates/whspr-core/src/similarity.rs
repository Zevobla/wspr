//! Pure vector-similarity utilities shared by diarization and speaker
//! matching. No I/O, no async — deliberately dependency-free so it stays
//! trivially testable.

/// Cosine similarity between two equal-length embedding vectors, in
/// `[-1.0, 1.0]`. Returns `0.0` for degenerate input (empty vectors,
/// mismatched lengths, or a zero-norm vector) rather than dividing by zero.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
