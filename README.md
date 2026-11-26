# hyperpad

A macOS Launchpad-style application launcher written in Rust for **Hyprland** (Wayland), built with `gtk4` and `gtk4-layer-shell`.

## Features

- **Fullscreen Layer-Shell Overlay**: Covers the entire screen (including status bars) with exclusive keyboard focus and frosted-glass blur.
- **macOS Launchpad Grid**: Configurable app grid (default: 7×4) with smooth horizontal page transitions (`SlideLeftRight`), clickable page indicator dots, and mouse wheel / trackpad swipe support.
- **App Folders**:
  - **Drag & Drop**: Drag an app onto another app to create a new folder, or drag an app onto an existing folder to move it in.
  - **Context Menu (Right-Click)**: Create folders, move or remove apps, rename or dissolve folders.
  - **2×2 Mini-Icon Preview** on folder tiles and a centered folder overlay with an inline editable title.
- **Frecency Sorting**: Remembers how often and how recently apps were launched—frequently used apps appear earlier both in the overview and in search results.
- **Fuzzy Search**: Start typing anywhere to instantly filter all installed applications (including apps inside folders).
- **Live Settings & Config File**: Gear icon in the top-right corner for live adjustments (background opacity, icon size, columns, rows, spacing), persisted to `~/.config/hyperpad/config.conf`.
- **Automatic System Language**: UI automatically adapts to your system locale (English & German supported).
- **Single-Instance Toggle**: Running `hyperpad` again while it is already open closes it cleanly—ideal for Hyprland keybinds.

## Dependencies

- **Rust** (`rustc` / `cargo`)
- **GTK4** (`gtk4`)
- **gtk4-layer-shell** (`gtk4-layer-shell`)

Arch Linux:
```bash
sudo pacman -S gtk4 gtk4-layer-shell rust
```

## Build & Install

```bash
git clone https://github.com/Papatin13/hyprpad.git
cd hyperpad
cargo build --release
mkdir -p ~/.local/bin
cp target/release/hyperpad ~/.local/bin/
```

## Hyprland Configuration

### Standard `hyprland.conf`

```ini
# Keybind to toggle Hyperpad (e.g. Super + W)
bind = $mainMod, W, exec, ~/.local/bin/hyperpad

# Frosted-glass blur for the "hyperpad" layer surface
layerrule = blur, hyperpad
layerrule = ignorezero, hyperpad
```

> **Note on `blur:xray`:** If your Hyprland config enables `blur:xray = true` globally, also add `xray = false` (or `unset`) for the `hyperpad` namespace so open windows behind the launcher are blurred as well.

### Lua-based Hyprland Config (`hyprland.lua`)

```lua
-- Keybind
hl.bind("SUPER + W", hl.dsp.exec_cmd("~/.local/bin/hyperpad"), { description = "App Launcher: Hyperpad" })

-- Blur rules
hl.layer_rule({ match = { namespace = "hyperpad" }, blur = true })
hl.layer_rule({ match = { namespace = "hyperpad" }, ignore_alpha = 0 })
hl.layer_rule({ match = { namespace = "hyperpad" }, xray = false })
```

## Configuration (`~/.config/hyperpad/config.conf`)

Created automatically on first launch (and editable via the gear icon in the top-right corner):

```ini
columns = 7
rows = 4
icon_size = 72
spacing = 16
background_opacity = 0.25
```

## Controls

- **Type anywhere**: Immediately starts fuzzy search
- **Arrow keys / `Page_Up` / `Page_Down`**: Navigate apps and pages
- **Mouse wheel / Trackpad swipe**: Switch pages
- **`Enter`**: Launch selected app or open selected folder
- **`Esc` or click empty background**: Close open folder or close Hyperpad
