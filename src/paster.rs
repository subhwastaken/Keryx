#![allow(dead_code)]

use arboard::Clipboard;
use std::thread;
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
use enigo::{Enigo, Key, Keyboard, Settings};

#[cfg(target_os = "macos")]
mod macos_paster {
    use std::ffi::c_void;

    type CGEventSourceRef = *mut c_void;
    type CGEventRef = *mut c_void;
    type CGKeyCode = u16;
    type CGEventTapLocation = u32;

    const K_CG_HID_EVENT_TAP: CGEventTapLocation = 0;
    const K_CG_SESSION_EVENT_TAP: CGEventTapLocation = 1;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
    const KEY_V: CGKeyCode = 9;
    const KEY_C: CGKeyCode = 8;
    const KEY_BACKSPACE: CGKeyCode = 51;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    pub fn copy_cmd_c() -> Result<(), String> {
        unsafe {
            let event_down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_C, true);
            if event_down.is_null() {
                return Err("Failed to create CoreGraphics keydown event".to_string());
            }
            CGEventSetFlags(event_down, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, event_down);
            CFRelease(event_down as *const c_void);

            std::thread::sleep(std::time::Duration::from_millis(15));

            let event_up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_C, false);
            if !event_up.is_null() {
                CGEventSetFlags(event_up, K_CG_EVENT_FLAG_MASK_COMMAND);
                CGEventPost(K_CG_HID_EVENT_TAP, event_up);
                CFRelease(event_up as *const c_void);
            }
        }
        Ok(())
    }

    pub fn paste_cmd_v() -> Result<(), String> {
        unsafe {
            // Post Cmd+V down ONCE to HID event tap (universal hardware queue)
            let event_down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_V, true);
            if event_down.is_null() {
                return Err("Failed to create CoreGraphics keydown event".to_string());
            }
            CGEventSetFlags(event_down, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, event_down);
            CFRelease(event_down as *const c_void);

            std::thread::sleep(std::time::Duration::from_millis(15));

            // Post Cmd+V up ONCE
            let event_up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_V, false);
            if !event_up.is_null() {
                CGEventSetFlags(event_up, K_CG_EVENT_FLAG_MASK_COMMAND);
                CGEventPost(K_CG_HID_EVENT_TAP, event_up);
                CFRelease(event_up as *const c_void);
            }
        }
        Ok(())
    }

    pub fn backspace(count: usize) -> Result<(), String> {
        for _ in 0..count {
            unsafe {
                let down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_BACKSPACE, true);
                if !down.is_null() {
                    CGEventPost(K_CG_SESSION_EVENT_TAP, down);
                    CFRelease(down as *const c_void);
                }
                let up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), KEY_BACKSPACE, false);
                if !up.is_null() {
                    CGEventPost(K_CG_SESSION_EVENT_TAP, up);
                    CFRelease(up as *const c_void);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }
}

/// Pastes text at the current cursor position anywhere in the system (Browser, Notes, Slack, IDEs)
/// Preserves and restores the user's previous clipboard contents automatically so voice typing doesn't destroy copied text.
pub fn paste_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    // 1. Save prior clipboard content (if any)
    let previous_clipboard: Option<String> = Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok());

    // 2. Set system clipboard with dictated text
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Clipboard set error: {e}"))?;

    // Allow clipboard to propagate to OS window server
    thread::sleep(Duration::from_millis(40));

    // 3. Simulate Command+V (or Ctrl+V) at active cursor
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = macos_paster::paste_cmd_v() {
            eprintln!("[paster] CoreGraphics paste failed: {e}. Trying AppleScript fallback...");
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg("tell application \"System Events\" to keystroke \"v\" using command down")
                .status();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("Enigo error: {e}"))?;
        enigo
            .key(Key::Control, enigo::Direction::Press)
            .map_err(|e| format!("Key press error: {e}"))?;
        enigo
            .key(Key::Unicode('v'), enigo::Direction::Click)
            .map_err(|e| format!("Key click error: {e}"))?;
        enigo
            .key(Key::Control, enigo::Direction::Release)
            .map_err(|e| format!("Key release error: {e}"))?;
    }

    // 4. Non-destructively restore prior clipboard content in the background after app consumes Cmd+V
    if let Some(prev_text) = previous_clipboard {
        let pasted_text = text.to_string();
        std::thread::spawn(move || {
            // Wait 250ms for target app (VS Code, Chrome, Slack) to read the clipboard via Cmd+V
            thread::sleep(Duration::from_millis(250));
            if let Ok(mut cb) = Clipboard::new() {
                // Only restore if the clipboard still contains what we pasted (user hasn't copied something new)
                if let Ok(current_text) = cb.get_text() {
                    if current_text == pasted_text {
                        let _ = cb.set_text(prev_text);
                    }
                }
            }
        });
    }

    Ok(())
}

/// Sends N backspace keypresses directly to the active application (for voice command deletions)
pub fn backspace_chars(count: usize) -> Result<(), String> {
    if count == 0 {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        macos_paster::backspace(count)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("Enigo error: {e}"))?;
        for _ in 0..count {
            let _ = enigo.key(Key::Backspace, enigo::Direction::Click);
        }
    }

    Ok(())
}

/// Attempts to read the currently selected / highlighted text by simulating Cmd+C (or Ctrl+C)
pub fn get_selected_text() -> Result<Option<String>, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    let orig = clipboard.get_text().unwrap_or_default();

    // Clear clipboard temporarily with a unique marker to detect if copy succeeded
    let marker = "__keryx_selection_marker__";
    let _ = clipboard.set_text(marker.to_string());
    thread::sleep(Duration::from_millis(20));

    #[cfg(target_os = "macos")]
    {
        let _ = macos_paster::copy_cmd_c();
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
            let _ = enigo.key(Key::Control, enigo::Direction::Press);
            let _ = enigo.key(Key::Unicode('c'), enigo::Direction::Click);
            let _ = enigo.key(Key::Control, enigo::Direction::Release);
        }
    }

    thread::sleep(Duration::from_millis(60));

    let selected = clipboard.get_text().unwrap_or_default();
    if selected != marker && !selected.trim().is_empty() {
        Ok(Some(selected))
    } else {
        // Restore original clipboard after short delay to prevent paste race condition after short delay to prevent paste race condition after short delay to prevent paste race condition after short delay to prevent paste race condition content
        let _ = clipboard.set_text(orig);
        Ok(None)
    }
}

/// Replaces previously pasted draft text by backspacing its length and pasting the refined text
#[allow(dead_code)]
pub fn replace_text(old_text: &str, new_text: &str) -> Result<(), String> {
    let old_trimmed = old_text.trim();
    let new_trimmed = new_text.trim();

    if old_trimmed == new_trimmed || old_trimmed.is_empty() {
        if !new_trimmed.is_empty() && old_trimmed.is_empty() {
            return paste_text(new_trimmed);
        }
        return Ok(());
    }

    let char_count = old_trimmed.chars().count();
    println!("[paster] Rectifying text: backspacing {} characters...", char_count);
    backspace_chars(char_count)?;
    thread::sleep(Duration::from_millis(25));
    paste_text(new_trimmed)
}
