//! Test utility for debugging keyboard keycodes on macOS using CGEventTap.
//! Run with: cargo run --bin test_hotkey
//! Press any key to see what keycode macOS reports.

#[cfg(target_os = "macos")]
fn main() {
    use std::ffi::c_void;

    type CGEventType = u32;
    type CGEventMask = u64;

    const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
    const K_CG_EVENT_KEY_UP: CGEventType = 11;
    const K_CG_EVENT_FLAGS_CHANGED: CGEventType = 12;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    type MachPort = *mut c_void;
    type CFRunLoopSource = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CGEventRef = *mut c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(tap: u32, place: u32, options: u32, events: CGEventMask,
            callback: unsafe extern "C" fn(*mut c_void, CGEventType, CGEventRef, *mut c_void) -> CGEventRef,
            user_info: *mut c_void) -> MachPort;
        fn CGEventTapEnable(tap: MachPort, enable: bool);
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventGetFlags(event: CGEventRef) -> u64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(alloc: *mut c_void, tap: MachPort, order: i32) -> CFRunLoopSource;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSource, mode: CFStringRef);
        fn CFRunLoopRun();
        static kCFRunLoopCommonModes: CFStringRef;
    }

    unsafe extern "C" fn callback(
        _proxy: *mut c_void,
        etype: CGEventType,
        event: CGEventRef,
        _ud: *mut c_void,
    ) -> CGEventRef {
        unsafe {
            let kc = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
            let flags = CGEventGetFlags(event);
            let type_name = match etype {
                10 => "KeyDown",
                11 => "KeyUp",
                12 => "FlagsChanged",
                _ => "Other",
            };
            println!("[{}] keycode={} flags=0x{:08x}", type_name, kc, flags);
        }
        event
    }

    println!("Keryx Hotkey Debugger — press any modifier keys (Option, Shift, Control, Fn...)");
    println!("Ctrl+C to stop.\n");

    unsafe {
        let mask: CGEventMask = (1 << K_CG_EVENT_KEY_DOWN) | (1 << K_CG_EVENT_KEY_UP) | (1 << K_CG_EVENT_FLAGS_CHANGED);
        let tap = CGEventTapCreate(K_CG_SESSION_EVENT_TAP, K_CG_HEAD_INSERT_EVENT_TAP, 0, mask, callback, std::ptr::null_mut());
        if tap.is_null() {
            eprintln!("❌ CGEventTapCreate failed — enable Accessibility permission for Terminal/this app");
            std::process::exit(1);
        }
        let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        println!("✓ Tap installed. Listening...\n");
        CFRunLoopRun();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    use rdev::{listen, Event, EventType};
    println!("Press any keys (Ctrl+C to stop)...");
    listen(move |event: Event| {
        match event.event_type {
            EventType::KeyPress(key) => println!("--> KeyPress: {:?}", key),
            EventType::KeyRelease(key) => println!("<-- KeyRelease: {:?}", key),
            _ => {}
        }
    }).unwrap();
}
