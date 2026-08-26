use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use hound::{WavSpec, WavWriter};
use parking_lot::Mutex;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Recorder {
    pub target_sample_rate: u32,
    pub target_channels: u16,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Recorder {
            target_sample_rate: 16000,
            target_channels: 1,
        }
    }

    /// Records audio until `stop_flag` is set to true.
    /// Captures at hardware native sample rate/channels, then resamples and downmixes to 16kHz mono WAV.
    pub fn record_with_streaming(
        &self,
        stop_flag: Arc<AtomicBool>,
        level_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    ) -> Result<Vec<u8>, String> {
        let host = cpal::default_host();
        let (device, supported_config) = get_input_device_and_config(&host)?;

        let native_sample_rate = supported_config.sample_rate().0;
        let native_channels = supported_config.channels();
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();

        println!(
            "[recorder] Capturing audio: native {}Hz, {} ch, format: {:?}",
            native_sample_rate, native_channels, sample_format
        );

        let raw_samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let samples_clone = raw_samples.clone();
        let stop_clone = stop_flag.clone();
        let cb_clone = level_callback.clone();

        let err_fn = |err| eprintln!("[Keryx] Audio stream error: {err}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    if !stop_clone.load(Ordering::Relaxed) {
                        samples_clone.lock().extend_from_slice(data);
                        if let Some(cb) = &cb_clone {
                            let sum_sq: f32 = data.iter().map(|&s| s * s).sum();
                            let rms = (sum_sq / data.len().max(1) as f32).sqrt();
                            cb((rms * 12.0).min(1.0));
                        }
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    if !stop_clone.load(Ordering::Relaxed) {
                        let converted: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        samples_clone.lock().extend(&converted);
                        if let Some(cb) = &cb_clone {
                            let sum_sq: f32 = converted.iter().map(|&s| s * s).sum();
                            let rms = (sum_sq / converted.len().max(1) as f32).sqrt();
                            cb((rms * 12.0).min(1.0));
                        }
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::U8 => device.build_input_stream(
                &stream_config,
                move |data: &[u8], _| {
                    if !stop_clone.load(Ordering::Relaxed) {
                        let converted: Vec<f32> =
                            data.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect();
                        samples_clone.lock().extend(&converted);
                        if let Some(cb) = &cb_clone {
                            let sum_sq: f32 = converted.iter().map(|&s| s * s).sum();
                            let rms = (sum_sq / converted.len().max(1) as f32).sqrt();
                            cb((rms * 12.0).min(1.0));
                        }
                    }
                },
                err_fn,
                None,
            ),
            _ => {
                return Err(format!("Unsupported audio sample format: {:?}", sample_format));
            }
        }
        .map_err(|e| format!("Failed to build audio input stream: {e}"))?;

        stream.play().map_err(|e| format!("Failed to start audio stream: {e}"))?;

        // Record until stop flag is set
        while !stop_flag.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        drop(stream);

        let captured_samples = raw_samples.lock().clone();
        if captured_samples.is_empty() {
            return Err("No audio samples captured".to_string());
        }

        // Downmix to mono and resample to target_sample_rate (16000 Hz)
        let mut processed_samples = resample_and_downmix(
            &captured_samples,
            native_sample_rate,
            self.target_sample_rate,
            native_channels,
        );

        // Auto-gain normalize to boost quiet speech and whisper for high STT accuracy
        normalize_audio_gain(&mut processed_samples);

        let wav_bytes = encode_to_wav(&processed_samples, self.target_sample_rate, self.target_channels)?;
        Ok(wav_bytes)
    }

    #[allow(dead_code)]
    pub fn samples_to_wav(&self, samples: &[f32]) -> Result<Vec<u8>, String> {
        let mut samples_vec = samples.to_vec();
        normalize_audio_gain(&mut samples_vec);
        encode_to_wav(&samples_vec, self.target_sample_rate, self.target_channels)
    }
}

/// Normalizes audio amplitude to ensure soft/whispered speech is recognized accurately
pub fn normalize_audio_gain(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }
    let mut peak: f32 = 0.0;
    for &s in samples.iter() {
        let abs = s.abs();
        if abs > peak {
            peak = abs;
        }
    }
    // If speech is soft (peak < 0.65) and above noise floor (peak > 0.005), apply gentle gain boost
    if peak > 0.005 && peak < 0.65 {
        let gain = (0.85 / peak).min(5.0); // max 5x boost
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

/// Downmixes multi-channel audio to mono and resamples to target sample rate using linear interpolation
pub fn resample_and_downmix(
    input: &[f32],
    in_rate: u32,
    out_rate: u32,
    channels: u16,
) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }

    // Step 1: Downmix channels to mono
    let mono: Vec<f32> = if channels > 1 {
        let ch = channels as usize;
        input
            .chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        input.to_vec()
    };

    // Step 2: Resample if sample rates differ
    if in_rate == out_rate {
        return mono;
    }

    let ratio = in_rate as f64 / out_rate as f64;
    let out_len = ((mono.len() as f64) / ratio).floor() as usize;
    let mut resampled = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let frac = (src_idx - idx0 as f64) as f32;

        let s0 = if idx0 < mono.len() { mono[idx0] } else { 0.0 };
        let s1 = if idx0 + 1 < mono.len() { mono[idx0 + 1] } else { s0 };

        resampled.push(s0 + frac * (s1 - s0));
    }

    resampled
}

fn encode_to_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>, String> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer =
            WavWriter::new(&mut buf, spec).map_err(|e| format!("WAV writer error: {e}"))?;
        for &sample in samples {
            let s = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(s)
                .map_err(|e| format!("WAV write error: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("WAV finalize error: {e}"))?;
    }

    Ok(buf.into_inner())
}

fn get_input_device_and_config(
    host: &cpal::Host,
) -> Result<(cpal::Device, cpal::SupportedStreamConfig), String> {
    // 1. Try default input device
    if let Some(device) = host.default_input_device() {
        if let Ok(cfg) = device.default_input_config() {
            return Ok((device, cfg));
        }
        if let Ok(mut configs) = device.supported_input_configs() {
            if let Some(cfg) = configs.next() {
                return Ok((device, cfg.with_max_sample_rate()));
            }
        }
    }

    // 2. Fallback: Iterate all available input devices
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".into());
            if let Ok(cfg) = device.default_input_config() {
                println!("[recorder] Using input device '{}' (default config)", name);
                return Ok((device, cfg));
            }
            if let Ok(mut configs) = device.supported_input_configs() {
                if let Some(cfg) = configs.next() {
                    println!("[recorder] Using input device '{}' (supported config)", name);
                    return Ok((device, cfg.with_max_sample_rate()));
                }
            }
        }
    }

    Err("No working audio input device with a valid stream configuration was found. Please check Microphone permissions in macOS System Settings -> Privacy & Security -> Microphone.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_stereo_to_mono_48k_to_16k() {
        // Create 1 second of stereo 48000Hz sine wave (96000 samples)
        let sample_count = 48000 * 2;
        let mut input = Vec::with_capacity(sample_count);
        for i in 0..48000 {
            let s = (i as f32 * 0.05).sin();
            input.push(s); // Left
            input.push(s); // Right
        }

        let out = resample_and_downmix(&input, 48000, 16000, 2);
        assert_eq!(out.len(), 16000);
    }

    #[test]
    fn test_resample_same_rate() {
        let input = vec![0.1, 0.2, 0.3, 0.4];
        let out = resample_and_downmix(&input, 16000, 16000, 1);
        assert_eq!(out, input);
    }

    #[test]
    fn test_empty_input() {
        let input: Vec<f32> = Vec::new();
        let out = resample_and_downmix(&input, 48000, 16000, 2);
        assert!(out.is_empty());
    }
}
