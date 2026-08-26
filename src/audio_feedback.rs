#![allow(dead_code)]
#![allow(unexpected_cfgs)]

//! Subtle audio & haptic feedback chimes for Keryx
//! Provides instant non-visual feedback when recording starts and finishes.

#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use cocoa::foundation::NSString;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};

/// Play a subtle, soft click/pop sound when recording starts
pub fn play_start_chime() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| unsafe {
            // "Pop" or "Tink" sound from macOS System Library
            let sound_name = NSString::alloc(nil).init_str("Pop");
            let sound_cls = class!(NSSound);
            let sound: id = msg_send![sound_cls, soundNamed: sound_name];
            if !sound.is_null() {
                let () = msg_send![sound, setVolume: 0.35f32];
                let () = msg_send![sound, play];
            }
        });
    }
}

/// Play a subtle "tink" success sound when transcription & paste finishes
pub fn play_success_chime() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| unsafe {
            let sound_name = NSString::alloc(nil).init_str("Tink");
            let sound_cls = class!(NSSound);
            let sound: id = msg_send![sound_cls, soundNamed: sound_name];
            if !sound.is_null() {
                let () = msg_send![sound, setVolume: 0.40f32];
                let () = msg_send![sound, play];
            }
        });
    }
}

/// Play a soft cancel/dismiss sound when user cancels recording (e.g. Esc)
pub fn play_cancel_chime() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| unsafe {
            let sound_name = NSString::alloc(nil).init_str("Basso");
            let sound_cls = class!(NSSound);
            let sound: id = msg_send![sound_cls, soundNamed: sound_name];
            if !sound.is_null() {
                let () = msg_send![sound, setVolume: 0.20f32];
                let () = msg_send![sound, play];
            }
        });
    }
}

/// Triggers subtle macOS trackpad haptic feedback (Generic / Alignment)
pub fn trigger_haptic_feedback() {
    #[cfg(target_os = "macos")]
    {
        unsafe {
            let performer_cls = class!(NSHapticFeedbackManager);
            let performer: id = msg_send![performer_cls, defaultPerformer];
            if !performer.is_null() {
                // 0 = Generic, 1 = Alignment, 2 = LevelChange
                let () = msg_send![performer, performFeedbackPattern: 0 performanceTime: 0];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedback_functions_callable() {
        // Ensure no panics during invocation
        play_start_chime();
        play_success_chime();
        play_cancel_chime();
        trigger_haptic_feedback();
    }
}
