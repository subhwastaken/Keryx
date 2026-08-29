#![allow(dead_code)]
/// Desktop notifications for HUD feedback
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Pass title and message as separate positional argv arguments to osascript.
        // This completely eliminates AppleScript interpolation and injection vulnerabilities.
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg("on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run")
            .arg(title)
            .arg(message)
            .spawn();
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Use notify-rust on other platforms
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(message)
            .show();
    }
}

pub fn notify_recording_start() {
    notify("Keryx 🎙", "Recording... (release to transcribe)");
}

pub fn notify_hands_free_start() {
    notify("Keryx 🎙", "Hands-free mode — tap again to stop");
}

pub fn notify_transcribing() {
    notify("Keryx ⚡", "Transcribing...");
}

pub fn notify_done(text: &str) {
    let char_count = text.chars().count();
    let preview = if char_count > 60 {
        format!("{}...", text.chars().take(60).collect::<String>())
    } else {
        text.to_string()
    };
    notify("Keryx ✅", &preview);
}

pub fn notify_cancelled() {
    notify("Keryx ✗", "Cancelled");
}

pub fn notify_error(err: &str) {
    notify("Keryx ⚠", err);
}
