#![allow(unexpected_cfgs)]
#![allow(dead_code)]

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSApp, NSColor, NSScreen, NSWindowStyleMask};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil, NO, YES};
#[cfg(target_os = "macos")]
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Copy, Clone, Debug)]
struct SendPtr(usize);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

impl SendPtr {
    fn from_id(ptr: id) -> Self {
        SendPtr(ptr as usize)
    }
    fn to_id(self) -> id {
        self.0 as id
    }
}

static CURRENT_WINDOW: Mutex<Option<SendPtr>> = Mutex::new(None);
static CURRENT_MONITOR: Mutex<Option<SendPtr>> = Mutex::new(None);
static SETTINGS_OPENING: AtomicBool = AtomicBool::new(false);

type CallbackMap = HashMap<usize, Box<dyn FnMut() + Send>>;

#[cfg(target_os = "macos")]
fn run_on_main_thread<F: FnOnce() + Send + 'static>(f: F) {
    #[link(name = "System")]
    extern "C" {
        fn dispatch_async_f(
            queue: *mut std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
        static _dispatch_main_q: std::ffi::c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopGetMain() -> *mut std::ffi::c_void;
        fn CFRunLoopWakeUp(rl: *mut std::ffi::c_void);
    }

    extern "C" fn trampoline<F: FnOnce()>(context: *mut std::ffi::c_void) {
        unsafe {
            let b = Box::from_raw(context as *mut F);
            b();
        }
    }

    let boxed = Box::new(f);
    let raw = Box::into_raw(boxed) as *mut std::ffi::c_void;
    unsafe {
        let main_q = &_dispatch_main_q as *const _ as *mut std::ffi::c_void;
        dispatch_async_f(main_q, raw, trampoline::<F>);
        let rl = CFRunLoopGetMain();
        if !rl.is_null() {
            CFRunLoopWakeUp(rl);
        }
    }
}

fn get_config_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".config/keryx");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(".env");
        if !path.exists() {
            let legacy = home.join(".config/wisprflow/.env");
            if legacy.exists() {
                let _ = std::fs::copy(&legacy, &path);
            } else {
                let default_content = include_str!("../config/.env.example");
                let _ = std::fs::write(&path, default_content);
            }
        }
        path
    } else {
        PathBuf::from(".env")
    }
}

fn read_env_file() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let path = get_config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    map
}

pub fn save_env_file(updates: &HashMap<String, String>) -> Result<(), String> {
    let path = get_config_path();
    let mut lines = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') && trimmed.contains('=') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    let k = k.trim();
                    if let Some(new_val) = updates.get(k) {
                        lines.push(format!("{}={}", k, new_val));
                        seen_keys.insert(k.to_string());
                        continue;
                    }
                }
            }
            lines.push(line.to_string());
        }
    }

    for (k, v) in updates {
        if !seen_keys.contains(k) {
            lines.push(format!("{}={}", k, v));
        }
    }

    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

// ── Dropdown Item Definitions & Value Mapping ────────────────────────────────

const HOTKEY_OPTIONS: &[(&str, &str)] = &[
    ("Right Option (Recommended)", "right_alt"),
    ("Left Option", "option"),
    ("Right Control", "right_control"),
    ("Left Control", "control"),
    ("Right Shift", "right_shift"),
    ("Left Shift", "shift"),
    ("Caps Lock", "caps_lock"),
    ("Fn / Globe", "fn"),
    ("Space", "space"),
    ("F1", "f1"),
    ("F2", "f2"),
    ("F3", "f3"),
    ("F4", "f4"),
    ("F5", "f5"),
    ("F6", "f6"),
    ("F7", "f7"),
    ("F8", "f8"),
    ("F9", "f9"),
    ("F10", "f10"),
    ("F11", "f11"),
    ("F12", "f12"),
    ("Key D", "d"),
    ("Key A", "a"),
];

const MODE_OPTIONS: &[(&str, bool)] = &[
    ("AI Polish (Format, Punctuate & Remove Fillers)", true),
    ("Instant STT (0ms Latency, Direct Paste)", false),
];

const STT_OPTIONS: &[(&str, &str)] = &[
    ("Auto (Local Whisper with Cloud Fallback)", "auto"),
    ("Local whisper.cpp (Offline / Apple Metal)", "local"),
    ("NVIDIA Parakeet CTC 1.1B (Fast Cloud)", "nvidia"),
    ("Groq Whisper Large v3 (120ms LPU)", "groq"),
    ("OpenAI Whisper (Cloud)", "openai"),
];

const LLM_PROV_OPTIONS: &[(&str, &str)] = &[
    ("NVIDIA Build (Free Tier)", "nvidia"),
    ("Groq LPU (Ultra-Fast)", "groq"),
    ("OpenAI (GPT-4o)", "openai"),
    ("None (Disabled / Instant Mode)", "none"),
];

const MODEL_OPTIONS: &[(&str, &str)] = &[
    ("Llama 3.2 11B Vision Instruct (NVIDIA Default)", "meta/llama-3.2-11b-vision-instruct"),
    ("Llama 3.1 8B Instruct (Lightweight & Fast)", "meta/llama-3.1-8b-instruct"),
    ("Llama 3.3 70B Instruct (High Accuracy)", "meta/llama-3.3-70b-instruct"),
    ("Llama 3.3 70B Versatile (Groq)", "llama-3.3-70b-versatile"),
    ("GPT-4o Mini (OpenAI)", "gpt-4o-mini"),
    ("GPT-4o (OpenAI High Accuracy)", "gpt-4o"),
];

/// Helper to get friendly display name for a hotkey string
fn get_hotkey_display_name(hotkey_str: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        let keycodes = crate::macos_hotkey::cgeventtap::hotkey_str_to_keycodes(hotkey_str);
        if let Some(&kc) = keycodes.first() {
            return crate::macos_hotkey::cgeventtap::keycode_to_name(kc).to_string();
        }
    }
    hotkey_str.replace('_', " ").to_uppercase()
}

/// Opens the clean, native macOS Settings Window styled in #2563EB -> #3B82F6 -> #60A5FA
pub fn open_settings_window() {
    #[cfg(target_os = "macos")]
    {
        if SETTINGS_OPENING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            println!("[settings] Already opening, ignoring duplicate request");
            return;
        }

        run_on_main_thread(move || unsafe {
            SETTINGS_OPENING.store(false, Ordering::SeqCst);

            let mut guard = CURRENT_WINDOW.lock();
            if let Some(existing) = *guard {
                let win = existing.to_id();
                let is_visible: bool = msg_send![win, isVisible];
                if is_visible {
                    let () = msg_send![win, makeKeyAndOrderFront: nil];
                    let () = msg_send![NSApp(), activateIgnoringOtherApps: YES];
                    return;
                }
            }

            let env = read_env_file();
            let raw_hotkey = env.get("HOTKEY").cloned().unwrap_or_else(|| "right_alt".into());
            let raw_stt = env.get("TRANSCRIPTION_PROVIDER").cloned().unwrap_or_else(|| "auto".into());
            let raw_llm_prov = env.get("LLM_PROVIDER").cloned().unwrap_or_else(|| "nvidia".into());
            let raw_model = env.get("NVIDIA_LLM_MODEL").cloned().unwrap_or_else(|| "meta/llama-3.2-11b-vision-instruct".into());
            let raw_whisper_model = env.get("WHISPER_MODEL").cloned().unwrap_or_else(|| "ggml-small.en.bin".into());
            let raw_nv_key = env.get("NVIDIA_API_KEY").cloned().unwrap_or_default();
            let raw_gq_key = env.get("GROQ_API_KEY").cloned().unwrap_or_default();
            let raw_oa_key = env.get("OPENAI_API_KEY").cloned().unwrap_or_default();

            let initial_ai_post = match env.get("AI_POSTPROCESSING").map(|s| s.to_lowercase()).as_deref() {
                Some("false") | Some("0") | Some("no") | Some("off") => false,
                Some("true") | Some("1") | Some("yes") | Some("on") => true,
                _ => raw_llm_prov.to_lowercase() != "none",
            };

            // Window geometry (Generous 770pt height)
            let win_width = 540.0;
            let win_height = 770.0;

            let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(win_width, win_height));
            let style_mask = NSWindowStyleMask::NSTitledWindowMask
                | NSWindowStyleMask::NSClosableWindowMask
                | NSWindowStyleMask::NSMiniaturizableWindowMask;

            let win: id = msg_send![class!(NSWindow), alloc];
            let win: id = msg_send![
                win,
                initWithContentRect: rect
                styleMask: style_mask
                backing: 2
                defer: NO
            ];

            let title = NSString::alloc(nil).init_str("Keryx Settings");
            let () = msg_send![win, setTitle: title];
            let () = msg_send![win, center];

            // Dark appearance
            let dark_aqua = NSString::alloc(nil).init_str("NSAppearanceNameDarkAqua");
            let app_cls = class!(NSAppearance);
            let dark_appearance: id = msg_send![app_cls, appearanceNamed: dark_aqua];
            let () = msg_send![win, setAppearance: dark_appearance];

            // Midnight surface background #0D1117
            let bg_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.051, 0.067, 0.090, 1.0);
            let () = msg_send![win, setBackgroundColor: bg_color];

            let content_view: id = msg_send![win, contentView];

            // ── Header: Pure White KERYX Title & Ice Blue Subtitle ────────────
            let header_logo = create_label("KERYX", 22.0, true, 20.0, 712.0, 200.0, 28.0);
            let white_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 1.0, 1.0, 1.0, 1.0);
            let () = msg_send![header_logo, setTextColor: white_color];
            let () = msg_send![content_view, addSubview: header_logo];

            let sub_title = create_label("High-Performance Speech Intelligence & Dictation", 11.5, false, 20.0, 692.0, 500.0, 16.0);
            let ice_blue_sub = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.576, 0.773, 0.992, 1.0); // #93C5FD
            let () = msg_send![sub_title, setTextColor: ice_blue_sub];
            let () = msg_send![content_view, addSubview: sub_title];

            // ── Card 1: GENERAL & HOTKEY (y: 574.0, h: 104.0) ─────────────────
            let card1 = create_card_view(20.0, 574.0, 500.0, 104.0);
            let () = msg_send![content_view, addSubview: card1];

            let sec1_title = create_section_header("GENERAL", 16.0, 76.0);
            let () = msg_send![card1, addSubview: sec1_title];

            let lbl_hk = create_field_label("Activation Key:", 16.0, 38.0, 120.0);
            let () = msg_send![card1, addSubview: lbl_hk];

            let mut hotkey_titles: Vec<String> = HOTKEY_OPTIONS.iter().map(|(t, _)| t.to_string()).collect();
            let current_hk_val_arc = Arc::new(Mutex::new(raw_hotkey.clone()));

            let selected_hk_title = HOTKEY_OPTIONS
                .iter()
                .find(|(_, val)| *val == raw_hotkey.as_str())
                .map(|(t, _)| t.to_string())
                .unwrap_or_else(|| {
                    let custom_title = format!("Custom ({})", get_hotkey_display_name(&raw_hotkey));
                    hotkey_titles.push(custom_title.clone());
                    custom_title
                });

            let hk_title_refs: Vec<&str> = hotkey_titles.iter().map(|s| s.as_str()).collect();
            let popup_hotkey = create_popup_button(&hk_title_refs, &selected_hk_title, 140.0, 34.0, 210.0, 28.0);
            let () = msg_send![card1, addSubview: popup_hotkey];

            // Interactive Record Button
            let btn_record: id = msg_send![class!(NSButton), alloc];
            let rec_frame = NSRect::new(NSPoint::new(358.0, 34.0), NSSize::new(126.0, 28.0));
            let btn_record: id = msg_send![btn_record, initWithFrame: rec_frame];
            let () = msg_send![btn_record, setTitle: NSString::alloc(nil).init_str("Capture Key...")];
            let () = msg_send![btn_record, setBezelStyle: 1];
            let () = msg_send![card1, addSubview: btn_record];

            let hk_hint = create_label("Hold key to talk • Double-tap for continuous hands-free", 10.5, false, 16.0, 10.0, 460.0, 16.0);
            let muted = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.45, 0.55, 0.70, 1.0);
            let () = msg_send![hk_hint, setTextColor: muted];
            let () = msg_send![card1, addSubview: hk_hint];

            // ── Card 2: ENGINE & MODELS (y: 258.0, h: 302.0) ─────────────────
            let card2 = create_card_view(20.0, 258.0, 500.0, 302.0);
            let () = msg_send![content_view, addSubview: card2];

            let sec2_title = create_section_header("ENGINE & MODELS", 16.0, 272.0);
            let () = msg_send![card2, addSubview: sec2_title];

            // Row 1: Processing Mode
            let lbl_mode = create_field_label("Processing Mode:", 16.0, 236.0, 130.0);
            let () = msg_send![card2, addSubview: lbl_mode];

            let mode_titles: Vec<&str> = MODE_OPTIONS.iter().map(|(t, _)| *t).collect();
            let selected_mode_title = if initial_ai_post {
                MODE_OPTIONS[0].0
            } else {
                MODE_OPTIONS[1].0
            };
            let popup_mode = create_popup_button(&mode_titles, selected_mode_title, 140.0, 232.0, 344.0, 28.0);
            let () = msg_send![card2, addSubview: popup_mode];

            // Row 2: STT Engine
            let lbl_stt = create_field_label("Speech Engine:", 16.0, 194.0, 130.0);
            let () = msg_send![card2, addSubview: lbl_stt];

            let stt_titles: Vec<&str> = STT_OPTIONS.iter().map(|(t, _)| *t).collect();
            let selected_stt_title = STT_OPTIONS
                .iter()
                .find(|(_, val)| *val == raw_stt.to_lowercase().as_str())
                .map(|(t, _)| *t)
                .unwrap_or(STT_OPTIONS[0].0);
            let popup_stt = create_popup_button(&stt_titles, selected_stt_title, 140.0, 190.0, 344.0, 28.0);
            let () = msg_send![card2, addSubview: popup_stt];

            // Row 3: Local Whisper Model + In-App Auto Downloader
            let lbl_lm = create_field_label("Local Model:", 16.0, 152.0, 130.0);
            let () = msg_send![card2, addSubview: lbl_lm];

            let local_model_titles: Vec<&str> = crate::model_downloader::WHISPER_MODELS
                .iter()
                .map(|m| m.name)
                .collect();

            let active_whisper_info = crate::model_downloader::find_model_info(&raw_whisper_model)
                .unwrap_or(&crate::model_downloader::WHISPER_MODELS[0]);

            let popup_local_model = create_popup_button(&local_model_titles, active_whisper_info.name, 140.0, 148.0, 220.0, 28.0);
            let () = msg_send![card2, addSubview: popup_local_model];

            // Download Model Button
            let btn_download_model: id = msg_send![class!(NSButton), alloc];
            let dl_frame = NSRect::new(NSPoint::new(366.0, 148.0), NSSize::new(118.0, 28.0));
            let btn_download_model: id = msg_send![btn_download_model, initWithFrame: dl_frame];
            let is_installed_init = crate::model_downloader::is_model_installed(active_whisper_info.filename);
            let init_dl_title = if is_installed_init { "Installed" } else { "Download" };
            let () = msg_send![btn_download_model, setTitle: NSString::alloc(nil).init_str(init_dl_title)];
            let () = msg_send![btn_download_model, setBezelStyle: 1];
            let () = msg_send![btn_download_model, setEnabled: if is_installed_init { NO } else { YES }];
            let () = msg_send![card2, addSubview: btn_download_model];

            // Progress Bar (NSProgressIndicator)
            let pb_frame = NSRect::new(NSPoint::new(140.0, 126.0), NSSize::new(220.0, 12.0));
            let progress_bar: id = msg_send![class!(NSProgressIndicator), alloc];
            let progress_bar: id = msg_send![progress_bar, initWithFrame: pb_frame];
            let () = msg_send![progress_bar, setIndeterminate: NO];
            let () = msg_send![progress_bar, setMinValue: 0.0f64];
            let () = msg_send![progress_bar, setMaxValue: 100.0f64];
            let () = msg_send![progress_bar, setDoubleValue: 0.0f64];
            let () = msg_send![progress_bar, setHidden: YES];
            let () = msg_send![card2, addSubview: progress_bar];

            // Model Status / Progress Text Label
            let init_status_text = if is_installed_init {
                "✓ Model ready on disk"
            } else {
                "⚠️ Model not downloaded on disk"
            };
            let lbl_model_status = create_label(init_status_text, 10.5, false, 140.0, 122.0, 344.0, 18.0);
            let sky_blue = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.376, 0.647, 0.980, 1.0);
            let () = msg_send![lbl_model_status, setTextColor: if is_installed_init { sky_blue } else { ice_blue_sub }];
            let () = msg_send![card2, addSubview: lbl_model_status];

            // Row 4: LLM Provider
            let lbl_lp = create_field_label("LLM Provider:", 16.0, 78.0, 130.0);
            let () = msg_send![card2, addSubview: lbl_lp];

            let lp_titles: Vec<&str> = LLM_PROV_OPTIONS.iter().map(|(t, _)| *t).collect();
            let selected_lp_title = LLM_PROV_OPTIONS
                .iter()
                .find(|(_, val)| *val == raw_llm_prov.to_lowercase().as_str())
                .map(|(t, _)| *t)
                .unwrap_or(LLM_PROV_OPTIONS[0].0);
            let popup_lp = create_popup_button(&lp_titles, selected_lp_title, 140.0, 74.0, 344.0, 28.0);
            let () = msg_send![card2, addSubview: popup_lp];

            // Row 5: LLM Model
            let lbl_m = create_field_label("AI Model:", 16.0, 32.0, 130.0);
            let () = msg_send![card2, addSubview: lbl_m];

            let model_titles: Vec<&str> = MODEL_OPTIONS.iter().map(|(t, _)| *t).collect();
            let selected_model_title = MODEL_OPTIONS
                .iter()
                .find(|(_, val)| *val == raw_model.as_str())
                .map(|(t, _)| *t)
                .unwrap_or(MODEL_OPTIONS[0].0);
            let popup_model = create_popup_button(&model_titles, selected_model_title, 140.0, 28.0, 344.0, 28.0);
            let () = msg_send![card2, addSubview: popup_model];

            // ── Card 3: API CREDENTIALS (y: 58.0, h: 186.0) ───────────────────
            let card3 = create_card_view(20.0, 58.0, 500.0, 186.0);
            let () = msg_send![content_view, addSubview: card3];

            let sec3_title = create_section_header("API CREDENTIALS", 16.0, 156.0);
            let () = msg_send![card3, addSubview: sec3_title];

            // Row 1: NVIDIA Key
            let lbl_nv = create_field_label("NVIDIA Key:", 16.0, 122.0, 120.0);
            let () = msg_send![card3, addSubview: lbl_nv];
            let input_nv = create_text_field(&raw_nv_key, "nvapi-...", 140.0, 120.0, 344.0, 24.0);
            let () = msg_send![card3, addSubview: input_nv];

            // Row 2: Groq Key
            let lbl_gq = create_field_label("Groq Key:", 16.0, 80.0, 120.0);
            let () = msg_send![card3, addSubview: lbl_gq];
            let input_gq = create_text_field(&raw_gq_key, "gsk_...", 140.0, 78.0, 344.0, 24.0);
            let () = msg_send![card3, addSubview: input_gq];

            // Row 3: OpenAI Key
            let lbl_oa = create_field_label("OpenAI Key:", 16.0, 38.0, 120.0);
            let () = msg_send![card3, addSubview: lbl_oa];
            let input_oa = create_text_field(&raw_oa_key, "sk-...", 140.0, 36.0, 344.0, 24.0);
            let () = msg_send![card3, addSubview: input_oa];

            // Note
            let cred_note = create_label("Saved securely to ~/.config/keryx/.env", 10.5, false, 16.0, 10.0, 460.0, 16.0);
            let () = msg_send![cred_note, setTextColor: muted];
            let () = msg_send![card3, addSubview: cred_note];

            // ── Footer: Status & Action Buttons (y: 14.0) ───────────────────
            let status_lbl = create_label("", 11.5, false, 24.0, 18.0, 280.0, 20.0);
            let () = msg_send![status_lbl, setTextColor: sky_blue];
            let () = msg_send![content_view, addSubview: status_lbl];

            // Cancel Button
            let cancel_btn: id = msg_send![class!(NSButton), alloc];
            let cancel_frame = NSRect::new(NSPoint::new(310.0, 14.0), NSSize::new(90.0, 32.0));
            let cancel_btn: id = msg_send![cancel_btn, initWithFrame: cancel_frame];
            let () = msg_send![cancel_btn, setTitle: NSString::alloc(nil).init_str("Cancel")];
            let () = msg_send![cancel_btn, setBezelStyle: 1];
            let () = msg_send![content_view, addSubview: cancel_btn];

            // Save & Apply Button
            let save_btn: id = msg_send![class!(NSButton), alloc];
            let save_frame = NSRect::new(NSPoint::new(410.0, 14.0), NSSize::new(110.0, 32.0));
            let save_btn: id = msg_send![save_btn, initWithFrame: save_frame];
            let () = msg_send![save_btn, setTitle: NSString::alloc(nil).init_str("Save & Apply")];
            let () = msg_send![save_btn, setBezelStyle: 1];
            let () = msg_send![save_btn, setKeyEquivalent: NSString::alloc(nil).init_str("\r")];
            let () = msg_send![content_view, addSubview: save_btn];

            // Pointers for action closures
            let popup_hk_p = SendPtr::from_id(popup_hotkey);
            let mode_p = SendPtr::from_id(popup_mode);
            let stt_p = SendPtr::from_id(popup_stt);
            let lp_p = SendPtr::from_id(popup_lp);
            let model_p = SendPtr::from_id(popup_model);
            let nv_p = SendPtr::from_id(input_nv);
            let gq_p = SendPtr::from_id(input_gq);
            let oa_p = SendPtr::from_id(input_oa);
            let status_p = SendPtr::from_id(status_lbl);
            let win_p = SendPtr::from_id(win);
            let rec_btn_p = SendPtr::from_id(btn_record);

            let dl_btn_p = SendPtr::from_id(btn_download_model);
            let pb_p = SendPtr::from_id(progress_bar);
            let model_status_p = SendPtr::from_id(lbl_model_status);
            let local_model_p = SendPtr::from_id(popup_local_model);

            // Track last valid installed model name for automatic rollback if uninstalled selection is cancelled
            let last_valid_model_name = Arc::new(Mutex::new(active_whisper_info.name.to_string()));

            // ── In-App Automatic Whisper Model Downloader Logic ──────────────
            let cancel_dl_flag = Arc::new(AtomicBool::new(false));

            let start_model_download = {
                let pb_p = pb_p.clone();
                let model_status_p = model_status_p.clone();
                let dl_btn_p = dl_btn_p.clone();
                let cancel_dl_flag = cancel_dl_flag.clone();
                let last_valid_model_name = last_valid_model_name.clone();

                Arc::new(move |model_info: crate::model_downloader::WhisperModelInfo| {
                    let pb = pb_p.to_id();
                    let status = model_status_p.to_id();
                    let btn = dl_btn_p.to_id();

                    let () = msg_send![btn, setEnabled: NO];
                    let () = msg_send![btn, setTitle: NSString::alloc(nil).init_str("Downloading...")];
                    let () = msg_send![pb, setHidden: NO];
                    let () = msg_send![pb, setDoubleValue: 0.0f64];
                    let ice = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.576, 0.773, 0.992, 1.0);
                    let () = msg_send![status, setTextColor: ice];
                    set_text(status, "Connecting to download server...");

                    let pb_ptr = pb_p.clone();
                    let status_ptr = model_status_p.clone();
                    let btn_ptr = dl_btn_p.clone();
                    let flag = cancel_dl_flag.clone();
                    let last_valid_clone = last_valid_model_name.clone();
                    let model_title_saved = model_info.name.to_string();

                    tokio::spawn(async move {
                        let pb_inner = pb_ptr.clone();
                        let status_inner = status_ptr.clone();

                        let res = crate::model_downloader::download_model_streaming(
                            &model_info,
                            flag,
                            move |frac, downloaded, total| {
                                let p = pb_inner.clone();
                                let s = status_inner.clone();
                                run_on_main_thread(move || {
                                    let _: () = msg_send![p.to_id(), setDoubleValue: frac * 100.0];
                                    let mb_down = downloaded as f64 / (1024.0 * 1024.0);
                                    let mb_total = total as f64 / (1024.0 * 1024.0);
                                    let text = format!("Downloading... {:.0}% ({:.1} MB / {:.1} MB)", frac * 100.0, mb_down, mb_total);
                                    set_text(s.to_id(), &text);
                                });
                            },
                        ).await;

                        run_on_main_thread(move || {
                            let pb = pb_ptr.to_id();
                            let status = status_ptr.to_id();
                            let btn = btn_ptr.to_id();

                            let () = msg_send![pb, setHidden: YES];

                            match res {
                                Ok(path) => {
                                    *last_valid_clone.lock() = model_title_saved;
                                    let () = msg_send![btn, setTitle: NSString::alloc(nil).init_str("Installed")];
                                    let () = msg_send![btn, setEnabled: NO];
                                    let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or("model");
                                    let msg = format!("✓ {} active & ready!", fname);
                                    set_text(status, &msg);
                                    let sky = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.376, 0.647, 0.980, 1.0);
                                    let () = msg_send![status, setTextColor: sky];
                                }
                                Err(e) => {
                                    let () = msg_send![btn, setTitle: NSString::alloc(nil).init_str("Retry")];
                                    let () = msg_send![btn, setEnabled: YES];
                                    let coral = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.98, 0.45, 0.45, 1.0);
                                    let () = msg_send![status, setTextColor: coral];
                                    set_text(status, &format!("Download failed: {}", e));
                                }
                            }
                        });
                    });
                })
            };

            // ── Download Button Direct Click ─────────────────────────────────
            let dl_fn_for_btn = start_model_download.clone();
            let local_model_p_for_btn = local_model_p.clone();
            register_button_action(btn_download_model, move || {
                let selected_title = get_popup_selected_title(local_model_p_for_btn.to_id());
                let model_info = crate::model_downloader::find_model_info(&selected_title)
                    .cloned()
                    .unwrap_or_else(|| crate::model_downloader::WHISPER_MODELS[0].clone());
                dl_fn_for_btn(model_info);
            });

            // ── Interactive Local Model Dropdown Selection Handler ───────────
            let dl_fn_for_popup = start_model_download.clone();
            let local_model_p_for_popup = local_model_p.clone();
            let btn_p_for_popup = dl_btn_p.clone();
            let pb_p_for_popup = pb_p.clone();
            let status_p_for_popup = model_status_p.clone();
            let last_valid_for_popup = last_valid_model_name.clone();

            register_button_action(popup_local_model, move || {
                let popup = local_model_p_for_popup.to_id();
                let btn = btn_p_for_popup.to_id();
                let pb = pb_p_for_popup.to_id();
                let status = status_p_for_popup.to_id();

                let selected_title = get_popup_selected_title(popup);
                let model_info = match crate::model_downloader::find_model_info(&selected_title) {
                    Some(m) => m.clone(),
                    None => return,
                };

                let is_installed = crate::model_downloader::is_model_installed(model_info.filename);
                let () = msg_send![pb, setHidden: YES];

                if is_installed {
                    *last_valid_for_popup.lock() = model_info.name.to_string();
                    let () = msg_send![btn, setTitle: NSString::alloc(nil).init_str("Installed")];
                    let () = msg_send![btn, setEnabled: NO];
                    set_text(status, "✓ Model ready on disk");
                    let sky = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.376, 0.647, 0.980, 1.0);
                    let () = msg_send![status, setTextColor: sky];
                } else {
                    // Alert Modal: Ask user to download automatically
                    let alert: id = msg_send![class!(NSAlert), alloc];
                    let alert: id = msg_send![alert, init];
                    let () = msg_send![alert, setMessageText: NSString::alloc(nil).init_str("Model Not Downloaded")];
                    let info_text = format!(
                        "'{}' (~{} MB) is not on disk yet.\n\nWould you like to download and set it up automatically now?",
                        model_info.name, model_info.size_mb
                    );
                    let () = msg_send![alert, setInformativeText: NSString::alloc(nil).init_str(&info_text)];
                    let () = msg_send![alert, addButtonWithTitle: NSString::alloc(nil).init_str("Download & Setup")];
                    let () = msg_send![alert, addButtonWithTitle: NSString::alloc(nil).init_str("Cancel")];
                    let () = msg_send![alert, setAlertStyle: 1]; // NSAlertStyleInformational

                    let resp: i64 = msg_send![alert, runModal];
                    if resp == 1000 {
                        // 1000 = Download & Setup
                        dl_fn_for_popup(model_info);
                    } else {
                        // 1001 = Cancel -> Revert selection back to previous installed model!
                        let prev_title = last_valid_for_popup.lock().clone();
                        let prev_ns = NSString::alloc(nil).init_str(&prev_title);
                        let () = msg_send![popup, selectItemWithTitle: prev_ns];

                        let () = msg_send![btn, setTitle: NSString::alloc(nil).init_str("Installed")];
                        let () = msg_send![btn, setEnabled: NO];
                        let revert_msg = format!("Reverted to '{}'.", prev_title);
                        set_text(status, &revert_msg);
                        let ice = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.576, 0.773, 0.992, 1.0);
                        let () = msg_send![status, setTextColor: ice];
                    }
                }
            });

            // ── Interactive "Capture Key..." Click Handler ───────────────────
            let hk_arc_for_record = current_hk_val_arc.clone();
            register_button_action(btn_record, move || {
                let rec_btn = rec_btn_p.to_id();
                let status = status_p.to_id();
                let popup = popup_hk_p.to_id();
                let hk_val_arc = hk_arc_for_record.clone();

                let mut mon_guard = CURRENT_MONITOR.lock();
                if let Some(m) = mon_guard.take() {
                    let () = msg_send![class!(NSEvent), removeMonitor: m.to_id()];
                }

                let () = msg_send![rec_btn, setTitle: NSString::alloc(nil).init_str("Press any key...")];
                let () = msg_send![status, setStringValue: NSString::alloc(nil).init_str("Listening: press any key on keyboard (Esc to cancel)")];

                let block = block::ConcreteBlock::new(move |event: id| -> id {
                    let keycode: u16 = msg_send![event, keyCode];
                    if keycode == 53 {
                        // Esc: cancel
                        let () = msg_send![rec_btn, setTitle: NSString::alloc(nil).init_str("Capture Key...")];
                        let () = msg_send![status, setStringValue: NSString::alloc(nil).init_str("Key capture cancelled.")];
                    } else {
                        let name = crate::macos_hotkey::cgeventtap::keycode_to_name(keycode);
                        let cfg_str = crate::macos_hotkey::cgeventtap::keycode_to_config_str(keycode);
                        *hk_val_arc.lock() = cfg_str.clone();

                        let item_title = format!("Custom ({})", name);
                        let ns_title = NSString::alloc(nil).init_str(&item_title);

                        let () = msg_send![popup, addItemWithTitle: ns_title];
                        let () = msg_send![popup, selectItemWithTitle: ns_title];

                        let () = msg_send![rec_btn, setTitle: NSString::alloc(nil).init_str("Capture Key...")];
                        let status_str = format!("✓ Captured '{}'. Click Save & Apply.", name);
                        let () = msg_send![status, setStringValue: NSString::alloc(nil).init_str(&status_str)];
                    }

                    let mut mon_guard = CURRENT_MONITOR.lock();
                    if let Some(m) = mon_guard.take() {
                        let () = msg_send![class!(NSEvent), removeMonitor: m.to_id()];
                    }

                    nil
                });

                let block = block.copy();
                let mask: u64 = (1 << 10) | (1 << 12); // NSKeyDownMask | NSFlagsChangedMask
                let monitor: id = msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: mask handler: &*block];
                *mon_guard = Some(SendPtr::from_id(monitor));
            });

            // Cancel Action
            let cancel_flag_on_close = cancel_dl_flag.clone();
            register_button_action(cancel_btn, move || {
                cancel_flag_on_close.store(true, Ordering::SeqCst);
                let mut mon_guard = CURRENT_MONITOR.lock();
                if let Some(m) = mon_guard.take() {
                    let () = msg_send![class!(NSEvent), removeMonitor: m.to_id()];
                }
                let () = msg_send![win_p.to_id(), close];
            });

            // Save & Apply Action
            let hk_arc_for_save = current_hk_val_arc.clone();
            register_button_action(save_btn, move || {
                let mut mon_guard = CURRENT_MONITOR.lock();
                if let Some(m) = mon_guard.take() {
                    let () = msg_send![class!(NSEvent), removeMonitor: m.to_id()];
                }

                let selected_hk_title = get_popup_selected_title(popup_hk_p.to_id());
                let hotkey_val = if selected_hk_title.starts_with("Custom (") {
                    hk_arc_for_save.lock().clone()
                } else {
                    HOTKEY_OPTIONS
                        .iter()
                        .find(|(t, _)| *t == selected_hk_title)
                        .map(|(_, val)| val.to_string())
                        .unwrap_or_else(|| hk_arc_for_save.lock().clone())
                };

                let selected_mode_title = get_popup_selected_title(mode_p.to_id());
                let selected_stt_title = get_popup_selected_title(stt_p.to_id());
                let selected_local_model_title = get_popup_selected_title(local_model_p.to_id());
                let selected_lp_title = get_popup_selected_title(lp_p.to_id());
                let selected_model_title = get_popup_selected_title(model_p.to_id());

                let ai_postprocessing_val = MODE_OPTIONS
                    .iter()
                    .find(|(t, _)| *t == selected_mode_title)
                    .map(|(_, val)| *val)
                    .unwrap_or(true);

                let stt_val = STT_OPTIONS
                    .iter()
                    .find(|(t, _)| *t == selected_stt_title)
                    .map(|(_, val)| *val)
                    .unwrap_or("auto");

                let whisper_model_val = crate::model_downloader::find_model_info(&selected_local_model_title)
                    .map(|m| crate::model_downloader::get_model_path(m.filename).to_string_lossy().to_string())
                    .unwrap_or_else(|| selected_local_model_title.clone());

                let lp_val = LLM_PROV_OPTIONS
                    .iter()
                    .find(|(t, _)| *t == selected_lp_title)
                    .map(|(_, val)| *val)
                    .unwrap_or("nvidia");

                if stt_val == "local" {
                    if let Some(m_info) = crate::model_downloader::find_model_info(&selected_local_model_title) {
                        if !crate::model_downloader::is_model_installed(m_info.filename) {
                            let warn_str = format!("⚠️ Local offline requires '{}'. Click [Download] first or use Auto.", m_info.name);
                            set_text(status_p.to_id(), &warn_str);
                            let coral = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.98, 0.45, 0.45, 1.0);
                            let () = msg_send![status_p.to_id(), setTextColor: coral];
                            return;
                        }
                    }
                }

                let model_val = MODEL_OPTIONS
                    .iter()
                    .find(|(t, _)| *t == selected_model_title)
                    .map(|(_, val)| *val)
                    .unwrap_or("meta/llama-3.2-11b-vision-instruct");

                let mut updates = HashMap::new();
                updates.insert("HOTKEY".to_string(), hotkey_val);
                updates.insert("AI_POSTPROCESSING".to_string(), ai_postprocessing_val.to_string());
                updates.insert("TRANSCRIPTION_PROVIDER".to_string(), stt_val.to_string());
                updates.insert("WHISPER_MODEL".to_string(), whisper_model_val);
                updates.insert("LLM_PROVIDER".to_string(), lp_val.to_string());
                updates.insert("NVIDIA_LLM_MODEL".to_string(), model_val.to_string());
                updates.insert("NVIDIA_API_KEY".to_string(), get_text(nv_p.to_id()));
                updates.insert("GROQ_API_KEY".to_string(), get_text(gq_p.to_id()));
                updates.insert("OPENAI_API_KEY".to_string(), get_text(oa_p.to_id()));

                if let Err(e) = save_env_file(&updates) {
                    set_text(status_p.to_id(), &format!("Error: {}", e));
                } else {
                    set_text(status_p.to_id(), "✓ Saved successfully");
                    let () = msg_send![win_p.to_id(), close];
                }
            });

            let () = msg_send![win, makeKeyAndOrderFront: nil];
            let () = msg_send![NSApp(), activateIgnoringOtherApps: YES];

            *guard = Some(SendPtr::from_id(win));
        });
    }
}

// ── Native macOS Cocoa UI Helpers ───────────────────────────────────────────

#[cfg(target_os = "macos")]
unsafe fn create_gradient_brand_logo(x: f64, y: f64, w: f64, h: f64) -> id {
    let view: id = msg_send![class!(NSView), alloc];
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let view: id = msg_send![view, initWithFrame: frame];
    let () = msg_send![view, setWantsLayer: YES];
    let layer: id = msg_send![view, layer];

    // User-specified exact gradient: #8B5CF6 -> #6366F1 -> #60A5FA
    let grad_layer: id = msg_send![class!(CAGradientLayer), alloc];
    let grad_layer: id = msg_send![grad_layer, init];
    let grad_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h));
    let () = msg_send![grad_layer, setFrame: grad_frame];

    let c1 = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.545, 0.361, 0.965, 1.0); // #8B5CF6 (Violet)
    let c2 = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.388, 0.400, 0.945, 1.0); // #6366F1 (Indigo)
    let c3 = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.376, 0.647, 0.980, 1.0); // #60A5FA (Sky Blue)

    let cg_c1: id = msg_send![c1, CGColor];
    let cg_c2: id = msg_send![c2, CGColor];
    let cg_c3: id = msg_send![c3, CGColor];

    let array_cls = class!(NSArray);
    let objects = [cg_c1, cg_c2, cg_c3];
    let colors: id = msg_send![array_cls, arrayWithObjects: objects.as_ptr() count: 3usize];
    let () = msg_send![grad_layer, setColors: colors];

    // Horizontal linear gradient
    let () = msg_send![grad_layer, setStartPoint: NSPoint::new(0.0, 0.5)];
    let () = msg_send![grad_layer, setEndPoint: NSPoint::new(1.0, 0.5)];

    // Text mask layer
    let text_layer: id = msg_send![class!(CATextLayer), alloc];
    let text_layer: id = msg_send![text_layer, init];
    let () = msg_send![text_layer, setFrame: grad_frame];
    let ns_str = NSString::alloc(nil).init_str("KERYX");
    let () = msg_send![text_layer, setString: ns_str];

    let font_cls = class!(NSFont);
    let font: id = msg_send![font_cls, boldSystemFontOfSize: 22.0];
    let () = msg_send![text_layer, setFont: font];
    let () = msg_send![text_layer, setFontSize: 22.0];

    let screen = NSScreen::mainScreen(nil);
    let scale: f64 = if !screen.is_null() { msg_send![screen, backingScaleFactor] } else { 2.0 };
    let () = msg_send![text_layer, setContentsScale: scale];
    let () = msg_send![grad_layer, setContentsScale: scale];

    let () = msg_send![grad_layer, setMask: text_layer];
    let () = msg_send![layer, addSublayer: grad_layer];

    view
}

#[cfg(target_os = "macos")]
unsafe fn create_card_view(x: f64, y: f64, w: f64, h: f64) -> id {
    let view: id = msg_send![class!(NSView), alloc];
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let view: id = msg_send![view, initWithFrame: frame];
    let () = msg_send![view, setWantsLayer: YES];

    let layer: id = msg_send![view, layer];
    // Deep Slate Card #131823
    let card_bg = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.075, 0.094, 0.137, 1.0);
    let cg_color: id = msg_send![card_bg, CGColor];
    let () = msg_send![layer, setBackgroundColor: cg_color];
    let () = msg_send![layer, setCornerRadius: 10.0];

    // Subtle 1px Slate Blue Border #1E293B (0.85 opacity)
    let border_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.118, 0.161, 0.231, 0.85);
    let cg_border: id = msg_send![border_color, CGColor];
    let () = msg_send![layer, setBorderColor: cg_border];
    let () = msg_send![layer, setBorderWidth: 1.0];

    view
}

#[cfg(target_os = "macos")]
unsafe fn create_section_header(text: &str, x: f64, y: f64) -> id {
    let lbl = create_label(text, 10.5, true, x, y, 220.0, 16.0);
    // Royal Blue #3B82F6
    let blue_accent = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.231, 0.510, 0.965, 1.0);
    let () = msg_send![lbl, setTextColor: blue_accent];
    lbl
}

#[cfg(target_os = "macos")]
unsafe fn create_field_label(text: &str, x: f64, y: f64, w: f64) -> id {
    let lbl = create_label(text, 12.0, false, x, y, w, 20.0);
    // Cool Soft White #F1F5F9
    let text_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.940, 0.960, 0.980, 1.0);
    let () = msg_send![lbl, setTextColor: text_color];
    lbl
}

#[cfg(target_os = "macos")]
unsafe fn create_label(
    text: &str,
    size: f64,
    bold: bool,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> id {
    let lbl: id = msg_send![class!(NSTextField), alloc];
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let lbl: id = msg_send![lbl, initWithFrame: frame];
    let ns_str = NSString::alloc(nil).init_str(text);
    let () = msg_send![lbl, setStringValue: ns_str];
    let () = msg_send![lbl, setBezeled: NO];
    let () = msg_send![lbl, setDrawsBackground: NO];
    let () = msg_send![lbl, setEditable: NO];
    let () = msg_send![lbl, setSelectable: NO];

    let font_cls = class!(NSFont);
    let font: id = if bold {
        msg_send![font_cls, boldSystemFontOfSize: size]
    } else {
        msg_send![font_cls, systemFontOfSize: size]
    };
    let () = msg_send![lbl, setFont: font];

    let white = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.95, 0.95, 0.98, 1.0);
    let () = msg_send![lbl, setTextColor: white];
    lbl
}

#[cfg(target_os = "macos")]
unsafe fn create_popup_button(
    items: &[&str],
    selected: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> id {
    let popup: id = msg_send![class!(NSPopUpButton), alloc];
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let popup: id = msg_send![popup, initWithFrame: frame pullsDown: NO];
    let () = msg_send![popup, setBezelStyle: 1]; // NSRoundedBezelStyle

    let () = msg_send![popup, removeAllItems];
    for &item in items {
        let ns_str = NSString::alloc(nil).init_str(item);
        let () = msg_send![popup, addItemWithTitle: ns_str];
    }

    let sel_str = NSString::alloc(nil).init_str(selected);
    let () = msg_send![popup, selectItemWithTitle: sel_str];

    let font_cls = class!(NSFont);
    let font: id = msg_send![font_cls, systemFontOfSize: 12.0];
    let () = msg_send![popup, setFont: font];

    popup
}

#[cfg(target_os = "macos")]
unsafe fn get_popup_selected_title(popup: id) -> String {
    let val: id = msg_send![popup, titleOfSelectedItem];
    if val.is_null() {
        return String::new();
    }
    let utf8: *const std::os::raw::c_char = msg_send![val, UTF8String];
    if utf8.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(utf8)
        .to_string_lossy()
        .trim()
        .to_string()
}

#[cfg(target_os = "macos")]
unsafe fn create_text_field(initial: &str, placeholder: &str, x: f64, y: f64, w: f64, h: f64) -> id {
    let tf: id = msg_send![class!(NSTextField), alloc];
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    let tf: id = msg_send![tf, initWithFrame: frame];
    let ns_str = NSString::alloc(nil).init_str(initial);
    let () = msg_send![tf, setStringValue: ns_str];
    let () = msg_send![tf, setEditable: YES];
    let () = msg_send![tf, setSelectable: YES];
    let () = msg_send![tf, setBezeled: YES];
    let () = msg_send![tf, setBezelStyle: 1]; // NSTextFieldRoundedBezel

    let ph_str = NSString::alloc(nil).init_str(placeholder);
    let () = msg_send![tf, setPlaceholderString: ph_str];

    let font_cls = class!(NSFont);
    let font: id = msg_send![font_cls, systemFontOfSize: 11.5];
    let () = msg_send![tf, setFont: font];
    tf
}

#[cfg(target_os = "macos")]
unsafe fn get_text(tf: id) -> String {
    let val: id = msg_send![tf, stringValue];
    if val.is_null() {
        return String::new();
    }
    let utf8: *const std::os::raw::c_char = msg_send![val, UTF8String];
    if utf8.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(utf8)
        .to_string_lossy()
        .trim()
        .to_string()
}

#[cfg(target_os = "macos")]
unsafe fn set_text(tf: id, text: &str) {
    let ns_str = NSString::alloc(nil).init_str(text);
    let () = msg_send![tf, setStringValue: ns_str];
}

#[cfg(target_os = "macos")]
fn register_button_action<F: FnMut() + Send + 'static>(btn: id, callback: F) {
    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Once;

    static INIT: Once = Once::new();
    static CALLBACKS: Mutex<Option<CallbackMap>> = Mutex::new(None);
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

    INIT.call_once(|| {
        let mut map_guard = CALLBACKS.lock();
        *map_guard = Some(HashMap::new());

        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("KeryxButtonActionTarget", superclass).unwrap();

        extern "C" fn perform_action(this: &Object, _cmd: Sel, _sender: id) {
            unsafe {
                let id_val: usize = *this.get_ivar("action_id");
                if id_val != 0 {
                    let mut map_guard = CALLBACKS.lock();
                    if let Some(map) = map_guard.as_mut() {
                        if let Some(closure) = map.get_mut(&id_val) {
                            closure();
                        }
                    }
                }
            }
        }

        decl.add_ivar::<usize>("action_id");
        unsafe {
            decl.add_method(
                sel!(trigger:),
                perform_action as extern "C" fn(&Object, Sel, id),
            );
        }
        decl.register();
    });

    let action_id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    {
        let mut map_guard = CALLBACKS.lock();
        if let Some(map) = map_guard.as_mut() {
            map.insert(action_id, Box::new(callback));
        }
    }

    unsafe {
        let cls = class!(KeryxButtonActionTarget);
        let target: id = msg_send![cls, alloc];
        let target: id = msg_send![target, init];
        (*target).set_ivar("action_id", action_id);

        let () = msg_send![btn, setTarget: target];
        let () = msg_send![btn, setAction: sel!(trigger:)];
    }
}
