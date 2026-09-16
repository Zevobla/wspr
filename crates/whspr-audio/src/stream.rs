//! Shared cpal input-stream plumbing for `capture` and `preroll_monitor`:
//! converting whatever sample format/channel layout the device reports
//! into mono `f32`, once, instead of each caller hand-rolling its own
//! F32/I16/U16 conversion closures.

use cpal::traits::DeviceTrait;
use cpal::{FromSample, Sample, SizedSample, StreamConfig};

use whspr_core::{Result, WhsprError};

/// Downmixes interleaved multi-channel `data` (frame-major, e.g.
/// `[L0, R0, L1, R1, ...]` for 2 channels) to mono, appending the result
/// to `out`.
///
/// Each output sample is the average of one input frame's `channels`
/// samples. `channels <= 1` is already mono, so `data` is copied through
/// unchanged (no averaging division needed, and no behavior change for
/// the common single-channel case). A trailing partial frame — `data.len()`
/// not an exact multiple of `channels` — is dropped rather than guessed
/// at, since cpal callbacks always deliver whole frames in practice and a
/// stray partial one isn't safely averageable.
pub(crate) fn downmix_interleaved(data: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(data);
        return;
    }

    let frames = data.len() / channels;
    out.reserve(frames);
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        out.push(sum / channels as f32);
    }
}

/// Builds a cpal input stream on `device` that converts every incoming
/// buffer — whatever sample format the device reports (F32/I16/U16) and
/// however many channels it has — to mono `f32` before handing it to
/// `sink`. Shared by `capture::start_capture_with` and
/// `PrerollMonitor::start`, so there's exactly one implementation of
/// "convert the device's native samples to mono f32", not one per caller
/// (each previously hand-rolled its own F32/I16/U16 closures).
///
/// `what` is a short label (e.g. `"capture"`, `"preroll"`) folded into
/// any build/stream-error message, to keep the two callers'
/// diagnostics distinguishable.
pub(crate) fn build_mono_input_stream<F>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    sink: F,
    what: &'static str,
) -> Result<cpal::Stream>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    let channels = config.channels() as usize;
    let stream_config: StreamConfig = (*config).into();

    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_typed_input_stream::<f32, F>(device, stream_config, channels, sink, what)
        }
        cpal::SampleFormat::I16 => {
            build_typed_input_stream::<i16, F>(device, stream_config, channels, sink, what)
        }
        cpal::SampleFormat::U16 => {
            build_typed_input_stream::<u16, F>(device, stream_config, channels, sink, what)
        }
        other => Err(WhsprError::Audio(format!(
            "unsupported sample format for {what} stream: {other:?}"
        ))),
    }
}

/// Builds the actual typed cpal stream for one concrete sample type `T`,
/// converting each callback's buffer to `f32` and downmixing it to mono
/// before calling `sink`. The two scratch buffers are allocated once
/// (captured by the callback closure `build_input_stream` stores) and
/// only `clear()`-ed per callback, so steady-state capture does no
/// per-callback heap allocation beyond growth.
fn build_typed_input_stream<T, F>(
    device: &cpal::Device,
    stream_config: StreamConfig,
    channels: usize,
    mut sink: F,
    what: &'static str,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
    F: FnMut(&[f32]) + Send + 'static,
{
    let mut float_scratch: Vec<f32> = Vec::new();
    let mut mono_scratch: Vec<f32> = Vec::new();

    device
        .build_input_stream(
            stream_config,
            move |data: &[T], _: &_| {
                float_scratch.clear();
                float_scratch.extend(data.iter().map(|s| f32::from_sample(*s)));
                mono_scratch.clear();
                downmix_interleaved(&float_scratch, channels, &mut mono_scratch);
                sink(&mono_scratch);
            },
            move |err| tracing::error!("cpal {what} stream error: {err}"),
            None,
        )
        .map_err(|e| crate::capture::mic_access_error(&format!("build {what} input stream"), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_interleaved_mono_is_a_passthrough() {
        let data = vec![0.1, 0.2, 0.3];
        let mut out = Vec::new();
        downmix_interleaved(&data, 1, &mut out);
        assert_eq!(out, data);
    }

    #[test]
    fn downmix_interleaved_stereo_averages_channels() {
        // [L0, R0, L1, R1] -> [(L0+R0)/2, (L1+R1)/2]
        let data = vec![1.0, 0.0, 0.5, 0.5];
        let mut out = Vec::new();
        downmix_interleaved(&data, 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_interleaved_three_channels_averages_all_three() {
        let data = vec![1.0, 0.5, 0.0]; // one frame, 3 channels
        let mut out = Vec::new();
        downmix_interleaved(&data, 3, &mut out);
        assert_eq!(out, vec![0.5]);
    }

    #[test]
    fn downmix_interleaved_drops_trailing_partial_frame() {
        // Two full stereo frames plus one stray extra sample.
        let data = vec![1.0, 0.0, 0.5, 0.5, 0.9];
        let mut out = Vec::new();
        downmix_interleaved(&data, 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_interleaved_of_empty_slice_is_empty() {
        let data: Vec<f32> = Vec::new();
        let mut out = Vec::new();
        downmix_interleaved(&data, 2, &mut out);
        assert!(out.is_empty());
    }
}
