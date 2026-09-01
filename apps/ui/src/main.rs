mod backend;
mod ui;

use anyhow::Result;
use gpui::{App, AppContext, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeMode};
use gpui_component_assets::Assets;

fn main() -> Result<()> {
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        // The higher-level initializer installs gpui-base's focus, input, and
        // popup infrastructure before the window is created.
        gpui_component::init(cx);

        let connection = if offline_requested() {
            backend::Backend::offline("offline mode requested")
        } else {
            match backend::Backend::initialize() {
                Ok(connection) => connection,
                Err(error) => {
                    eprintln!("failed to initialize clip backend: {error}");
                    backend::Backend::offline(error)
                }
            }
        };
        let backend = connection.backend;
        let events = connection.events;
        Theme::change(ThemeMode::Dark, None, cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1180.), px(760.)), cx)),
            window_min_size: Some(size(px(520.), px(420.))),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| ui::AppView::new(backend, events, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open Clip window");
        })
        .detach();
    });

    Ok(())
}

fn offline_requested() -> bool {
    std::env::args().any(|argument| argument == "--offline")
        || matches!(
            std::env::var("CLIP_UI_OFFLINE").ok().as_deref(),
            Some("1" | "true" | "yes" | "on")
        )
}
