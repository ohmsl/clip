# Clip UI

The desktop UI is a native Rust application built with [GPUI](https://github.com/zed-industries/zed)
and [gpui-base](https://longbridge.github.io/gpui-component/base/getting-started). The app owns
the presentation for its base buttons, inputs, sliders, and switches; the higher-level GPUI
Component layer remains only where it supplies existing popup, select, theme, and notification
infrastructure. Capture, settings, logs, global shortcuts, and clip export all run in the same
process as the GPUI window.

## Development

From the repository root:

```sh
cargo run --manifest-path apps/ui/Cargo.toml
```

If GStreamer or the capture backend is unavailable, the UI can still be launched in
offline mode. Capture controls are disabled, while settings and logs remain available:

```sh
cargo run --manifest-path apps/ui/Cargo.toml --no-default-features -- --offline
```

This command disables the backend feature, so it does not build GStreamer or `apps/core`.
The same runtime mode can also be enabled in a normal backend build with `--offline` or
`CLIP_UI_OFFLINE=1`.

The GPUI installation follows the upstream setup: Rust 1.90 or newer, the platform prerequisites
from the installation guide, and the matching `gpui`, `gpui_platform`, `gpui-base`,
`gpui-component`, and `gpui-component-assets` git dependencies declared in `Cargo.toml`.
