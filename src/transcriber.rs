use crate::config::{Config, TranscriptionProvider};
use reqwest::multipart;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TranscriptionResponse {
    text: String,
}

pub async fn transcribe(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    if wav_bytes.is_empty() {
        return Err("No audio data to transcribe".to_string());
    }

    match &config.transcription_provider {
        TranscriptionProvider::Auto => transcribe_auto(wav_bytes, config).await,
        TranscriptionProvider::Nvidia => transcribe_nvidia(wav_bytes, config).await,
        TranscriptionProvider::Groq => transcribe_groq(wav_bytes, config).await,
        TranscriptionProvider::OpenAI => transcribe_openai(wav_bytes, config).await,
        TranscriptionProvider::Local => transcribe_local(wav_bytes, config),
    }
}

/// Auto mode: tries local whisper.cpp (free, offline, cross-platform) if installed, else cloud fallback
async fn transcribe_auto(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    // 1. Try local whisper.cpp first if binary + model both exist
    if config.whisper_bin.exists() && config.whisper_model.exists() {
        println!("[auto-stt] Using offline whisper.cpp (GPU-accelerated, local)...");
        return transcribe_local(wav_bytes, config);
    }

    #[cfg(target_os = "macos")]
    {
        if !config.whisper_bin.exists() {
            println!("[auto-stt] Local whisper.cpp binary not found at {:?}", config.whisper_bin);
        } else if !config.whisper_model.exists() {
            println!("[auto-stt] Whisper model file not found at {:?}", config.whisper_model);
        }
    }

    // 2. Fall back to Groq if key is set
    if config.groq_api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        println!("[auto-stt] Falling back to Groq Whisper (free cloud)...");
        return transcribe_groq(wav_bytes, config).await;
    }

    // 3. Fall back to NVIDIA
    if config.nvidia_api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        println!("[auto-stt] Falling back to NVIDIA STT...");
        return transcribe_nvidia(wav_bytes, config).await;
    }

    // 4. Fall back to OpenAI
    if config.openai_api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
        println!("[auto-stt] Falling back to OpenAI Whisper...");
        return transcribe_openai(wav_bytes, config).await;
    }

    Err("No STT provider available.\n\nOptions:\n  1. Install whisper.cpp locally (free, offline): run `make setup`\n  2. Add GROQ_API_KEY to ~/.config/keryx/.env (free tier at console.groq.com)\n  3. Add NVIDIA_API_KEY to ~/.config/keryx/.env".to_string())
}

/// NVIDIA Build — uses OpenAI-compatible Whisper endpoint
async fn transcribe_nvidia(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    let api_key = config
        .nvidia_api_key
        .as_ref()
        .ok_or("NVIDIA_API_KEY not set in ~/.config/keryx/.env")?;

    println!("[nvidia-stt] Uploading {} bytes to NVIDIA Build...", wav_bytes.len());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let part = multipart::Part::bytes(wav_bytes.clone())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = multipart::Form::new()
        .part("file", part)
        .text("model", "openai/whisper-large-v3")
        .text("language", "en")
        .text("prompt", "Technical speech, APIs, code, punctuation, Keryx, macOS.")
        .text("response_format", "json");

    let response = client
        .post("https://integrate.api.nvidia.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("NVIDIA STT network error: {e}"))?;

    let status = response.status();
    println!("[nvidia-stt] HTTP status: {status}");

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        println!("[nvidia-stt] Error body: {body}");

        // If NVIDIA STT fails, fall back to Groq if key is set
        if let Some(groq_key) = &config.groq_api_key {
            if !groq_key.is_empty() {
                println!("[nvidia-stt] Falling back to Groq STT with actual audio...");
                return transcribe_with_groq_key(wav_bytes, groq_key).await;
            }
        }

        // If local whisper is available, fall back to it
        if config.whisper_bin.exists() && config.whisper_model.exists() {
            println!("[nvidia-stt] Falling back to local whisper.cpp...");
            return transcribe_local(wav_bytes, config);
        }

        return Err(format!(
            "NVIDIA STT error {status}: {body}\n\nTip: Set TRANSCRIPTION_PROVIDER=local or GROQ_API_KEY in ~/.config/keryx/.env"
        ));
    }

    let body = response.text().await.map_err(|e| format!("NVIDIA STT read error: {e}"))?;
    println!("[nvidia-stt] Response: {body}");

    let result: TranscriptionResponse = serde_json::from_str(&body)
        .map_err(|e| format!("NVIDIA STT parse error: {e}\nBody was: {body}"))?;

    Ok(result.text.trim().to_string())
}

/// Groq Whisper transcription (very fast, free tier)
async fn transcribe_groq(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    let api_key = config
        .groq_api_key
        .as_ref()
        .ok_or("GROQ_API_KEY not set — get a free key at console.groq.com")?;

    transcribe_with_groq_key(wav_bytes, api_key).await
}

async fn transcribe_with_groq_key(wav_bytes: Vec<u8>, api_key: &str) -> Result<String, String> {
    if wav_bytes.is_empty() {
        return Err("No audio data".to_string());
    }

    println!("[groq-stt] Uploading {} bytes to Groq...", wav_bytes.len());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let part = multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-large-v3")
        .text("prompt", "Technical speech, APIs, code, punctuation, Keryx, macOS.")
        .text("response_format", "json");

    let response = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Groq STT network error: {e}"))?;

    let status = response.status();
    println!("[groq-stt] HTTP status: {status}");

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Groq STT error {status}: {body}"));
    }

    let result: TranscriptionResponse = response
        .json()
        .await
        .map_err(|e| format!("Groq STT parse error: {e}"))?;

    Ok(result.text.trim().to_string())
}

/// OpenAI Whisper transcription
async fn transcribe_openai(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    let api_key = config
        .openai_api_key
        .as_ref()
        .ok_or("OPENAI_API_KEY not set")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let part = multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1");

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("OpenAI STT network error: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI STT error {status}: {body}"));
    }

    let result: TranscriptionResponse = response
        .json()
        .await
        .map_err(|e| format!("OpenAI STT parse error: {e}"))?;

    Ok(result.text.trim().to_string())
}

/// Local whisper.cpp binary execution
fn transcribe_local(wav_bytes: Vec<u8>, config: &Config) -> Result<String, String> {
    use std::io::{Read, Write};

    if wav_bytes.len() < 8000 {
        return Ok(String::new());
    }

    if !config.whisper_bin.exists() {
        return Err(format!(
            "whisper.cpp binary not found at {:?}\nRun `make setup` or set WHISPER_BIN in ~/.config/keryx/.env",
            config.whisper_bin
        ));
    }

    let effective_model_path = if config.whisper_model.exists() {
        config.whisper_model.clone()
    } else {
        // Look up any model installed in ~/.config/keryx/models/ or ~/.config/wisprflow/models/
        crate::model_downloader::WHISPER_MODELS
            .iter()
            .map(|m| crate::model_downloader::get_model_path(m.filename))
            .find(|p| p.exists() && std::fs::metadata(p).map(|meta| meta.len() > 1_000_000).unwrap_or(false))
            .unwrap_or_else(|| config.whisper_model.clone())
    };

    if !effective_model_path.exists() {
        return Err(format!(
            "Whisper model not found at {:?}\nDownload a model in Settings or run `make setup`",
            config.whisper_model
        ));
    }

    // Write wav to temp file (cross-platform)
    let mut tmp = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile()
        .map_err(|e| format!("Failed to create temp audio file: {e}"))?;
    tmp.write_all(&wav_bytes).map_err(|e| format!("Failed to write audio: {e}"))?;
    let tmp_path = tmp.path().to_path_buf();

    println!(
        "[local-stt] Running whisper.cpp with model {:?} on {} bytes...",
        effective_model_path.file_name().unwrap_or_default(),
        wav_bytes.len()
    );

    let bin_dir = config.whisper_bin.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut cmd = std::process::Command::new(&config.whisper_bin);
    cmd.current_dir(bin_dir);

    // Set platform-appropriate dynamic library path
    #[cfg(target_os = "macos")]
    {
        cmd.env("DYLD_LIBRARY_PATH", bin_dir);
        cmd.env("DYLD_FALLBACK_LIBRARY_PATH", bin_dir);
    }
    #[cfg(target_os = "linux")]
    {
        cmd.env("LD_LIBRARY_PATH", bin_dir);
    }
    // Windows: DLLs are found via PATH or same directory — no special env needed

    // Adaptive thread count: uses available performance cores (4 to 8) without overloading CPU/battery
    let thread_count = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(4, 8))
        .unwrap_or(4)
        .to_string();

    let model_str = effective_model_path.to_string_lossy();
    let lang = if model_str.contains(".en.") || model_str.ends_with(".en.bin") {
        "en"
    } else {
        "auto"
    };

    cmd.args([
        "-m", effective_model_path.to_str().unwrap_or(""),
        "-f", tmp_path.to_str().unwrap_or(""),
        "-t", &thread_count,
        "--prompt", "Technical speech, APIs, code, punctuation, Keryx, macOS.",
        "--no-timestamps",
        "--no-prints",
        "-l", lang,
    ]);

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("whisper.cpp exec error: {e}"))?;

    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30);

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start_time.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("whisper.cpp process timed out after 30s".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("whisper.cpp wait error: {e}"));
            }
        }
    };

    let mut stdout_buf = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut stdout_buf);
    }

    if !status.success() {
        let mut err_msg = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut err_msg);
        }
        return Err(format!("whisper.cpp exited with status {status}: {err_msg}"));
    }

    let stdout_text = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    if !stdout_text.is_empty() && !stdout_text.eq_ignore_ascii_case("[blank_audio]") {
        return Ok(stdout_text);
    }

    Ok(String::new())
}

