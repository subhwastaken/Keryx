# KERYX Design System & Color Palette

This document defines the official visual design language, color tokens, typography, and UI specifications for **Keryx**.

---

## 1. Official Color Palette (Monochromatic Royal Blue System)

| Token Name | Hex Code | RGB (f64 / Cocoa) | Usage & Role |
| :--- | :--- | :--- | :--- |
| **Sapphire Blue** | `#2563EB` | `(0.145, 0.388, 0.922)` | Waveform Bar 0, Primary action buttons, Key focus rings |
| **Royal Blue** | `#3B82F6` | `(0.231, 0.510, 0.965)` | Waveform Bar 1, Section header accents (`GENERAL`, `MODELS`) |
| **Sky Blue** | `#60A5FA` | `(0.376, 0.647, 0.980)` | Waveform Bar 2, Active status pills, Success feedback text |
| **Ice Blue** | `#93C5FD` | `(0.576, 0.773, 0.992)` | Waveform Bar 3, Subtitles, Helper text, Dropdown highlights |
| **Pure White** | `#FFFFFF` | `(1.000, 1.000, 1.000)` | Main `KERYX` brand logo, Window title, Primary values |
| **Cool Soft White** | `#F1F5F9` | `(0.940, 0.960, 0.980)` | Field labels, Form prompt text |
| **Midnight Surface** | `#0D1117` | `(0.051, 0.067, 0.090)` | Settings window backdrop, Base background layer |
| **Deep Slate Card** | `#131823` | `(0.075, 0.094, 0.137)` | Section card containers, Form groupings |
| **Hairline Border** | `#1E293B` | `(0.118, 0.161, 0.231)` | 1px card borders (0.85 opacity), Input dividers |

---

## 2. Dynamic Notch HUD Specifications ([`src/hud_overlay.rs`](file:///Volumes/WD_Subharup/wisprflow-rs/src/hud_overlay.rs))

- **Dimensions:** Width `175.0pt`, Height `24.0pt`, Corner Radius `12.0pt` (Full capsule).
- **Position:** Flush at top-center of active display (`y = screen_height - 24.0`).
- **Container Color:** Jet Black (`#000000`, 98% opacity) with 0.5px border (`#FFFFFF`, 12% opacity).
- **Equalizer Waveform (4 Animated Bars):**
  - Bar Width: `2.5pt`, Bar Gap: `2.0pt`, Corner Radius: `1.25pt`.
  - Vertical Midpoint: `12.0pt` (symmetrically centered).
  - Heights: Smooth audio amplitude scaling between `min_h = 3.5pt` and `max_h = 13.5pt`.
  - Gradient Stops:
    $$\text{Bar 0: \#2563EB} \longrightarrow \text{Bar 1: \#3B82F6} \longrightarrow \text{Bar 2: \#60A5FA} \longrightarrow \text{Bar 3: \#93C5FD}$$
- **Capsule Typography:**
  - Font: `SF Pro Display`, `11.5pt`, Regular / Medium.
  - Text Color: `#F1F5F9` (`0.94, 0.94, 0.96`).
  - Vertical Baseline: Aligned at `y = 1.5pt` / `h = 17.0pt` to match the exact `12.0pt` centerline of the equalizer bars.

---

## 3. Settings Window Specifications ([`src/settings_gui.rs`](file:///Volumes/WD_Subharup/wisprflow-rs/src/settings_gui.rs))

- **Window Dimensions:** `540.0pt` $\times$ `690.0pt` (`NSWindowStyleMaskTitled | NSWindowStyleMaskClosable`).
- **Backdrop:** Midnight Slate (`#0D1117`).
- **Brand Header:**
  - `KERYX` in pure crisp white (`#FFFFFF`, `22.0pt` bold).
  - Subtitle in Ice Blue (`#93C5FD`, `11.5pt` regular).
- **Cards & Layout:**
  - Rounded cards (`10.0pt` radius) in Deep Slate (`#131823`).
  - 1px hairline border in Slate Blue (`#1E293B`, 85% opacity).
  - Section Headers in Royal Blue (`#3B82F6`, `10.5pt` bold uppercase).
  - Labels in Cool Soft White (`#F1F5F9`, `12.0pt`).
- **Interactive Controls:**
  - Hotkey Picker: Dropdown (`NSPopUpButton`) with instant selection + `[ Capture Key... ]` interactive recorder.
  - Form Fields: Bordered Cocoa `NSTextField` with secure masks where appropriate.
  - Primary Action Button: "Save & Apply" with Return key equivalent (`\r`).
