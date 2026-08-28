#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub enum AppCategory {
    Coding,
    Chat,
    Email,
    Notes,
    Browser,
    General,
}

#[derive(Debug, Clone)]
pub struct ActiveAppInfo {
    pub name: String,
    pub bundle_id: String,
    pub category: AppCategory,
}

#[cfg(target_os = "macos")]
pub fn get_active_app() -> ActiveAppInfo {
    use cocoa::base::id;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front_app: id = msg_send![workspace, frontmostApplication];

        if front_app.is_null() {
            return ActiveAppInfo {
                name: "Unknown".to_string(),
                bundle_id: "unknown".to_string(),
                category: AppCategory::General,
            };
        }

        let name_ns: id = msg_send![front_app, localizedName];
        let bundle_ns: id = msg_send![front_app, bundleIdentifier];

        let name = if !name_ns.is_null() {
            let utf8: *const std::os::raw::c_char = msg_send![name_ns, UTF8String];
            if !utf8.is_null() {
                std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string()
            } else {
                "Unknown".to_string()
            }
        } else {
            "Unknown".to_string()
        };

        let bundle_id = if !bundle_ns.is_null() {
            let utf8: *const std::os::raw::c_char = msg_send![bundle_ns, UTF8String];
            if !utf8.is_null() {
                std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        };

        let category = categorize_app(&bundle_id, &name);

        ActiveAppInfo {
            name,
            bundle_id,
            category,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_active_app() -> ActiveAppInfo {
    ActiveAppInfo {
        name: "Desktop App".to_string(),
        bundle_id: "generic".to_string(),
        category: AppCategory::General,
    }
}

fn categorize_app(bundle_id: &str, name: &str) -> AppCategory {
    let bid = bundle_id.to_lowercase();
    let n = name.to_lowercase();

    if bid.contains("vscode")
        || bid.contains("cursor")
        || bid.contains("vscodium")
        || bid.contains("xcode")
        || bid.contains("intellij")
        || bid.contains("pycharm")
        || bid.contains("clion")
        || bid.contains("rustrover")
        || bid.contains("webstorm")
        || bid.contains("terminal")
        || bid.contains("iterm")
        || bid.contains("warp")
        || bid.contains("alacritty")
        || bid.contains("kitty")
        || bid.contains("zed")
        || n.contains("terminal")
        || n.contains("iterm")
    {
        AppCategory::Coding
    } else if bid.contains("slack")
        || bid.contains("telegram")
        || bid.contains("discord")
        || bid.contains("whatsapp")
        || bid.contains("messages")
        || bid.contains("teams")
        || bid.contains("mattermost")
    {
        AppCategory::Chat
    } else if bid.contains("mail")
        || bid.contains("outlook")
        || bid.contains("superhuman")
        || bid.contains("thunderbird")
        || bid.contains("gmail")
    {
        AppCategory::Email
    } else if bid.contains("notes")
        || bid.contains("notion")
        || bid.contains("obsidian")
        || bid.contains("bear")
        || bid.contains("craft")
        || bid.contains("logseq")
        || bid.contains("linear")
    {
        AppCategory::Notes
    } else if bid.contains("chrome")
        || bid.contains("safari")
        || bid.contains("brave")
        || bid.contains("firefox")
        || bid.contains("arc")
        || bid.contains("edge")
    {
        AppCategory::Browser
    } else {
        AppCategory::General
    }
}
