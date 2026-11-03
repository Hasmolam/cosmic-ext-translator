<div align="center">

# cosmic-ext-translator

**Real-time Wayland Screen and Game Subtitle Translator for COSMIC Desktop**

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![COSMIC Desktop](https://img.shields.io/badge/DE-COSMIC-purple.svg)](https://github.com/pop-os/cosmic-epoch)
[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-180%20Passed-brightgreen.svg)]()

<br/>

<p align="center">
  <img src="docs/assets/settings_capture.png" width="63%" alt="Standalone Configuration & Inspection Dashboard" />
  &nbsp;&nbsp;
  <img src="docs/assets/applet_popup.png" width="29%" alt="COSMIC Panel Applet Popup" />
</p>

*Non-blocking PipeWire capture, pure-Rust neural OCR, contextual machine translation, and 100% click-through Wayland layer-shell HUD subtitles for full-screen games and visual novels.*

</div>

---

## Technical Pipeline & Capabilities

The architecture is designed around zero focus loss, deterministic buffer lifecycles, and low CPU/GPU overhead:

1. **Non-blocking Screen Capture (`ashpd` + PipeWire):**
   - Utilizes XDG Desktop Portal Screencast with `restore_token` caching to remember screen capture permissions across launches.
   - PipeWire event loop executes on a dedicated OS worker thread (`std::thread`), decoupled from Tokio async execution.
   - Buffer negotiation acquires only the user-defined Region of Interest (ROI) and immediately drops the raw Spa buffer (`dequeue_buffer()`), preventing buffer starvation or compositor session drops.
   - **Inner Ignore Masking (`ignore_roi`):** Sub-regions within the active capture area (such as animated character portraits, live busts, or mini-maps) can be masked out prior to debouncing and OCR to eliminate false positive triggers.

2. **Two-Stage Hybrid Trigger (`TextGate` Debouncer):**
   - **64-bit dHash + Levenshtein Similarity:** Distinguishes dialogue typewriter animation (letter-by-letter streaming in RPGs/VNs) and 3D background movement.
   - Frames must remain stable for `settle_duration` (configurable, default 250ms) before triggering OCR inference, preventing rate-limit exhaustion and compute waste.
   - **High-Pass Blur Subtraction Filter:** Optional Gaussian blur subtraction filter attenuates smooth background gradients and illumination shifts while sharpening low-contrast text edges.

3. **Pure-Rust OCR, Vertical CJK Layout, and Furigana Pruning:**
   - **Local Inference (`ocrs` + `rten`):** Runs pure-Rust neural OCR models locally with SIMD acceleration (`opt-level = 3`), eliminating external C++ runtime dependencies.
   - **Vertical CJK Topological Sorter:** Classifies aspect ratios ($H/W > 1.4$), clusters vertical columns via horizontal overlap metrics, and enforces right-to-left (RTL) column ordering with top-to-bottom (TTB) reading sequences for Japanese and Chinese text. Preserves space-aware joins between Western and CJK tokens.
   - **Geometric Furigana Filter:** Identifies and filters out undersized phonetic ruby/furigana annotations adjacent to kanji characters while preserving essential punctuation marks.

4. **Context-Aware Translation and Speaker Disambiguation:**
   - **Speaker Tag Extraction:** Parses speaker names enclosed in 7 standard bracket styles (`【...】`, `〔...〕`, `「...」`, `[...]`, `Name:`, etc.), separating the speaker label from dialogue text.
   - **In-Memory Glossary:** Replaces recognized speaker names and in-game lore terminology based on user-defined glossary pairs.
   - **Sliding Turn History (`TranslationContext`):** Maintains recent dialogue history in memory to provide context for LLM translation backends (resolving omitted pronouns and gender references in Japanese).
   - **Engines Supported:** Google Translate Web API, local Ollama LLMs (`qwen2.5:3b`, etc.), DeepL API, OpenAI-compatible endpoints, and modular offline neural engine traits (`OfflineTranslationBackend`).
   - **HTTP 429 Rate-Limit Manager:** Employs exponential backoff with retry-after parsing when remote APIs throttle requests, surfacing an active countdown pill directly on the HUD.
   - **Morphological Japanese Dictionary:** Built-in longest-prefix tokenizer and lexical lookup for Japanese game dialogue.

5. **Input-Transparent Layer-Shell HUD (`wlr-layer-shell`):**
   - Displays translations and status notifications as an overlay above full-screen windows.
   - Configured with `input_zone: Some(vec![])` to guarantee 100% click-through transparency to underlying game windows.

6. **Dialogue Backlog and Standalone Settings Dashboard:**
   - Retains up to 500 translation entries in a searchable and copyable FIFO history buffer.
   - Launches settings as a standalone COSMIC desktop application (`cosmic::app::run`) with full window decorations and system headerbar, syncing changes live to the panel applet via `cosmic-config` subscription (`watch_config`).

---

## User Interface & Features

<table>
  <tr>
    <td width="50%" valign="top" align="center">
      <b>Live Inspection & Capture Area</b>
      <br/><br/>
      <img src="docs/assets/settings_capture.png" alt="Capture & Area" width="100%" />
      <br/><br/>
      <sub>Interactive ROI definition, ignore masking (busts/mini-maps), Otsu binarization inspection, and settle timing threshold.</sub>
    </td>
    <td width="50%" valign="top" align="center">
      <b>COSMIC Panel Applet</b>
      <br/><br/>
      <img src="docs/assets/applet_popup.png" alt="Panel Applet" width="56%" />
      <br/><br/>
      <sub>System tray popup for instant translation toggling, preset switching (RPG 25%, VN 33%, Full Screen), and one-shot capture.</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top" align="center">
      <b>Translation Engine Configuration</b>
      <br/><br/>
      <img src="docs/assets/settings_engine.png" alt="Translation Engine" width="100%" />
      <br/><br/>
      <sub>Multi-backend routing with support for Google Translate, DeepL API, local Ollama models (e.g. Qwen2.5), and OpenAI endpoints.</sub>
    </td>
    <td width="50%" valign="top" align="center">
      <b>Subtitle HUD & Calibration</b>
      <br/><br/>
      <img src="docs/assets/settings_hud.png" alt="Subtitle HUD" width="100%" />
      <br/><br/>
      <sub>Input-transparent layer-shell overlay positioning, background opacity control, and interactive screen calibration.</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top" align="center">
      <b>In-Game Terminology Glossary</b>
      <br/><br/>
      <img src="docs/assets/settings_glossary.png" alt="Game Glossary" width="100%" />
      <br/><br/>
      <sub>User-defined substitution dictionary for recurring character names, fantasy terminology, and dialogue consistency.</sub>
    </td>
    <td width="50%" valign="top" align="center">
      <b>Searchable Dialogue Backlog</b>
      <br/><br/>
      <img src="docs/assets/settings_history.png" alt="Dialogue History" width="100%" />
      <br/><br/>
      <sub>500-turn in-memory FIFO backlog with real-time text query filtering and single-click clipboard copying.</sub>
    </td>
  </tr>
</table>

---

## Workspace Architecture

The project is structured as a multi-crate Rust workspace under GPL-3.0-or-later:

| Crate | Purpose |
| :--- | :--- |
| `crates/translator-core` | Headless core engine: PipeWire capture thread, ROI and ignore masking, TextGate debouncer, blur subtraction, `ocrs` OCR inference, vertical CJK reading order sorter, furigana pruner, speaker parser, translation context, HTTP 429 manager, Japanese dictionary, and IPC socket server. |
| `crates/cosmic-translator-overlay` | Dual-mode Wayland layer-shell binary: `--hud` (click-through subtitle HUD and countdown pill) and `--selector` (interactive screen region picker). |
| `crates/cosmic-applet-translator` | COSMIC panel applet (`com.system76.CosmicExtTranslator`), popup quick-actions, and 5-tab standalone settings dashboard (Capture, Engine, Display, Glossary, History). |

---

## Global Shortcuts

Registered via the XDG Desktop Portal Global Shortcuts API for in-game control:

| Shortcut | Action |
| :--- | :--- |
| `Super + Alt + S` | Select capture region interactively (`--selector`) |
| `Super + Alt + T` | Toggle continuous real-time translation loop |
| `Super + Alt + H` | Toggle subtitle HUD overlay visibility |
| `Super + Alt + C` | Translate current screen immediately (one-shot trigger) |

---

## System Requirements

The following packages are required for compilation on Ubuntu or Pop!_OS 24.04+:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config clang libclang-dev \
    libpipewire-0.3-dev libspa-0.2-dev libwayland-dev libxkbcommon-dev \
    libdbus-1-dev just
```

* **Desktop Environment:** COSMIC Desktop (`cosmic-comp` and `xdg-desktop-portal-cosmic`)
* **Optional (for local LLM translation):** [Ollama](https://ollama.com/) (`ollama pull qwen2.5:3b`)

---

## Building and Installation

The project uses `just` for workflow automation.

### Build and Test

```bash
# Verify workspace compilation
just check

# Run Clippy lints
just lint

# Execute all workspace unit, integration, and adversarial tests (180 tests)
just test
```

### Installation

```bash
# Build optimized release binaries
just build-release

# Install binaries, .desktop launcher, metainfo, and icon to system directories (/usr/bin and /usr/share)
sudo $(which just) install
```

To remove the installation:

```bash
sudo $(which just) uninstall
```

---

## Development and Direct Execution

```bash
# Run the standalone configuration dashboard and inspection window
cargo run -p cosmic-applet-translator

# Run in COSMIC panel applet mode
just run-applet

# Launch interactive region selector directly
cargo run -p cosmic-translator-overlay -- --selector

# Launch HUD subtitle overlay directly
cargo run -p cosmic-translator-overlay -- --hud

# Test PipeWire capture pipeline (requires active Wayland session)
just test-capture
```

---

## License

This project is licensed under the [GNU General Public License v3.0 or later](LICENSE) (GPL-3.0-or-later).
