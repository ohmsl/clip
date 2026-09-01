# Clip

This is a work-in-progress desktop clipping and capture application. It is a fully fledged capture pipeline with a UI-oriented architecture, designed to record short clips of gameplay or desktop activity with synced audio.

The project is currently focused on correctness, architecture, and long-term flexibility rather than polish.

---

## What Clip Is

- A local clipping application
- Built on **GStreamer**
- Designed for **low-latency video + audio capture**
- Modular, graph-based pipeline architecture
- Explicit control over sources, encoders, and muxing
- Intended to support:
    - Desktop / game capture
    - System audio
    - Microphone audio
    - Multiple audio sources mixed together
    - Ring-buffer-based clipping (save the last N seconds)

This is **not** a wrapper around OBS, ShadowPlay, or Media Foundation.
Clip builds and owns its pipeline directly.

---

## Current Status

⚠️ **Work in progress**

Implemented:
- Modular `VideoGraph`
- Modular `AudioGraph`
- Multiple audio sources with optional mixing
- AAC audio encoding
- H.264 video encoding
- MPEG-TS muxing
- Ring buffer for encoded packets
- Explicit pipeline lifecycle management
- Centralised bus/error handling

In progress / planned:
- UI layer
- Hotkey-based clip saving
- Persistent settings
- Per-source configuration
- Robust recovery from device loss
- Cross-platform abstractions (Windows-first for now)

Expect breaking changes. Expect refactors.

---

## Architecture Overview

Clip is structured around **explicit media graphs**, not a giant imperative pipeline function.

At a high level:

- `VideoGraph`
    - Builds the full video pipeline
    - Owns capture source, transforms, encoder, parser, and queue
    - Exposes a single, mux-ready output

- `AudioGraph`
    - Builds one or more audio sources
    - Optionally mixes them
    - Encodes to AAC
    - Exposes a single, mux-ready output

- `GstCapture`
    - Owns the pipeline lifecycle
    - Wires graph outputs into the mux
    - Handles state transitions and shutdown
    - Pushes encoded packets into a ring buffer

The goal is:
- Minimal logic in `GstCapture`
- Maximum clarity inside each graph
- No hidden side effects
- No "magic" linking

If you are looking for a single `start_pipeline()` function that does everything, this project intentionally does not work that way.

---

## Why GStreamer

GStreamer provides:
- Precise control over timing and buffering
- Explicit graph topology
- Hardware encoder access
- Deterministic behaviour when used carefully

Clip embraces that complexity instead of abstracting it away poorly.

---

## Building

Requirements:
- Rust (stable)
- GStreamer (with plugins for:
    - H.264 encoding
    - AAC encoding
    - MPEG-TS muxing
    - WASAPI on Windows)

This project assumes you know how to install GStreamer correctly on your platform. If you're not sure, refer to the [official documentation](https://gstreamer.freedesktop.org/documentation/installing/index.html).

Build:
```sh
cargo build --release --manifest-path apps/ui/Cargo.toml
```

Run:
```sh
cargo run --manifest-path apps/ui/Cargo.toml
```

---

## Non-Goals (For Now)

- Being a drop-in OBS replacement
- Supporting every codec under the sun
- Providing a plugin ecosystem
- Running invisibly in the background

The focus is:
**correct capture, clean architecture, and long-term maintainability.**

---

## Contributing

This is currently an experimental project.
Contributions are welcome, but expect rapid iteration and breaking changes.

If you are reading the code and thinking:
> “This is very explicit.”

Good. That is intentional.
