#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
use cocoa::appkit::{
    NSColor, NSScreen, NSWindow,
};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil, NO, YES};
#[cfg(target_os = "macos")]
use cocoa::foundation::{NSRect, NSString};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
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

pub struct HudOverlay {
    #[cfg(target_os = "macos")]
    window: Arc<Mutex<Option<SendPtr>>>,
    #[cfg(target_os = "macos")]
    label: Arc<Mutex<Option<SendPtr>>>,
    #[cfg(target_os = "macos")]
    bars: Arc<Mutex<Vec<SendPtr>>>,
    generation: Arc<AtomicU64>,
}

unsafe impl Send for HudOverlay {}
unsafe impl Sync for HudOverlay {}

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

impl Default for HudOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl HudOverlay {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            HudOverlay {
                window: Arc::new(Mutex::new(None)),
                label: Arc::new(Mutex::new(None)),
                bars: Arc::new(Mutex::new(Vec::new())),
                generation: Arc::new(AtomicU64::new(0)),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            HudOverlay {}
        }
    }

    /// Shows the MacBook Dynamic Notch / Island HUD at the top center of the screen
    pub fn show(&self, initial_text: &str) {
        let text = initial_text.to_string();
        #[cfg(target_os = "macos")]
        {
            self.generation.fetch_add(1, Ordering::SeqCst);
            let win_arc = self.window.clone();
            let lbl_arc = self.label.clone();
            let bars_arc = self.bars.clone();

            run_on_main_thread(move || unsafe {
                let mut win_guard = win_arc.lock();
                let mut lbl_guard = lbl_arc.lock();
                let mut bars_guard = bars_arc.lock();

                let screen = NSScreen::mainScreen(nil);
                let frame: NSRect = if !screen.is_null() {
                    msg_send![screen, frame]
                } else {
                    NSRect::new(
                        cocoa::foundation::NSPoint::new(0.0, 0.0),
                        cocoa::foundation::NSSize::new(1440.0, 900.0),
                    )
                };

                let width = 175.0;
                let height = 24.0;
                let x = frame.origin.x + (frame.size.width - width) / 2.0;
                // Position flush at the very top center of the screen
                let y = frame.origin.y + frame.size.height - height;

                let rect = NSRect::new(
                    cocoa::foundation::NSPoint::new(x, y),
                    cocoa::foundation::NSSize::new(width, height),
                );

                if win_guard.is_none() {
                    let window: id = msg_send![class!(NSPanel), alloc];
                    // NSWindowStyleMaskBorderless (0) | NSWindowStyleMaskNonactivatingPanel (1 << 7) = 128
                    let window = window.initWithContentRect_styleMask_backing_defer_(
                        rect,
                        cocoa::appkit::NSWindowStyleMask::from_bits_truncate(128),
                        cocoa::appkit::NSBackingStoreBuffered,
                        NO,
                    );

                    window.setLevel_(102); // kCGOverlayWindowLevelKey (above all apps, full screens & menu bar)
                    window.setBackgroundColor_(NSColor::clearColor(nil));
                    window.setOpaque_(NO);
                    window.setHasShadow_(YES);
                    window.setIgnoresMouseEvents_(YES);
                    let _: () = msg_send![window, setHidesOnDeactivate: NO];
                    let _: () = msg_send![window, setCanHide: NO];

                    // 1 (CanJoinAllSpaces) | 16 (Stationary) | 64 (IgnoresCycle) | 256 (FullScreenAuxiliary)
                    let behavior = cocoa::appkit::NSWindowCollectionBehavior::from_bits_truncate(1 | 16 | 64 | 256);
                    window.setCollectionBehavior_(behavior);

                    // Jet black container
                    let content_view: id = window.contentView();
                    let _: () = msg_send![content_view, setWantsLayer: YES];
                    let layer: id = msg_send![content_view, layer];
                    let _: () = msg_send![layer, setCornerRadius: 12.0];
                    let _: () = msg_send![layer, setMasksToBounds: YES];

                    let bg_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(
                        nil, 0.0, 0.0, 0.0, 0.98,
                    );
                    let cg_bg: id = msg_send![bg_color, CGColor];
                    let _: () = msg_send![layer, setBackgroundColor: cg_bg];

                    let border_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(
                        nil, 1.0, 1.0, 1.0, 0.12,
                    );
                    let cg_border: id = msg_send![border_color, CGColor];
                    let _: () = msg_send![layer, setBorderColor: cg_border];
                    let _: () = msg_send![layer, setBorderWidth: 0.5f64];

                    // Create 4 animated waveform equalizer sticks
                    bars_guard.clear();
                    let bar_count = 4;
                    let bar_w = 2.5;
                    let bar_gap = 2.0;
                    let start_x = 10.0;
                    let center_y = height / 2.0;

                    // Single-family monochromatic Blue gradient: Deep Sapphire (#2563EB) -> Royal Blue (#3B82F6) -> Sky Blue (#60A5FA) -> Ice Blue (#93C5FD)
                    let gradient_colors = [
                        (0.145f64, 0.388f64, 0.922f64), // #2563EB (Sapphire Blue)
                        (0.231f64, 0.510f64, 0.965f64), // #3B82F6 (Royal Blue)
                        (0.376f64, 0.647f64, 0.980f64), // #60A5FA (Sky Blue)
                        (0.576f64, 0.773f64, 0.992f64), // #93C5FD (Ice Blue)
                    ];

                    for i in 0..bar_count {
                        let bx = start_x + (i as f64) * (bar_w + bar_gap);
                        let bh = 5.0 + (i as f64 % 2.0) * 2.5;
                        let by = center_y - (bh / 2.0);

                        let bar_rect = NSRect::new(
                            cocoa::foundation::NSPoint::new(bx, by),
                            cocoa::foundation::NSSize::new(bar_w, bh),
                        );
                        let bar_view: id = msg_send![class!(NSView), alloc];
                        let bar_view: id = msg_send![bar_view, initWithFrame: bar_rect];
                        let _: () = msg_send![bar_view, setWantsLayer: YES];
                        let blayer: id = msg_send![bar_view, layer];
                        let _: () = msg_send![blayer, setCornerRadius: 1.25];

                        let (r, g, b) = gradient_colors[i % gradient_colors.len()];
                        let bar_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(
                            nil, r, g, b, 0.95,
                        );
                        let cg_bar: id = msg_send![bar_color, CGColor];
                        let _: () = msg_send![blayer, setBackgroundColor: cg_bar];

                        let _: () = msg_send![content_view, addSubview: bar_view];
                        bars_guard.push(SendPtr::from_id(bar_view));
                    }

                    // Create status text label vertically centered with the equalizer bars
                    let label_x = start_x + (bar_count as f64) * (bar_w + bar_gap) + 8.0;
                    let label_h = 17.0;
                    let label_y = 1.5; // Visually aligns text baseline and cap-height with the bar center
                    let lbl_rect = NSRect::new(
                        cocoa::foundation::NSPoint::new(label_x, label_y),
                        cocoa::foundation::NSSize::new(width - label_x - 6.0, label_h),
                    );
                    let label: id = msg_send![class!(NSTextField), alloc];
                    let label: id = msg_send![label, initWithFrame: lbl_rect];

                    let _: () = msg_send![label, setBezeled: NO];
                    let _: () = msg_send![label, setDrawsBackground: NO];
                    let _: () = msg_send![label, setEditable: NO];
                    let _: () = msg_send![label, setSelectable: NO];
                    let _: () = msg_send![label, setAlignment: 0];

                    let cell: id = msg_send![label, cell];
                    let _: () = msg_send![cell, setUsesSingleLineMode: YES];
                    let _: () = msg_send![cell, setWraps: NO];

                    let ns_str = NSString::alloc(nil).init_str(&text);
                    let _: () = msg_send![label, setStringValue: ns_str];

                    let txt_color = NSColor::colorWithCalibratedRed_green_blue_alpha_(
                        nil, 0.94, 0.94, 0.96, 1.0,
                    );
                    let _: () = msg_send![label, setTextColor: txt_color];

                    // SF Pro font size 11.5
                    let font: id = msg_send![class!(NSFont), systemFontOfSize: 11.5f64];
                    let _: () = msg_send![label, setFont: font];

                    let _: () = msg_send![content_view, addSubview: label];

                    *win_guard = Some(SendPtr::from_id(window));
                    *lbl_guard = Some(SendPtr::from_id(label));
                }

                if let Some(win_ptr) = *win_guard {
                    let win = win_ptr.to_id();
                    let cur_alpha: f64 = msg_send![win, alphaValue];
                    let _: () = msg_send![win, setFrame: rect display: YES];

                    if cur_alpha < 0.5 {
                        // Smoothly fade in in place
                        let _: () = msg_send![win, setAlphaValue: 0.0f64];
                        let _: () = msg_send![win, orderFrontRegardless];

                        let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
                        let ctx: id = msg_send![class!(NSAnimationContext), currentContext];
                        let _: () = msg_send![ctx, setDuration: 0.16f64];
                        let _: () = msg_send![ctx, setAllowsImplicitAnimation: YES];

                        let animator: id = msg_send![win, animator];
                        let _: () = msg_send![animator, setAlphaValue: 1.0f64];

                        let _: () = msg_send![class!(NSAnimationContext), endGrouping];
                    } else {
                        let _: () = msg_send![win, setAlphaValue: 1.0f64];
                        let _: () = msg_send![win, orderFrontRegardless];
                    }
                }
                if let Some(lbl_ptr) = *lbl_guard {
                    let lbl = lbl_ptr.to_id();
                    let ns_str = NSString::alloc(nil).init_str(&text);
                    let _: () = msg_send![lbl, setStringValue: ns_str];
                }
            });
        }
    }

    /// Updates live waveform sticks height based on audio amplitude level (0.0 to 1.0)
    pub fn update_audio_level(&self, level: f32) {
        #[cfg(target_os = "macos")]
        {
            let bars_arc = self.bars.clone();
            run_on_main_thread(move || unsafe {
                let bars_guard = bars_arc.lock();
                let bar_count = bars_guard.len();
                if bar_count == 0 {
                    return;
                }

                let clamped = level.clamp(0.05, 1.0);
                let max_h = 13.0;
                let min_h = 3.0;
                let height = 24.0;
                let center_y = height / 2.0;

                // Stagger heights across equalizer sticks for organic wave look
                let multipliers = [0.70, 1.15, 1.0, 0.65];

                for (i, bar_ptr) in bars_guard.iter().enumerate() {
                    let bar_view = bar_ptr.to_id();
                    let mult = multipliers.get(i).copied().unwrap_or(1.0);
                    let bh = (min_h + (max_h - min_h) * clamped as f64 * mult).clamp(min_h, max_h);
                    let by = center_y - (bh / 2.0);

                    let frame: NSRect = msg_send![bar_view, frame];
                    let new_frame = NSRect::new(
                        cocoa::foundation::NSPoint::new(frame.origin.x, by),
                        cocoa::foundation::NSSize::new(frame.size.width, bh),
                    );
                    let _: () = msg_send![bar_view, setFrame: new_frame];
                }
            });
        }
    }

    /// Updates live text displayed on the Dynamic Notch pill
    pub fn update_text(&self, text: &str) {
        let text = text.to_string();
        #[cfg(target_os = "macos")]
        {
            let lbl_arc = self.label.clone();
            run_on_main_thread(move || unsafe {
                if let Some(lbl_ptr) = *lbl_arc.lock() {
                    let lbl = lbl_ptr.to_id();
                    let ns_str = NSString::alloc(nil).init_str(&text);
                    let _: () = msg_send![lbl, setStringValue: ns_str];
                }
            });
        }
    }

    /// Hides the Dynamic Notch HUD with a clean in-place minimalist fade out
    pub fn hide(&self) {
        #[cfg(target_os = "macos")]
        {
            let current_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let win_arc = self.window.clone();
            run_on_main_thread(move || unsafe {
                if let Some(win_ptr) = *win_arc.lock() {
                    let win = win_ptr.to_id();

                    let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
                    let ctx: id = msg_send![class!(NSAnimationContext), currentContext];
                    let _: () = msg_send![ctx, setDuration: 0.18f64];
                    let _: () = msg_send![ctx, setAllowsImplicitAnimation: YES];

                    let animator: id = msg_send![win, animator];
                    let _: () = msg_send![animator, setAlphaValue: 0.0f64];

                    let _: () = msg_send![class!(NSAnimationContext), endGrouping];
                }
            });

            let win_arc_delayed = self.window.clone();
            let gen_arc = self.generation.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                run_on_main_thread(move || unsafe {
                    // Only order out if no new show() was triggered during the 200ms animation
                    if gen_arc.load(Ordering::SeqCst) == current_gen {
                        if let Some(win_ptr) = *win_arc_delayed.lock() {
                            let win = win_ptr.to_id();
                            let alpha: f64 = msg_send![win, alphaValue];
                            if alpha <= 0.05 {
                                let _: () = msg_send![win, orderOut: nil];
                            }
                        }
                    }
                });
            });
        }
    }
}

