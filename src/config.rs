use std::env;
use std::path::PathBuf;
use dirs::home_dir;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Config {
    // NVIDIA
    pub nvidia_api_key: Option<String>,
    pub nvidia_stt_model: String,
    pub nvidia_llm_model: String,
    pub nvidia_tts_model: String,

    // Other providers
    pub groq_api_key: Option<String>,
    pub openai_api_key: Option<String>,

    // Provider selections
    pub transcription_provider: TranscriptionProvider,
    pub llm_provider: LlmProvider,
    pub ai_postprocessing: bool,
    pub tts_provider: TtsProvider,

    // macOS TTS
    pub macos_tts_voice: String,

    // Hotkey
    pub hotkey: String,
    pub double_tap_ms: u64,
    pub auto_stop_secs: u64,

    // Local whisper
    pub whisper_bin: PathBuf,
    pub whisper_model: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptionProvider {
    Nvidia,
    Groq,
    OpenAI,
    /// whisper.cpp binary (Metal on Apple Silicon, CUDA on Windows)
    Local,
    /// Auto: local whisper.cpp if installed, else cloud
    Auto,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    Nvidia,
    Groq,
    OpenAI,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TtsProvider {
    Auto,
    MacOS,
    Nvidia,
    None,
}

impl Config {
    pub fn load() -> Self {
        // Try loading from ~/.config/keryx/.env or legacy ~/.config/wisprflow/.env
        let config_env = home_dir().and_then(|h| {
            let keryx_env = h.join(".config/keryx/.env");
            if keryx_env.exists() {
                Some(keryx_env)
            } else {
                let legacy_env = h.join(".config/wisprflow/.env");
                if legacy_env.exists() {
                    Some(legacy_env)
                } else {
                    None
                }
            }
        });

        // Also try local .env
        let local_env = std::path::Path::new(".env");

        if let Some(path) = config_env {
            let _ = dotenvy::from_path(&path);
        } else if local_env.exists() {
            let _ = dotenvy::dotenv();
        }

        let transcription_provider = match env::var("TRANSCRIPTION_PROVIDER")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "groq" => TranscriptionProvider::Groq,
            "openai" => TranscriptionProvider::OpenAI,
            "local" => TranscriptionProvider::Local,
            "nvidia" => TranscriptionProvider::Nvidia,
            _ => TranscriptionProvider::Auto, // default: auto-detect
        };

        let llm_provider = match env::var("LLM_PROVIDER")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "groq" => LlmProvider::Groq,
            "openai" => LlmProvider::OpenAI,
            "none" => LlmProvider::None,
            _ => LlmProvider::Nvidia, // default
        };

        let ai_postprocessing = match env::var("AI_POSTPROCESSING").ok().as_deref() {
            Some("false") | Some("0") | Some("no") | Some("off") => false,
            Some("true") | Some("1") | Some("yes") | Some("on") => true,
            _ => llm_provider != LlmProvider::None,
        };

        let tts_provider = match env::var("TTS_PROVIDER")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "macos" => TtsProvider::MacOS,
            "nvidia" => TtsProvider::Nvidia,
            "none" => TtsProvider::None,
            _ => TtsProvider::Auto, // default
        };

        let expand_path = |s: String| -> PathBuf {
            if s.starts_with('~') {
                if let Some(home) = home_dir() {
                    // Handle both "~" and "~/something"
                    let rest = s.trim_start_matches('~').trim_start_matches('/');
                    if rest.is_empty() {
                        return home;
                    }
                    return home.join(rest);
                }
            }
            PathBuf::from(s)
        };

        Config {
            nvidia_api_key: env::var("NVIDIA_API_KEY").ok().filter(|s| !s.is_empty()),
            nvidia_stt_model: env::var("NVIDIA_STT_MODEL")
                .unwrap_or_else(|_| "nvidia/parakeet-ctc-1.1b".to_string()),
            nvidia_llm_model: env::var("NVIDIA_LLM_MODEL")
                .unwrap_or_else(|_| "meta/llama-3.2-11b-vision-instruct".to_string()),
            nvidia_tts_model: env::var("NVIDIA_TTS_MODEL")
                .unwrap_or_else(|_| "nvidia/fastpitch-hifigan-tts".to_string()),

            groq_api_key: env::var("GROQ_API_KEY").ok().filter(|s| !s.is_empty()),
            openai_api_key: env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty()),

            transcription_provider,
            llm_provider,
            ai_postprocessing,
            tts_provider,

            macos_tts_voice: env::var("MACOS_TTS_VOICE")
                .unwrap_or_else(|_| "Samantha".to_string()),

            hotkey: env::var("HOTKEY").unwrap_or_else(|_| "right_alt".to_string()),
            double_tap_ms: env::var("DOUBLE_TAP_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(400),
            auto_stop_secs: env::var("AUTO_STOP_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),

            whisper_bin: expand_path(
                env::var("WHISPER_BIN")
                    .unwrap_or_else(|_| {
                        let keryx_bin = expand_path("~/.config/keryx/whisper.cpp/build/bin/whisper-cli".to_string());
                        if keryx_bin.exists() {
                            "~/.config/keryx/whisper.cpp/build/bin/whisper-cli".to_string()
                        } else {
                            "~/.config/wisprflow/whisper.cpp/build/bin/whisper-cli".to_string()
                        }
                    }),
            ),
            whisper_model: {
                if let Ok(model_env) = env::var("WHISPER_MODEL") {
                    expand_path(model_env)
                } else {
                    let keryx_model = expand_path("~/.config/keryx/models/ggml-small.en.bin".to_string());
                    let legacy_model = expand_path("~/.config/wisprflow/models/ggml-small.en.bin".to_string());
                    if keryx_model.exists() {
                        keryx_model
                    } else if legacy_model.exists() {
                        legacy_model
                    } else {
                        expand_path("~/.config/keryx/models/ggml-small.bin".to_string())
                    }
                }
            },
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let expand_path = |s: String| -> PathBuf {
            if s.starts_with('~') {
                if let Some(home) = home_dir() {
                    let rest = s.trim_start_matches('~').trim_start_matches('/');
                    if rest.is_empty() {
                        return home;
                    }
                    return home.join(rest);
                }
            }
            PathBuf::from(s)
        };

        Config {
            nvidia_api_key: None,
            nvidia_stt_model: "nvidia/parakeet-ctc-1.1b".to_string(),
            nvidia_llm_model: "meta/llama-3.2-11b-vision-instruct".to_string(),
            nvidia_tts_model: "nvidia/fastpitch-hifigan-tts".to_string(),
            groq_api_key: None,
            openai_api_key: None,
            transcription_provider: TranscriptionProvider::Auto,
            llm_provider: LlmProvider::Nvidia,
            ai_postprocessing: true,
            tts_provider: TtsProvider::Auto,
            macos_tts_voice: "Samantha".to_string(),
            hotkey: "right_alt".to_string(),
            double_tap_ms: 400,
            auto_stop_secs: 300,
            whisper_bin: expand_path("~/.config/keryx/whisper.cpp/build/bin/whisper-cli".to_string()),
            whisper_model: expand_path("~/.config/keryx/models/ggml-small.en.bin".to_string()),
        }
    }
}
