#![allow(dead_code)]
use crate::config::{Config, TtsProvider};
use serde_json::json;
use std::sync::OnceLock;

fn get_tts_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_nodelay(true)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

pub async fn speak(text: &str, config: &Config) {
    let provider = resolve_provider(&config.tts_provider);

    let result = match provider {
        ResolvedTts::MacOS => speak_macos(text, &config.macos_tts_voice),
        ResolvedTts::Nvidia => speak_nvidia(text, config).await,
        ResolvedTts::None => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("[Keryx] TTS error: {e}");
    }
}

enum ResolvedTts {
    MacOS,
    Nvidia,
    None,
}

fn resolve_provider(provider: &TtsProvider) -> ResolvedTts {
    match provider {
        TtsProvider::MacOS => ResolvedTts::MacOS,
        TtsProvider::Nvidia => ResolvedTts::Nvidia,
        TtsProvider::None => ResolvedTts::None,
        TtsProvider::Auto => {
            #[cfg(target_os = "macos")]
            return ResolvedTts::MacOS;
            #[cfg(not(target_os = "macos"))]
            return ResolvedTts::Nvidia;
        }
    }
}

/// macOS native TTS using the `say` command — offline, zero latency, free
fn speak_macos(text: &str, voice: &str) -> Result<(), String> {
    // Sanitize voice name to ensure only alphanumeric, spaces, and hyphens
    let safe_voice = if voice.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_') && !voice.is_empty() {
        voice
    } else {
        "Samantha"
    };

    let status = std::process::Command::new("say")
        .arg("-v")
        .arg(safe_voice)
        .arg(text)
        .status()
        .map_err(|e| format!("say command failed: {e}"))?;

    if !status.success() {
        return Err(format!("say exited with status: {status}"));
    }

    Ok(())
}

/// NVIDIA Build TTS API
async fn speak_nvidia(text: &str, config: &Config) -> Result<(), String> {
    let api_key = config
        .nvidia_api_key
        .as_ref()
        .ok_or("NVIDIA_API_KEY not set")?;

    let client = get_tts_http_client();

    let body = json!({
        "model": config.nvidia_tts_model,
        "input": text,
        "voice": "english-us.female-1",
        "response_format": "wav"
    });

    let response = client
        .post("https://integrate.api.nvidia.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("NVIDIA TTS request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("NVIDIA TTS error {status}: {body}"));
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("NVIDIA TTS read error: {e}"))?;

    // Play audio bytes using system player
    play_audio_bytes(&audio_bytes)
}

fn play_audio_bytes(bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    tmp.write_all(bytes).map_err(|e| e.to_string())?;
    let tmp_path = tmp.path().to_path_buf();

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("afplay")
            .arg(&tmp_path)
            .status()
            .map_err(|e| format!("afplay failed: {e}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    tmp_path.to_str().unwrap_or("")
                ),
            ])
            .status()
            .map_err(|e| format!("PowerShell audio failed: {e}"))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("aplay")
            .arg(&tmp_path)
            .status()
            .map_err(|e| format!("aplay failed: {e}"))?;
    }

    Ok(())
}
