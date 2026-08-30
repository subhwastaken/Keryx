pub mod app_context;
pub mod audio_feedback;
pub mod config;
pub mod hud;
pub mod hud_overlay;
pub mod llm;
pub mod macos_hotkey;
pub mod model_downloader;
pub mod paster;
pub mod recorder;
pub mod settings_gui;
pub mod smart_spacing;
pub mod spoken_formatting;
pub mod transcriber;
pub mod tts;
pub mod vad;

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    HotkeyPress,
    HotkeyRelease,
    DoubleTap,
    Cancel,
    AutoStop,
}
