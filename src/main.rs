mod app_context;
mod config;
mod hud;
mod hud_overlay;
mod llm;
#[cfg(target_os = "macos")]
mod macos_hotkey;
mod model_downloader;
mod paster;
mod recorder;
mod settings_gui;
mod smart_spacing;
mod spoken_formatting;
mod transcriber;
mod tts;

use config::Config;
use fs2::FileExt;
use hud_overlay::HudOverlay;
use parking_lot::Mutex;
use recorder::Recorder;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    Idle,
    Recording,
    HandsFree,
    Transcribing,
}

pub use keryx::AppEvent;

struct SessionController {
    state: AppState,
    current_session: u64,
    stop_flag: Arc<AtomicBool>,
}

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn main() {
    env_logger::init();
    println!("🎙 Keryx (Rust) starting...");

    // Single instance lock check using fs2
    let lock_file_path = std::env::temp_dir().join("keryx_lockfile.lock");
    let lock_file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[Keryx] Warning: Could not open lockfile: {e}");
            std::fs::File::create(&lock_file_path).expect("Failed to create lockfile")
        }
    };

    if lock_file.try_lock_exclusive().is_err() {
        println!("⚠️  Keryx is already running in your menu bar. Exiting duplicate instance.");
        hud::notify(
            "Keryx Active 🎙",
            "Keryx is already running in your top menu bar. Hold your hotkey to dictate.",
        );
        return;
    }

    // Keep lock_file alive throughout process execution
    let _lock = lock_file;

    // Ensure config dir and default .env exist before loading
    setup_config_dir();
    let config = Arc::new(Config::load());
    println!("✓ Config loaded:");
    println!("  STT provider : {:?}", config.transcription_provider);
    println!("  LLM provider : {:?}", config.llm_provider);
    println!("  TTS provider : {:?}", config.tts_provider);
    println!("  Hotkey       : {}", config.hotkey);
    println!("  NVIDIA key   : {}", if config.nvidia_api_key.is_some() { "✓ set" } else { "✗ NOT SET" });
    println!("  Groq key     : {}", if config.groq_api_key.is_some() { "✓ set" } else { "✗ NOT SET" });

    // On macOS, configure NSApp as Accessory (menu bar)
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicyAccessory};
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
        app.finishLaunching();
    }

    // Check/prompt permissions ONCE (cached by macOS after grant)
    #[cfg(target_os = "macos")]
    check_and_prompt_accessibility();

    #[cfg(target_os = "macos")]
    check_and_prompt_microphone();

    // Channel for keyboard events to async Tokio runtime
    let (tx, rx) = mpsc::unbounded_channel::<AppEvent>();

    let session_ctrl = Arc::new(Mutex::new(SessionController {
        state: AppState::Idle,
        current_session: 0,
        stop_flag: Arc::new(AtomicBool::new(false)),
    }));

    let key_held = Arc::new(Mutex::new(false));
    let last_release: Arc<Mutex<Option<std::time::Instant>>> = Arc::new(Mutex::new(None));

    let hud_overlay = Arc::new(HudOverlay::new());

    // Start Tokio async runtime in background thread
    let config_bg = config.clone();
    let ctrl_bg = session_ctrl.clone();
    let tx_bg = tx.clone();
    let overlay_bg = hud_overlay.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to start Tokio runtime");
        rt.block_on(run_event_loop(rx, ctrl_bg, tx_bg, config_bg, overlay_bg));
    });

    // ── Pre-warm network and models in background (eliminates cold-start lag) ──
    let config_warm = config.clone();
    thread::spawn(move || {
        // 1. Touch/mmap whisper model into filesystem cache
        if config_warm.whisper_model.exists() {
            if let Ok(mut f) = std::fs::File::open(&config_warm.whisper_model) {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                let _ = f.read_exact(&mut buf);
            }
        }
        // 2. Pre-warm HTTP/TLS connection to NVIDIA
        if let Some(key) = &config_warm.nvidia_api_key {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
            if let Ok(rt) = rt {
                rt.block_on(async {
                    let client = reqwest::Client::builder()
                        .tcp_nodelay(true)
                        .timeout(std::time::Duration::from_secs(3))
                        .build();
                    if let Ok(c) = client {
                        let _ = c.head("https://integrate.api.nvidia.com")
                            .header("Authorization", format!("Bearer {key}"))
                            .send()
                            .await;
                    }
                });
            }
        }
    });

    // Build system tray icon with menu event handler
    let (_tray, settings_menu_id, quit_menu_id) = build_tray();

    // Spawn tray menu click listener
    let overlay_tray = hud_overlay.clone();
    let ctrl_tray = session_ctrl.clone();
    thread::spawn(move || {
        let menu_rx = MenuEvent::receiver();
        while let Ok(event) = menu_rx.recv() {
            if event.id == settings_menu_id {
                println!("[tray] Opening native settings window...");
                #[cfg(target_os = "macos")]
                settings_gui::open_settings_window();
                #[cfg(not(target_os = "macos"))]
                {
                    if let Some(home) = dirs::home_dir() {
                        let env_path = home.join(".config/keryx/.env");
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("notepad").arg(&env_path).spawn();
                        #[cfg(target_os = "linux")]
                        let _ = std::process::Command::new("xdg-open").arg(&env_path).spawn();
                    }
                }
            } else if event.id == quit_menu_id {
                println!("[tray] User clicked 'Quit Keryx'. Exiting...");
                overlay_tray.hide();
                let ctrl = ctrl_tray.lock();
                ctrl.stop_flag.store(true, Ordering::SeqCst);
                #[cfg(target_os = "macos")]
                unsafe {
                    use objc::{msg_send, sel, sel_impl};
                    let app = cocoa::appkit::NSApp();
                    let _: () = msg_send![app, terminate: cocoa::base::nil];
                }
                #[cfg(not(target_os = "macos"))]
                std::process::exit(0);
            }
        }
    });

    println!("\n✅ Keryx ready! Hold [{}] to dictate.", config.hotkey);
    println!("   Release to transcribe. Double-tap for hands-free. Esc to cancel.\n");

    // ── PRODUCTION HOTKEY LISTENER ────────────────────────────────────────────
    // On macOS we use CGEventTap attached to the MAIN THREAD's RunLoop.
    // ─────────────────────────────────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    {
        let hotkey_config = config.hotkey.clone();
        let double_tap_ms = config.double_tap_ms;

        let tx_for_tap = tx.clone();
        let key_held_for_tap = key_held.clone();
        let last_release_for_tap = last_release.clone();

        let keycodes = macos_hotkey::cgeventtap::hotkey_str_to_keycodes(&hotkey_config);
        println!("[hotkey] Installing CGEventTap for keycodes: {:?}", keycodes);

        let tx_arc: Arc<dyn Fn(AppEvent) + Send + Sync> = Arc::new(move |ev| {
            let _ = tx_for_tap.send(ev);
        });

        let tap_state = macos_hotkey::cgeventtap::TapState {
            keycodes,
            tx_key: tx_arc,
            key_held: key_held_for_tap,
            last_release: last_release_for_tap,
            double_tap_ms,
        };

        // Install CGEventTap on main RunLoop BEFORE entering Cocoa app.run()
        macos_hotkey::cgeventtap::install(tap_state);

        // Run Cocoa main event loop on main thread (services both AppKit + our CGEventTap source)
        unsafe {
            use cocoa::appkit::{NSApp, NSApplication};
            let app = NSApp();
            app.run();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        use rdev::{listen, Event, EventType, Key};

        let hotkey_config = config.hotkey.clone();
        let double_tap_ms = config.double_tap_ms;
        let tx_key = tx.clone();
        let tx_esc = tx.clone();

        thread::spawn(move || {
            println!("✓ Hotkey listener thread active (rdev).");
            loop {
                let tx_key_c = tx_key.clone();
                let tx_esc_c = tx_esc.clone();
                let key_held_c = key_held.clone();
                let last_release_c = last_release.clone();
                let hk_cfg = hotkey_config.clone();

                let result = listen(move |event: Event| {
                    match event.event_type {
                        EventType::KeyPress(key) if matches_hotkey_rdev(key, &hk_cfg) => {
                            let mut held = key_held_c.lock();
                            if !*held {
                                *held = true;
                                let is_double_tap = {
                                    let last = last_release_c.lock();
                                    last.map(|t| t.elapsed() < Duration::from_millis(double_tap_ms))
                                        .unwrap_or(false)
                                };
                                if is_double_tap {
                                    let _ = tx_key_c.send(AppEvent::DoubleTap);
                                } else {
                                    let _ = tx_key_c.send(AppEvent::HotkeyPress);
                                }
                            }
                        }
                        EventType::KeyRelease(key) if matches_hotkey_rdev(key, &hk_cfg) => {
                            let mut held = key_held_c.lock();
                            if *held {
                                *held = false;
                                *last_release_c.lock() = Some(std::time::Instant::now());
                                let _ = tx_key_c.send(AppEvent::HotkeyRelease);
                            }
                        }
                        EventType::KeyPress(Key::Escape) => {
                            let _ = tx_esc_c.send(AppEvent::Cancel);
                        }
                        _ => {}
                    }
                });

                if let Err(err) = result {
                    println!("[hotkey] ⚠️  Hotkey listener error: {:?}. Retrying...", err);
                    thread::sleep(Duration::from_secs(3));
                } else {
                    break;
                }
            }
        });

        loop {
            thread::park();
        }
    }
}

/// Cross-platform rdev hotkey matcher (used on Windows/Linux only)
#[cfg(not(target_os = "macos"))]
fn matches_hotkey_rdev(key: rdev::Key, hotkey_config: &str) -> bool {
    use rdev::Key;
    match hotkey_config.to_lowercase().as_str() {
        "right_alt" | "right_option" => matches!(key, Key::AltGr | Key::Alt),
        "right_shift" | "shift_right" | "shift" => matches!(key, Key::ShiftRight | Key::ShiftLeft),
        "left_shift" | "shift_left" => matches!(key, Key::ShiftLeft),
        "right_ctrl" | "right_control" | "ctrl" => matches!(key, Key::ControlRight | Key::ControlLeft),
        "caps_lock" | "capslock" => matches!(key, Key::CapsLock),
        _ => matches!(key, Key::AltGr | Key::Alt | Key::ShiftRight | Key::ShiftLeft),
    }
}

async fn run_event_loop(
    mut rx: mpsc::UnboundedReceiver<AppEvent>,
    session_ctrl: Arc<Mutex<SessionController>>,
    tx: mpsc::UnboundedSender<AppEvent>,
    config: Arc<Config>,
    hud_overlay: Arc<HudOverlay>,
) {
    loop {
        let event = rx.recv().await;
        let Some(event) = event else { break };

        let current_state = session_ctrl.lock().state.clone();
        println!("[state] Current: {:?}, Received Event: {:?}", current_state, event);

        match (current_state, event) {
            // ── Idle OR Transcribing: key pressed → Start Hold-to-Talk Recording immediately
            (AppState::Idle | AppState::Transcribing, AppEvent::HotkeyPress) => {
                let my_session = {
                    let mut ctrl = session_ctrl.lock();
                    ctrl.stop_flag.store(true, Ordering::SeqCst);
                    let new_stop = Arc::new(AtomicBool::new(false));
                    ctrl.stop_flag = new_stop.clone();
                    let session_id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
                    ctrl.current_session = session_id;
                    ctrl.state = AppState::Recording;
                    session_id
                };

                println!("[record] Starting microphone (session #{})...", my_session);
                hud_overlay.show("Listening...");

                let stop_flag = {
                    let ctrl = session_ctrl.lock();
                    ctrl.stop_flag.clone()
                };

                start_recording_task(
                    my_session,
                    stop_flag,
                    session_ctrl.clone(),
                    config.clone(),
                    hud_overlay.clone(),
                );
            }

            // ── Idle OR Transcribing: Double-tap → Start Hands-Free Mode
            (AppState::Idle | AppState::Transcribing, AppEvent::DoubleTap) => {
                let my_session = {
                    let mut ctrl = session_ctrl.lock();
                    ctrl.stop_flag.store(true, Ordering::SeqCst);
                    let new_stop = Arc::new(AtomicBool::new(false));
                    ctrl.stop_flag = new_stop.clone();
                    let session_id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
                    ctrl.current_session = session_id;
                    ctrl.state = AppState::HandsFree;
                    session_id
                };

                println!("[hands-free] Started hands-free recording session (#{})...", my_session);
                hud_overlay.show("Hands-free mode");

                let stop_flag = {
                    let ctrl = session_ctrl.lock();
                    ctrl.stop_flag.clone()
                };

                let tx_auto = tx.clone();
                let timeout = config.auto_stop_secs;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(timeout)).await;
                    let _ = tx_auto.send(AppEvent::AutoStop);
                });

                start_recording_task(
                    my_session,
                    stop_flag,
                    session_ctrl.clone(),
                    config.clone(),
                    hud_overlay.clone(),
                );
            }

            // ── Recording: key released → stop recording and transcribe
            (AppState::Recording, AppEvent::HotkeyRelease) => {
                println!("[hotkey] Key released → stopping recording");
                let ctrl = session_ctrl.lock();
                ctrl.stop_flag.store(true, Ordering::SeqCst);
            }

            // ── HandsFree: key pressed or double tapped → stop hands-free recording
            (AppState::HandsFree, AppEvent::HotkeyPress | AppEvent::DoubleTap) => {
                println!("[hands-free] Stopping hands-free mode via hotkey");
                let ctrl = session_ctrl.lock();
                ctrl.stop_flag.store(true, Ordering::SeqCst);
            }

            // ── HandsFree: auto timeout
            (AppState::HandsFree, AppEvent::AutoStop) => {
                println!("[hands-free] Auto-stopped after timeout");
                let ctrl = session_ctrl.lock();
                ctrl.stop_flag.store(true, Ordering::SeqCst);
            }

            // ── Cancel (Esc)
            (_, AppEvent::Cancel) => {
                let mut ctrl = session_ctrl.lock();
                if ctrl.state != AppState::Idle {
                    println!("[cancel] Cancelling current dictation session");
                    ctrl.stop_flag.store(true, Ordering::SeqCst);
                    ctrl.state = AppState::Idle;
                    hud_overlay.hide();
                }
            }

            _ => {}
        }
    }
}

fn start_recording_task(
    my_session: u64,
    stop_flag: Arc<AtomicBool>,
    session_ctrl: Arc<Mutex<SessionController>>,
    config: Arc<Config>,
    hud_overlay: Arc<HudOverlay>,
) {
    tokio::spawn(async move {
        let rec = Recorder::new();
        let stop = stop_flag.clone();

        let overlay_cb = hud_overlay.clone();
        let level_cb: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |lvl| {
            overlay_cb.update_audio_level(lvl);
        });

        let wav_bytes = tokio::task::spawn_blocking(move || {
            rec.record_with_streaming(stop, Some(level_cb))
        })
        .await
        .unwrap_or(Err("Recording thread panicked".to_string()));

        // Atomic check before moving to transcription
        {
            let mut ctrl = session_ctrl.lock();
            if ctrl.current_session != my_session || ctrl.state == AppState::Idle {
                println!("[record] Discarding audio (session was cancelled or superseded)");
                hud_overlay.hide();
                return;
            }
            ctrl.state = AppState::Transcribing;
            hud_overlay.update_text("Transcribing...");
        }

        match wav_bytes {
            Ok(wav) if wav.len() > 1000 => {
                println!("[record] Captured {} bytes of audio, transcribing...", wav.len());
                process_audio(wav, &config, &hud_overlay, my_session, &session_ctrl).await;
            }
            Ok(_) => {
                println!("[record] Audio too short, skipping");
                let mut ctrl = session_ctrl.lock();
                if ctrl.current_session == my_session {
                    ctrl.state = AppState::Idle;
                    hud_overlay.hide();
                }
            }
            Err(e) => {
                eprintln!("[record] Audio error: {e}");
                let mut ctrl = session_ctrl.lock();
                if ctrl.current_session == my_session {
                    ctrl.state = AppState::Idle;
                    hud_overlay.hide();
                }
            }
        }

        // Only reset to Idle / hide if we are still this session in Transcribing state
        {
            let mut ctrl = session_ctrl.lock();
            if ctrl.current_session == my_session && ctrl.state == AppState::Transcribing {
                ctrl.state = AppState::Idle;
                hud_overlay.hide();
            }
        }
        println!("[state] Background processing complete");
    });
}

fn is_non_speech(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.len() < 2 {
        return true;
    }

    // Check if bracketed or parenthesized e.g. "[ Silence ]", "[BLANK_AUDIO]", "(music)"
    let is_bracketed = (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('*') && t.ends_with('*'));
    if is_bracketed {
        return true;
    }

    // Check alphanumeric stripped tokens
    let clean = t.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    matches!(
        clean.as_str(),
        ""
            | "silence"
            | "blank_audio"
            | "music"
            | "applause"
            | "cheering"
            | "laughter"
            | "cough"
            | "bell"
            | "gasp"
            | "snort"
            | "whispering"
            | "snicker"
            | "sigh"
            | "groan"
            | "throat clearing"
            | "thank you for watching"
            | "thanks for watching"
            | "subtitles by"
            | "amara.org"
    )
}

async fn process_audio(
    wav_bytes: Vec<u8>,
    config: &Config,
    hud_overlay: &HudOverlay,
    session_id: u64,
    session_ctrl: &Arc<Mutex<SessionController>>,
) {
    // 1. Transcribe (Instant Whisper STT)
    println!("[transcribe] Calling {:?} STT...", config.transcription_provider);
    let raw_text = match transcriber::transcribe(wav_bytes, config).await {
        Ok(t) => {
            println!("[transcribe] Raw text: {:?}", t);
            t
        }
        Err(e) => {
            eprintln!("[transcribe] STT Error: {e}");
            let is_current = session_ctrl.lock().current_session == session_id;
            if is_current {
                hud_overlay.update_text("Transcription failed");
                tokio::time::sleep(Duration::from_millis(1500)).await;
                if session_ctrl.lock().current_session == session_id {
                    hud_overlay.hide();
                }
            }
            return;
        }
    };

    let trimmed = raw_text.trim();
    if is_non_speech(trimmed) {
        println!("[transcribe] No valid speech detected ({:?}) — silently ignoring", trimmed);
        if session_ctrl.lock().current_session == session_id {
            hud_overlay.hide();
        }
        return;
    }

    let lower = trimmed.to_lowercase();

    // 2. Check for voice deletion commands ("scratch that", "clear text")
    if lower.contains("scratch that") || lower.contains("clear text") || lower.contains("never mind") {
        println!("[voice-command] Detected cancellation/deletion command");
        let _ = paster::backspace_chars(30);
        if session_ctrl.lock().current_session == session_id {
            hud_overlay.update_text("Erased");
            tokio::time::sleep(Duration::from_millis(600)).await;
            if session_ctrl.lock().current_session == session_id {
                hud_overlay.hide();
            }
        }
        return;
    }

    // 3. Post-processing / AI Polish (Instant mode if disabled or None)
    let final_text = if !config.ai_postprocessing || config.llm_provider == config::LlmProvider::None {
        println!("[stt-instant] AI post-processing disabled — instant paste mode active");
        spoken_formatting::format_spoken_commands(trimmed)
    } else {
        if session_ctrl.lock().current_session == session_id {
            hud_overlay.update_text("Polishing...");
        }
        println!("[llm] Polishing text with {:?}...", config.llm_provider);
        match llm::post_process(trimmed, config).await {
            Ok(t) => {
                let cleaned = t.trim().to_string();
                if cleaned.is_empty() {
                    trimmed.to_string()
                } else {
                    println!("[llm] Clean text: {:?}", cleaned);
                    cleaned
                }
            }
            Err(e) => {
                println!("[llm] Notice (applying local filler filter): {e}");
                llm::strip_filler_words(trimmed)
            }
        }
    };

    // 4. Context-aware smart spacing (prevents glued words across consecutive dictations)
    static LAST_PASTE_STATE: Mutex<Option<(char, std::time::Instant)>> = Mutex::new(None);

    let preceding_char = {
        let guard = LAST_PASTE_STATE.lock();
        if let Some((ch, time)) = *guard {
            if time.elapsed() < Duration::from_secs(45) {
                Some(ch)
            } else {
                None
            }
        } else {
            None
        }
    };

    let text_to_paste = smart_spacing::apply_smart_spacing(final_text.trim(), preceding_char);
    if text_to_paste.is_empty() {
        if session_ctrl.lock().current_session == session_id {
            hud_overlay.hide();
        }
        return;
    }

    // 5. Paste clean final text once at cursor
    println!("[paste] Pasting clean text at cursor: {:?}", text_to_paste);
    if let Err(e) = paster::paste_text(&text_to_paste) {
        eprintln!("[paste] Paste error: {e}");
    } else if let Some(last_ch) = text_to_paste.trim_end().chars().last() {
        let mut guard = LAST_PASTE_STATE.lock();
        *guard = Some((last_ch, std::time::Instant::now()));
    }

    // 6. Confirmation & fade out HUD
    if session_ctrl.lock().current_session == session_id {
        hud_overlay.update_text("✓ Pasted");
        tokio::time::sleep(Duration::from_millis(300)).await;
        if session_ctrl.lock().current_session == session_id {
            hud_overlay.hide();
        }
    }
}

fn generate_waves_icon_rgba() -> (Vec<u8>, u32, u32) {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];

    let set_pixel = |rgba: &mut Vec<u8>, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8| {
        if x < width && y < height {
            let idx = ((y * width + x) * 4) as usize;
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    };

    let w = 255u8; // Crisp white color for macOS menu bar soundwave

    // Symmetrical 5-bar soundwave equalizer
    // Bar 1 (left short)
    for y in 12..=20 {
        set_pixel(&mut rgba, 6, y, w, w, w, 255);
        set_pixel(&mut rgba, 7, y, w, w, w, 255);
    }
    // Bar 2 (mid-left medium)
    for y in 8..=24 {
        set_pixel(&mut rgba, 11, y, w, w, w, 255);
        set_pixel(&mut rgba, 12, y, w, w, w, 255);
    }
    // Bar 3 (center peak)
    for y in 4..=28 {
        set_pixel(&mut rgba, 16, y, w, w, w, 255);
        set_pixel(&mut rgba, 17, y, w, w, w, 255);
    }
    // Bar 4 (mid-right medium)
    for y in 8..=24 {
        set_pixel(&mut rgba, 21, y, w, w, w, 255);
        set_pixel(&mut rgba, 22, y, w, w, w, 255);
    }
    // Bar 5 (right short)
    for y in 12..=20 {
        set_pixel(&mut rgba, 26, y, w, w, w, 255);
        set_pixel(&mut rgba, 27, y, w, w, w, 255);
    }

    (rgba, width, height)
}

fn build_tray() -> (tray_icon::TrayIcon, tray_icon::menu::MenuId, tray_icon::menu::MenuId) {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new("Keryx Voice Intelligence", false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let settings_item = MenuItem::new("Settings...", true, None);
    let settings_id = settings_item.id().clone();
    let _ = menu.append(&settings_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit_item = MenuItem::new("Quit Keryx", true, None);
    let quit_id = quit_item.id().clone();
    let _ = menu.append(&quit_item);

    let (icon_rgba, width, height) = generate_waves_icon_rgba();
    let icon = tray_icon::Icon::from_rgba(icon_rgba, width, height)
        .expect("Failed to create crisp soundwave tray icon");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Keryx — Hold hotkey to dictate")
        .with_icon(icon)
        .build()
        .expect("Failed to build tray icon");

    (tray, settings_id, quit_id)
}

fn setup_config_dir() {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".config/keryx");
        let _ = std::fs::create_dir_all(&dir);
        let env_path = dir.join(".env");
        if !env_path.exists() {
            let legacy_env = home.join(".config/wisprflow/.env");
            if legacy_env.exists() {
                let _ = std::fs::copy(&legacy_env, &env_path);
            } else {
                let default_env = include_str!("../config/.env.example");
                let _ = std::fs::write(&env_path, default_env);
                println!("✓ Created default config at {}", env_path.display());
            }
        }
    }
}

#[allow(unexpected_cfgs)]
#[cfg(target_os = "macos")]
fn check_and_prompt_accessibility() {
    use cocoa::base::{id, nil, YES};
    use cocoa::foundation::{NSDictionary, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: id) -> bool;
        fn AXIsProcessTrusted() -> bool;
    }

    unsafe {
        // First, silently check WITHOUT prompting — avoids repeated dialogs
        let already_trusted = AXIsProcessTrusted();
        if already_trusted {
            println!("✓ Accessibility permission active (cached).");
            return;
        }

        // Not trusted yet — prompt once
        println!("[Keryx] Requesting Accessibility permission (one-time prompt)...");
        let key = NSString::alloc(nil).init_str("AXTrustedCheckOptionPrompt");
        let num_cls = class!(NSNumber);
        let val: id = msg_send![num_cls, numberWithBool: YES];
        let dict = NSDictionary::dictionaryWithObject_forKey_(nil, val, key);
        let is_trusted = AXIsProcessTrustedWithOptions(dict);
        if is_trusted {
            println!("✓ Accessibility permission active.");
        } else {
            println!("[Keryx] ⚠️ Accessibility permission needed.");
            println!("[Keryx]    System Settings → Privacy & Security → Accessibility → Enable Keryx");
        }
    }
}

#[allow(unexpected_cfgs)]
#[cfg(target_os = "macos")]
fn check_and_prompt_microphone() {
    use cocoa::foundation::NSString;
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    unsafe {
        let media_type = NSString::alloc(cocoa::base::nil).init_str("soun"); // AVMediaTypeAudio
        let cls = class!(AVCaptureDevice);
        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media_type];
        match status {
            3 => println!("✓ Microphone permission active."),
            0 => {
                // Not determined — request
                println!("[mic] Requesting microphone permission...");
                let _: () = msg_send![cls, requestAccessForMediaType: media_type completionHandler: {
                    // block — we just let it complete naturally
                    let block: *mut std::ffi::c_void = std::ptr::null_mut();
                    block
                }];
            }
            1 => println!("[mic] ⚠️ Microphone access RESTRICTED. Check MDM policy."),
            2 => println!("[mic] ⚠️ Microphone access DENIED. Enable in System Settings → Privacy."),
            _ => println!("[mic] Microphone status: {}", status),
        }
    }
}
