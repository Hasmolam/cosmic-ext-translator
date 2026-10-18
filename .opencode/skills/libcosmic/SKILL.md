---
name: libcosmic Best Practices & Architecture
description: Architecture, layer-shell rules, multi-window gotchas, theme conventions, and rendering best practices for System76's libcosmic and COSMIC Desktop applications.
---

# libcosmic Best Practices & Architecture Guide

This guide establishes the architectural standards, operational gotchas, and rendering rules for building native applications and panel applets with System76's `libcosmic` GUI toolkit on the COSMIC Desktop (Wayland / `cosmic-comp`).

---

## 1. Entrypoints: App vs Applet

`libcosmic` provides two distinct application runtime models:

### Standalone Application (`cosmic::app::run`)
- Used for regular desktop applications (e.g., text editors, file managers, overlays).
- Entrypoint: `cosmic::app::run::<AppModel>(settings, flags)`.
- Implements `cosmic::Application` trait.
- Main window is a standard toplevel window created automatically on startup unless configured otherwise.

### Panel Applet (`cosmic::applet::run`)
- Used for COSMIC panel plugins / status applets (e.g., battery, audio, calendar, translator).
- Entrypoint: `cosmic::applet::run::<AppModel>(flags)`.
- Main window (`core.main_window_id()`) is the panel icon button, embedded directly into `cosmic-panel`.
- Popup is positioned via `core.applet.get_popup_settings(...)` and `get_popup(...)`.

---

## 2. Critical Rendering Pitfalls & Buffer Lifecycle

### Pitfall 1: Ghost Trails (Solitaire Deck Effect) & Transparent Surfaces
- **Symptom:** Moving windows, switching tabs, or redrawing sliders leaves trails of previous frames smeared across the window, or the desktop wallpaper bleeds through the center of the window.
- **Root Cause:** In Wayland, applet processes and popup surfaces default to `transparent: true`. If the root widget returned by `view_window` is a naked `column![...]` or `row![...]`, the areas between widgets have alpha = 0. The Wayland compositor does NOT clear the buffer with an opaque color, causing previous draw calls to accumulate.
- **Rule:** Every toplevel window, dialog, or popup view MUST be enclosed in a root `container` with `Length::Fill` and an opaque background style:
  ```rust
  widget::container(content)
      .width(cosmic::iced::Length::Fill)
      .height(cosmic::iced::Length::Fill)
      .class(cosmic::theme::Container::WindowBackground)
      .into()
  ```

### Pitfall 2: Window ID Routing in `view_window(id: window::Id)`
- **Symptom:** Applet popup content is rendered inside the settings window, or duplicate header bars appear.
- **Root Cause:** The `else` branch of `view_window` falls through to `popup_container(...)` for unrecognized or transient window IDs.
- **Rule:** Always strictly guard every window ID. Unmatched IDs must return an empty element:
  ```rust
  fn view_window(&self, id: window::Id) -> Element<'_, Self::Message> {
      if Some(id) == self.settings_window {
          return self.settings_state.view(&self.config).map(Message::Settings);
      }
      if Some(id) == self.popup {
          return self.core.applet.popup_container(popup_content).into();
      }
      // Never fall through to popup_container for unknown IDs!
      widget::text("").into()
  }
  ```

### Pitfall 3: Popup vs Toplevel Window Competition
- When opening a secondary toplevel settings window from an applet popup, destroy the popup surface immediately (`destroy_popup(p)`). Leaving the popup open while a modal or toplevel window is spawned creates focus and positioner collisions in `cosmic-comp`.

---

## 3. Wayland Layer Shell (`zwlr_layer_shell_v1`) Contracts

`libcosmic` supports Wayland layer surfaces for overlays, panels, and desktop utilities.

### Click-Through Subtitle Overlay (HUD)
To guarantee 100% click-through transparency to underlying full-screen games:
1. `layer`: MUST be `Layer::Overlay`.
2. `keyboard_interactivity`: MUST be `KeyboardInteractivity::None`.
3. `input_zone`: MUST be `Some(vec![])`. An empty input vector tells the Wayland compositor to pass all pointer, touch, and cursor events through to underlying windows.
4. `exclusive_zone`: `-1` (does not disturb window viewports).

```rust
SctkLayerSurfaceSettings {
    id: self.surface_id,
    layer: Layer::Overlay,
    keyboard_interactivity: KeyboardInteractivity::None,
    input_zone: Some(vec![]), // 100% click-through
    exclusive_zone: -1,
    anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
    margin: IcedMargin { bottom: 40, top: 0, left: 60, right: 60 },
    size: Some((None, Some(180))),
    ..Default::default()
}
```

### Interactive Region Selector (ROI Picker)
To capture mouse events for dragging selection boxes:
1. `layer`: `Layer::Overlay`.
2. `keyboard_interactivity`: `KeyboardInteractivity::Exclusive` (intercepts Esc key to cancel).
3. `input_zone`: `None` (captures full mouse events).
4. `anchor`: `Anchor::all()`.

---

## 4. COSMIC Theme & Configuration (v2)

### Theme Architecture
- System theme configs live in `~/.config/cosmic/com.system76.CosmicTheme.Dark/v2/` (and `/usr/share/cosmic/`).
- In Theme v2, `accent`, `button`, `success`, `destructive`, and `warning` are **`Component`** structs, NOT raw color tuples.
- A valid `Component` struct requires the `base` field:
  ```ron
  (
      base: "#63D0DFFF",
      hover: "#63BAC6FF",
      pressed: "#3C737AFF",
      selected: "#63BAC6FF",
      selected_text: "#63D0DFFF",
      focus: "#63D0DFFF",
      divider: "#000000FF",
      on: "#000000FF",
      disabled: "#63D0DFFF",
      on_disabled: "#326870FF",
      border: "#63D0DFFF",
      disabled_border: "#63D0DF80",
  )
  ```
  *Never write raw `(red: ..., green: ..., blue: ...)` into `com.system76.CosmicTheme.Dark/v2/accent`.*

### Persistent Configuration (`cosmic-config`)
Derive `CosmicConfigEntry` with explicit version attribute:
```rust
#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq, Serialize, Deserialize)]
#[version = 1]
pub struct Config {
    pub enabled: bool,
    pub font_size: u16,
}
```

---

## 5. UI Layout & Widget Conventions

1. **Text Wrapping:** `widget::text(&str)` does NOT wrap by default. Always chain `.wrapping(cosmic::iced::widget::text::Wrapping::Word)` on variable-length or localized text.
2. **Settings Items:** Use `widget::settings::section()` and `widget::settings::item(title, control)` for standard COSMIC settings layouts.
3. **Button Semantics:**
   - Suggested action (primary call to action): `button::suggested("Save")`
   - Secondary action: `button::standard("Cancel")`
   - Flat/icon action: `button::custom(icon).class(theme::Button::Text)`
4. **Scrollables:** Always set explicit bounds on scrollable containers (`.height(Length::Fill)` or `Length::Fixed(...)`).
