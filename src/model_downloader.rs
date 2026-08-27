//! In-App Automatic Whisper Model Downloader & Manager
//!
//! Provides automated downloading, verification, and configuration for local whisper.cpp models
//! from Hugging Face repositories with streaming progress tracking.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WhisperModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub size_mb: u64,
}

pub const WHISPER_MODELS: &[WhisperModelInfo] = &[
    WhisperModelInfo {
        id: "small.en",
        name: "Whisper Small (Recommended — 466 MB)",
        filename: "ggml-small.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        size_mb: 466,
    },
    WhisperModelInfo {
        id: "base.en",
        name: "Whisper Base (Fast — 140 MB)",
        filename: "ggml-base.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        size_mb: 140,
    },
    WhisperModelInfo {
        id: "tiny.en",
        name: "Whisper Tiny (Ultra-Fast — 75 MB)",
        filename: "ggml-tiny.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        size_mb: 75,
    },
    WhisperModelInfo {
        id: "medium.en",
        name: "Whisper Medium (High-Accuracy — 1.5 GB)",
        filename: "ggml-medium.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        size_mb: 1530,
    },
];

/// Returns the default directory where local models are stored (`~/.config/keryx/models/`)
pub fn get_models_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".config").join("keryx").join("models");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        dir
    } else {
        PathBuf::from("models")
    }
}

/// Resolves the absolute path for a given model filename
pub fn get_model_path(filename: &str) -> PathBuf {
    // 1. Check primary keryx models dir
    let keryx_path = get_models_dir().join(filename);
    if keryx_path.exists() {
        return keryx_path;
    }

    // 2. Check legacy wisprflow models dir if exists
    if let Some(home) = dirs::home_dir() {
        let legacy_path = home.join(".config").join("wisprflow").join("models").join(filename);
        if legacy_path.exists() {
            return legacy_path;
        }
    }

    keryx_path
}

/// Checks whether a model file exists on disk and is at least 1MB
pub fn is_model_installed(filename: &str) -> bool {
    let path = get_model_path(filename);
    if path.exists() {
        if let Ok(meta) = fs::metadata(&path) {
            return meta.len() > 1_000_000;
        }
    }
    false
}

/// Finds a model by its filename or id
pub fn find_model_info(query: &str) -> Option<&'static WhisperModelInfo> {
    WHISPER_MODELS.iter().find(|m| {
        m.id == query
            || m.filename == query
            || m.name.to_lowercase().contains(&query.to_lowercase())
            || query.contains(m.filename)
    })
}

/// Downloads a Whisper model with live streaming progress updates
///
/// Callback signature: `progress_callback(fraction_0_to_1, downloaded_bytes, total_bytes)`
pub async fn download_model_streaming<F>(
    model_info: &WhisperModelInfo,
    cancel_flag: Arc<AtomicBool>,
    mut progress_cb: F,
) -> Result<PathBuf, String>
where
    F: FnMut(f64, u64, u64) + Send + 'static,
{
    let target_dir = get_models_dir();
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create models directory {:?}: {}", target_dir, e))?;
    }

    let final_path = target_dir.join(model_info.filename);
    let tmp_path = target_dir.join(format!("{}.tmp", model_info.filename));

    println!("[model-downloader] Downloading {} from {}...", model_info.filename, model_info.url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client init failed: {}", e))?;

    let mut response = client
        .get(model_info.url)
        .header("User-Agent", "Keryx-Desktop-App")
        .send()
        .await
        .map_err(|e| format!("Failed to connect to model download URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download server returned HTTP error {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    let total_size = response
        .content_length()
        .unwrap_or(model_info.size_mb * 1024 * 1024);

    let mut file = File::create(&tmp_path)
        .map_err(|e| format!("Failed to create temporary download file {:?}: {}", tmp_path, e))?;

    let mut downloaded: u64 = 0;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Error while downloading stream: {}", e))?
    {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = fs::remove_file(&tmp_path);
            return Err("Download cancelled by user".to_string());
        }

        file.write_all(&chunk)
            .map_err(|e| format!("Failed writing to disk: {}", e))?;

        downloaded += chunk.len() as u64;
        let fraction = if total_size > 0 {
            (downloaded as f64 / total_size as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        progress_cb(fraction, downloaded, total_size);
    }

    file.flush()
        .map_err(|e| format!("Failed flushing download file: {}", e))?;
    drop(file);

    // Atomically rename temporary file to final filename
    fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("Failed finalizing model file: {}", e))?;

    println!("[model-downloader] ✓ Download complete: {:?}", final_path);

    // Automatically update .env file
    let mut updates = std::collections::HashMap::new();
    updates.insert("WHISPER_MODEL".to_string(), final_path.to_string_lossy().to_string());
    let _ = crate::settings_gui::save_env_file(&updates);

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_model_info() {
        assert!(find_model_info("ggml-small.en.bin").is_some());
        assert!(find_model_info("small.en").is_some());
        assert!(find_model_info("tiny.en").is_some());
        assert!(find_model_info("nonexistent").is_none());
    }

    #[test]
    fn test_models_dir() {
        let dir = get_models_dir();
        assert!(dir.to_string_lossy().contains("models"));
    }
}
