#![allow(dead_code)]

//! Voice Activity Detection (VAD) and Silence Trimming Engine
//! Automatically trims silence from audio buffers to accelerate Whisper transcription latency
//! and provides real-time silence detection for hands-free auto-stop.

/// Computes the Root Mean Square (RMS) energy of an audio sample frame
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Trims dead silence from the beginning and end of an audio buffer
///
/// Keeps a brief pre-roll (50ms) and hangover (100ms) to ensure leading
/// consonants and trailing words are never clipped.
pub fn trim_silence(samples: &[f32], sample_rate: u32, threshold_rms: f32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let frame_size = (sample_rate as usize) / 50; // 20ms frame
    if frame_size == 0 || samples.len() < frame_size {
        return samples.to_vec();
    }

    let pre_roll_samples = (sample_rate as usize * 50) / 1000; // 50ms pre-roll
    let hangover_samples = (sample_rate as usize * 100) / 1000; // 100ms hangover

    // 1. Find start of voice
    let mut start_idx = 0;
    for chunk_start in (0..samples.len()).step_by(frame_size) {
        let chunk_end = (chunk_start + frame_size).min(samples.len());
        let rms = compute_rms(&samples[chunk_start..chunk_end]);
        if rms >= threshold_rms {
            start_idx = chunk_start.saturating_sub(pre_roll_samples);
            break;
        }
    }

    // 2. Find end of voice (searching backward)
    let mut end_idx = samples.len();
    let num_chunks = samples.len() / frame_size;
    for i in (0..num_chunks).rev() {
        let chunk_start = i * frame_size;
        let chunk_end = (chunk_start + frame_size).min(samples.len());
        let rms = compute_rms(&samples[chunk_start..chunk_end]);
        if rms >= threshold_rms {
            end_idx = (chunk_end + hangover_samples).min(samples.len());
            break;
        }
    }

    if start_idx >= end_idx {
        return samples.to_vec();
    }

    samples[start_idx..end_idx].to_vec()
}

/// Computes the duration (in seconds) of continuous trailing silence at the end of the buffer
pub fn detect_trailing_silence_sec(samples: &[f32], sample_rate: u32, threshold_rms: f32) -> f32 {
    if samples.is_empty() || sample_rate == 0 {
        return 0.0;
    }

    let frame_size = (sample_rate as usize) / 50; // 20ms frame
    if frame_size == 0 {
        return 0.0;
    }

    let mut silent_frames = 0;
    let num_chunks = samples.len() / frame_size;

    for i in (0..num_chunks).rev() {
        let chunk_start = i * frame_size;
        let chunk_end = (chunk_start + frame_size).min(samples.len());
        let rms = compute_rms(&samples[chunk_start..chunk_end]);

        if rms < threshold_rms {
            silent_frames += 1;
        } else {
            break;
        }
    }

    silent_frames as f32 * 0.02 // 20ms per frame
}

/// Checks if hands-free dictation should auto-stop based on silence duration
pub fn should_handsfree_auto_stop(
    samples: &[f32],
    sample_rate: u32,
    min_speech_sec: f32,
    max_trailing_silence_sec: f32,
) -> bool {
    let total_duration = samples.len() as f32 / sample_rate as f32;
    if total_duration < min_speech_sec {
        return false;
    }

    let trailing_silence = detect_trailing_silence_sec(samples, sample_rate, 0.012);
    trailing_silence >= max_trailing_silence_sec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_silence() {
        let sample_rate = 16000;
        // 1 sec silence, 0.5 sec speech, 1 sec silence
        let mut audio = vec![0.0f32; 16000];
        let speech = vec![0.1f32; 8000];
        let trailing = vec![0.0f32; 16000];

        audio.extend(speech);
        audio.extend(trailing);

        let trimmed = trim_silence(&audio, sample_rate, 0.02);
        // Original is 40,000 samples, trimmed should be ~8000 + pre/post padding
        assert!(trimmed.len() < 15000);
        assert!(trimmed.len() > 7000);
    }

    #[test]
    fn test_trailing_silence_detection() {
        let sample_rate = 16000;
        let speech = vec![0.1f32; 16000]; // 1s speech
        let silence = vec![0.0f32; 24000]; // 1.5s silence

        let mut buffer = speech;
        buffer.extend(silence);

        let silence_sec = detect_trailing_silence_sec(&buffer, sample_rate, 0.02);
        assert!((silence_sec - 1.5).abs() < 0.1);

        assert!(should_handsfree_auto_stop(&buffer, sample_rate, 0.5, 1.4));
    }
}
