/// Production-grade macOS global event tap using CGEventTap + CFRunLoop.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub mod cgeventtap {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};

    type CGEventType = u32;
    type CGEventMask = u64;
    type CGKeyCode   = u16;

    const K_CG_EVENT_KEY_DOWN:        CGEventType = 10;
    const K_CG_EVENT_KEY_UP:          CGEventType = 11;
    const K_CG_EVENT_FLAGS_CHANGED:   CGEventType = 12;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFFFFFE;
    const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: CGEventType = 0xFFFFFFFF;

    const K_CG_SESSION_EVENT_TAP:     u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    const K_CG_KEY_DOWN_MASK:         CGEventMask = 1 << K_CG_EVENT_KEY_DOWN;
    const K_CG_KEY_UP_MASK:           CGEventMask = 1 << K_CG_EVENT_KEY_UP;
    const K_CG_FLAGS_CHANGED_MASK:    CGEventMask = 1 << K_CG_EVENT_FLAGS_CHANGED;

    // macOS Virtual Key Codes
    pub const KVK_RIGHT_OPTION:  CGKeyCode = 61;
    pub const KVK_OPTION:        CGKeyCode = 58;
    pub const KVK_RIGHT_SHIFT:   CGKeyCode = 60;
    pub const KVK_SHIFT:         CGKeyCode = 56;
    pub const KVK_RIGHT_CONTROL: CGKeyCode = 62;
    pub const KVK_CONTROL:       CGKeyCode = 59;
    pub const KVK_FUNCTION:      CGKeyCode = 63;
    pub const KVK_CAPS_LOCK:     CGKeyCode = 57;
    pub const KVK_ESCAPE:        CGKeyCode = 53;

    type MachPort      = *mut c_void;
    type CFRunLoopSrc  = *mut c_void;
    type CFRunLoopRef  = *mut c_void;
    type CFStringRef   = *const c_void;
    type CGEventRef    = *mut c_void;

    pub type CGEventCb =
        unsafe extern "C" fn(*mut c_void, CGEventType, CGEventRef, *mut c_void) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32, place: u32, opts: u32,
            mask: CGEventMask, cb: CGEventCb, ud: *mut c_void,
        ) -> MachPort;
        fn CGEventTapEnable(tap: MachPort, enable: bool);
        fn CGEventGetIntegerValueField(ev: CGEventRef, field: u32) -> i64;
        fn CGEventGetFlags(ev: CGEventRef) -> u64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(alloc: *mut c_void, tap: MachPort, order: i32) -> CFRunLoopSrc;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopGetMain() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, src: CFRunLoopSrc, mode: CFStringRef);
        fn CFRunLoopWakeUp(rl: CFRunLoopRef);
        static kCFRunLoopCommonModes: CFStringRef;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    #[link(name = "System")]
    extern "C" {
        fn dispatch_async_f(
            queue: *mut c_void,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
        static _dispatch_main_q: c_void;
    }

    fn run_on_main_thread<F: FnOnce() + Send + 'static>(f: F) {
        extern "C" fn trampoline<F: FnOnce()>(context: *mut c_void) {
            unsafe {
                let b = Box::from_raw(context as *mut F);
                b();
            }
        }
        let boxed = Box::new(f);
        let raw = Box::into_raw(boxed) as *mut c_void;
        unsafe {
            let main_q = &_dispatch_main_q as *const _ as *mut c_void;
            dispatch_async_f(main_q, raw, trampoline::<F>);
            let rl = CFRunLoopGetMain();
            if !rl.is_null() {
                CFRunLoopWakeUp(rl);
            }
        }
    }

    const KEYCODE_FIELD: u32 = 9;

    // Official macOS CGEventFlags masks (CGEventTypes.h)
    const CG_FLAG_ALPHA_SHIFT:  u64 = 0x00010000; // Caps Lock
    const CG_FLAG_SHIFT:        u64 = 0x00020000; // Shift
    const CG_FLAG_CONTROL:      u64 = 0x00040000; // Control
    const CG_FLAG_ALTERNATE:    u64 = 0x00080000; // Option / Alt
    const CG_FLAG_COMMAND:      u64 = 0x00100000; // Command
    const CG_FLAG_SECONDARY_FN: u64 = 0x00800000; // Fn

    pub fn keycode_to_name(keycode: u16) -> &'static str {
        match keycode {
            61 => "Right Option",
            58 => "Left Option",
            60 => "Right Shift",
            56 => "Left Shift",
            62 => "Right Control",
            59 => "Left Control",
            55 => "Left Command",
            54 => "Right Command",
            57 => "Caps Lock",
            63 => "Fn / Globe",
            49 => "Space",
            48 => "Tab",
            36 => "Return",
            53 => "Escape",
            51 => "Delete",
            50 => "` (Backtick)",
            122 => "F1",
            120 => "F2",
            99 => "F3",
            118 => "F4",
            96 => "F5",
            97 => "F6",
            98 => "F7",
            100 => "F8",
            101 => "F9",
            109 => "F10",
            103 => "F11",
            111 => "F12",
            0 => "A",
            1 => "S",
            2 => "D",
            3 => "F",
            4 => "H",
            5 => "G",
            6 => "Z",
            7 => "X",
            8 => "C",
            9 => "V",
            11 => "B",
            12 => "Q",
            13 => "W",
            14 => "E",
            15 => "R",
            16 => "Y",
            17 => "T",
            31 => "O",
            32 => "U",
            34 => "I",
            35 => "P",
            37 => "L",
            38 => "J",
            40 => "K",
            45 => "N",
            46 => "M",
            18 => "1",
            19 => "2",
            20 => "3",
            21 => "4",
            23 => "5",
            22 => "6",
            26 => "7",
            28 => "8",
            25 => "9",
            29 => "0",
            _ => "Custom Key",
        }
    }

    pub fn keycode_to_config_str(keycode: u16) -> String {
        match keycode {
            61 => "right_alt".to_string(),
            58 => "option".to_string(),
            60 => "right_shift".to_string(),
            56 => "shift".to_string(),
            62 => "right_control".to_string(),
            59 => "control".to_string(),
            55 => "command".to_string(),
            57 => "caps_lock".to_string(),
            63 => "fn".to_string(),
            49 => "space".to_string(),
            48 => "tab".to_string(),
            100 => "f8".to_string(),
            122 => "f1".to_string(),
            120 => "f2".to_string(),
            99 => "f3".to_string(),
            118 => "f4".to_string(),
            96 => "f5".to_string(),
            97 => "f6".to_string(),
            98 => "f7".to_string(),
            101 => "f9".to_string(),
            109 => "f10".to_string(),
            103 => "f11".to_string(),
            111 => "f12".to_string(),
            other => format!("keycode:{}", other),
        }
    }

    pub fn hotkey_str_to_keycodes(hotkey: &str) -> Vec<CGKeyCode> {
        let lower = hotkey.trim().to_lowercase();
        if lower.starts_with("keycode:") {
            if let Ok(kc) = lower[8..].trim().parse::<u16>() {
                return vec![kc];
            }
        }
        if let Ok(kc) = lower.parse::<u16>() {
            return vec![kc];
        }
        match lower.as_str() {
            "right_alt" | "right_option"                      => vec![KVK_RIGHT_OPTION, KVK_OPTION],
            "alt" | "option" | "left_alt" | "left_option"     => vec![KVK_OPTION, KVK_RIGHT_OPTION],
            "right_shift" | "shift_right"                     => vec![KVK_RIGHT_SHIFT, KVK_SHIFT],
            "left_shift"  | "shift_left" | "shift"            => vec![KVK_SHIFT, KVK_RIGHT_SHIFT],
            "right_ctrl"  | "right_control"                   => vec![KVK_RIGHT_CONTROL, KVK_CONTROL],
            "left_ctrl"   | "left_control" | "ctrl" | "control"=> vec![KVK_CONTROL, KVK_RIGHT_CONTROL],
            "cmd" | "command" | "left_cmd" | "right_cmd"      => vec![55, 54],
            "fn" | "function" | "globe"                        => vec![KVK_FUNCTION],
            "caps_lock" | "capslock"                           => vec![KVK_CAPS_LOCK],
            "space"                                            => vec![49],
            "tab"                                              => vec![48],
            "backtick" | "grave"                               => vec![50],
            "f1" => vec![122], "f2" => vec![120], "f3" => vec![99], "f4" => vec![118],
            "f5" => vec![96], "f6" => vec![97], "f7" => vec![98], "f8" => vec![100],
            "f9" => vec![101], "f10" => vec![109], "f11" => vec![103], "f12" => vec![111],
            "a" => vec![0], "s" => vec![1], "d" => vec![2], "f" => vec![3],
            "z" => vec![6], "x" => vec![7], "c" => vec![8], "v" => vec![9],
            "q" => vec![12], "w" => vec![13], "e" => vec![14], "r" => vec![15],
            "any" | "all" | "modifier"                         => vec![KVK_RIGHT_OPTION, KVK_OPTION, KVK_RIGHT_SHIFT, KVK_SHIFT, KVK_CAPS_LOCK, KVK_FUNCTION, KVK_RIGHT_CONTROL, KVK_CONTROL],
            _                                                  => vec![KVK_RIGHT_OPTION, KVK_OPTION],
        }
    }

    static GLOBAL_TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
    static TAP_STATE: std::sync::OnceLock<TapState> = std::sync::OnceLock::new();

    pub struct TapState {
        pub keycodes:     std::sync::Arc<parking_lot::RwLock<Vec<CGKeyCode>>>,
        pub tx_key:       std::sync::Arc<dyn Fn(crate::AppEvent) + Send + Sync>,
        pub key_held:     std::sync::Arc<parking_lot::Mutex<bool>>,
        pub last_release: std::sync::Arc<parking_lot::Mutex<Option<std::time::Instant>>>,
        pub double_tap_ms: u64,
    }

    /// Dynamically update the active hotkey keycodes without restarting
    pub fn update_hotkey_keycodes(new_keys: Vec<CGKeyCode>) {
        if let Some(s) = TAP_STATE.get() {
            let mut guard = s.keycodes.write();
            *guard = new_keys;
            println!("[hotkey] ✓ Live updated active keycodes to: {:?}", *guard);
        }
    }

    /// Dynamically update the active hotkey from a string name (e.g. "right_shift", "option")
    pub fn update_hotkey_str(hotkey_str: &str) {
        let keys = hotkey_str_to_keycodes(hotkey_str);
        update_hotkey_keycodes(keys);
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: *mut c_void,
        etype: CGEventType,
        event: CGEventRef,
        userdata: *mut c_void,
    ) -> CGEventRef {
        // Auto-recover if macOS disabled tap
        if etype == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT || etype == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT {
            let port = GLOBAL_TAP_PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                CGEventTapEnable(port, true);
                println!("[hotkey] Tap re-enabled after timeout");
            }
            return event;
        }

        let state = if !userdata.is_null() {
            &*(userdata as *const TapState)
        } else if let Some(s) = TAP_STATE.get() {
            s
        } else {
            return event;
        };

        let keycode = CGEventGetIntegerValueField(event, KEYCODE_FIELD) as CGKeyCode;
        let flags = CGEventGetFlags(event);
        let keycodes_guard = state.keycodes.read();
        let is_hotkey = keycodes_guard.contains(&keycode);

        if is_hotkey || keycode == KVK_ESCAPE || etype == K_CG_EVENT_FLAGS_CHANGED {
            println!("[hotkey-event] etype={} keycode={} is_hotkey={} flags=0x{:08x} match_keys={:?}",
                etype, keycode, is_hotkey, flags, *keycodes_guard);
        }

        match etype {
            K_CG_EVENT_KEY_DOWN => {
                if keycode == KVK_ESCAPE {
                    (state.tx_key)(crate::AppEvent::Cancel);
                } else if is_hotkey {
                    fire_press(state);
                }
            }
            K_CG_EVENT_KEY_UP if is_hotkey => fire_release(state),

            K_CG_EVENT_FLAGS_CHANGED if is_hotkey => {
                let pressed = match keycode {
                    KVK_RIGHT_OPTION | KVK_OPTION       => (flags & CG_FLAG_ALTERNATE) != 0,
                    KVK_RIGHT_SHIFT  | KVK_SHIFT        => (flags & CG_FLAG_SHIFT) != 0,
                    KVK_RIGHT_CONTROL| KVK_CONTROL      => (flags & CG_FLAG_CONTROL) != 0,
                    55 | 54                             => (flags & CG_FLAG_COMMAND) != 0,
                    KVK_CAPS_LOCK                       => (flags & CG_FLAG_ALPHA_SHIFT) != 0,
                    KVK_FUNCTION                        => (flags & CG_FLAG_SECONDARY_FN) != 0,
                    _                                   => (flags & (CG_FLAG_ALTERNATE | CG_FLAG_SHIFT | CG_FLAG_CONTROL | CG_FLAG_COMMAND)) != 0,
                };
                let cur_held = *state.key_held.lock();
                if pressed && !cur_held {
                    fire_press(state);
                } else if !pressed && cur_held {
                    fire_release(state);
                }
            }
            _ => {}
        }
        event
    }

    fn fire_press(state: &TapState) {
        let mut held = state.key_held.lock();
        *held = true;
        let double = {
            let last = state.last_release.lock();
            last.map(|t| t.elapsed() < std::time::Duration::from_millis(state.double_tap_ms))
                .unwrap_or(false)
        };
        if double {
            println!("[hotkey] Double-tap detected → hands-free toggle");
            (state.tx_key)(crate::AppEvent::DoubleTap);
        } else {
            println!("[hotkey] Press detected → hold-to-talk");
            (state.tx_key)(crate::AppEvent::HotkeyPress);
        }
    }

    fn fire_release(state: &TapState) {
        let mut held = state.key_held.lock();
        *held = false;
        *state.last_release.lock() = Some(std::time::Instant::now());
        println!("[hotkey] Release detected");
        (state.tx_key)(crate::AppEvent::HotkeyRelease);
    }

    /// Synchronously install CGEventTap on the current (main) thread.
    /// If accessibility is missing, fall back to background poller.
    pub fn install(state: TapState) {
        let _ = TAP_STATE.set(state);
        let state_ref = TAP_STATE.get().expect("TapState initialized");
        let state_ptr = state_ref as *const TapState as *mut c_void;
        let mask = K_CG_KEY_DOWN_MASK | K_CG_KEY_UP_MASK | K_CG_FLAGS_CHANGED_MASK;

        unsafe {
            let tap = CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_DEFAULT,
                mask,
                event_tap_callback,
                state_ptr,
            );

            if !tap.is_null() {
                GLOBAL_TAP_PORT.store(tap, Ordering::SeqCst);
                let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, src, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                println!("[hotkey] ✓ CGEventTap active on main RunLoop! Press Right Option to dictate.");
                return;
            }

            eprintln!("[hotkey] ⚠️ CGEventTapCreate failed synchronously — starting background retry...");
        }

        // If direct install failed, poll in background
        std::thread::spawn(move || {
            loop {
                if unsafe { AXIsProcessTrusted() } {
                    println!("[hotkey] ✓ Accessibility granted — scheduling tap on main thread");
                    break;
                }
                eprintln!("[hotkey] Waiting for Accessibility permission...");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }

            run_on_main_thread(move || unsafe {
                let state_ref = TAP_STATE.get().expect("TapState initialized");
                let state_ptr = state_ref as *const TapState as *mut c_void;
                let tap = CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_DEFAULT,
                    mask,
                    event_tap_callback,
                    state_ptr,
                );

                if tap.is_null() {
                    eprintln!("[hotkey] ❌ CGEventTapCreate retry failed.");
                    return;
                }

                GLOBAL_TAP_PORT.store(tap, Ordering::SeqCst);
                let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, src, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                println!("[hotkey] ✓ CGEventTap active on main RunLoop! Press Right Option to dictate.");
            });
        });
    }
}
