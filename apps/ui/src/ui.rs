use crate::backend::{
    AudioCapsSummary, AudioDevice, Backend, BackendEvent, CaptureStatus, HotkeyController,
    LogEvent, LogLevel, ShortcutSettings, StatusSnapshot, UserSettings, VideoDevice,
    VideoEncoderDescriptor,
};
use chrono::{DateTime, FixedOffset};
use gpui::{
    AppContext, Context, Entity, IntoElement, KeyDownEvent, ParentElement, PathPromptOptions,
    Render, SharedString, Subscription, Task, Window, actions, div, prelude::*, px, relative,
};
use gpui_base::{
    Button as BaseButton, Input as BaseInput, Slider as BaseSlider,
    SliderIndicator as BaseSliderIndicator, SliderThumb as BaseSliderThumb,
    SliderTrack as BaseSliderTrack, Switch as BaseSwitch, SwitchThumb as BaseSwitchThumb,
    SwitchTrack as BaseSwitchTrack,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
};
use gpui_component::{
    ActiveTheme, IndexPath, Root, StyledExt, WindowExt,
    button::{Button as ComponentButton, ButtonVariants},
    h_flex,
    menu::DropdownMenu,
    notification::NotificationType,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
    v_flex,
};
use std::time::{Duration, Instant};

actions!(clip_ui, [OpenLogs, OpenSettings, GoHome]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Logs,
    Settings,
}

#[derive(Debug, Clone, Copy)]
struct LayoutMode {
    compact: bool,
    narrow: bool,
    short: bool,
}

impl LayoutMode {
    fn from_window(window: &Window) -> Self {
        let bounds = window.bounds();
        Self {
            compact: bounds.size.width < px(960.),
            narrow: bounds.size.width < px(720.),
            short: bounds.size.height < px(620.),
        }
    }

    fn tight_spacing(self) -> bool {
        self.compact || self.short
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiCaptureStatus {
    Offline,
    Starting,
    Running,
    Stopped,
    Error,
}

#[derive(Clone)]
struct OptionItem {
    id: String,
    label: String,
}

impl SearchableListItem for OptionItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

pub struct AppView {
    backend: Backend,
    events: crossbeam_channel::Receiver<BackendEvent>,
    hotkeys: Option<HotkeyController>,
    page: Page,
    status: StatusSnapshot,
    settings: UserSettings,
    draft: UserSettings,
    video_devices: Vec<VideoDevice>,
    microphones: Vec<AudioDevice>,
    encoders: Vec<VideoEncoderDescriptor>,
    logs: Vec<LogEvent>,
    capture_status: UiCaptureStatus,
    capture_error: Option<String>,
    is_clipping: bool,
    started_at: Option<Instant>,
    settings_pending: bool,
    recording_shortcut: bool,
    needs_sync: bool,
    pending_notifications: Vec<(NotificationType, String)>,
    video_select: Entity<SelectState<Vec<OptionItem>>>,
    mic_select: Entity<SelectState<Vec<OptionItem>>>,
    encoder_select: Entity<SelectState<Vec<OptionItem>>>,
    system_volume: Entity<SliderState>,
    mic_volume: Entity<SliderState>,
    bitrate: Entity<SliderState>,
    framerate: Entity<InputState>,
    clips_dir: Entity<InputState>,
    shortcut: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    _timer: Task<()>,
}

impl AppView {
    pub fn new(
        backend: Backend,
        events: crossbeam_channel::Receiver<BackendEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let status = backend.snapshot();
        let backend_available = backend.is_available();
        let unavailable_reason = backend.unavailable_reason();
        let settings = status.settings.clone();
        let draft = settings.clone();
        let (video_devices, microphones) = backend.devices();
        let encoders = backend.encoders().unwrap_or_default();

        let video_items: Vec<_> = video_devices
            .iter()
            .map(|device| OptionItem {
                id: device.id.clone(),
                label: device_label(device),
            })
            .collect();
        let mic_items = std::iter::once(OptionItem {
            id: "__none__".to_string(),
            label: "None".to_string(),
        })
        .chain(
            microphones
                .iter()
                .filter(|device| device.is_input)
                .map(|device| OptionItem {
                    id: device.id.clone(),
                    label: device.label.clone(),
                }),
        )
        .collect::<Vec<_>>();
        let encoder_items: Vec<_> = encoders
            .iter()
            .map(|encoder| OptionItem {
                id: encoder.id.clone(),
                label: encoder.name.clone(),
            })
            .collect();

        let video_ids = video_devices
            .iter()
            .map(|device| device.id.clone())
            .collect::<Vec<_>>();
        let encoder_ids = encoders
            .iter()
            .map(|encoder| encoder.id.clone())
            .collect::<Vec<_>>();
        let mic_ids = std::iter::once("__none__".to_string())
            .chain(
                microphones
                    .iter()
                    .filter(|device| device.is_input)
                    .map(|device| device.id.clone()),
            )
            .collect::<Vec<_>>();

        let video_select = cx.new(|cx| {
            SelectState::new(
                video_items,
                selected_index(&settings.video_device_id, &video_ids),
                window,
                cx,
            )
        });
        let mic_selected = settings.mic_device_id.as_deref().unwrap_or("__none__");
        let mic_select = cx.new(|cx| {
            SelectState::new(
                mic_items,
                Some(
                    IndexPath::default().row(
                        mic_ids
                            .iter()
                            .position(|id| id == mic_selected)
                            .unwrap_or(0),
                    ),
                ),
                window,
                cx,
            )
        });
        let encoder_select = cx.new(|cx| {
            SelectState::new(
                encoder_items,
                selected_index(&settings.video_encoder_id, &encoder_ids),
                window,
                cx,
            )
        });

        let system_volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(2.)
                .step(0.05)
                .default_value(settings.system_audio_volume)
        });
        let mic_volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(2.)
                .step(0.05)
                .default_value(settings.mic_volume)
        });
        let bitrate = cx.new(|_| {
            SliderState::new()
                .max(20000.)
                .min(1000.)
                .step(1000.)
                .default_value(settings.bitrate_kbps as f32)
        });
        let framerate =
            cx.new(|cx| InputState::new(window, cx).default_value(settings.framerate.to_string()));
        let clips_dir =
            cx.new(|cx| InputState::new(window, cx).default_value(settings.clips_dir.clone()));
        clips_dir.update(cx, |state, cx| state.set_readonly(true, cx));
        let shortcut =
            cx.new(|cx| InputState::new(window, cx).default_value(settings.shortcuts.clip.clone()));
        shortcut.update(cx, |state, cx| state.set_readonly(true, cx));

        let mut pending_notifications = Vec::new();
        let mut hotkeys = if backend_available {
            match HotkeyController::new(backend.clone()) {
                Ok(hotkeys) => Some(hotkeys),
                Err(error) => {
                    pending_notifications.push((
                        NotificationType::Warning,
                        format!("Global shortcuts unavailable: {error}"),
                    ));
                    None
                }
            }
        } else {
            pending_notifications.push((
                NotificationType::Warning,
                "Backend unavailable; running in offline mode".to_string(),
            ));
            None
        };
        if let Some(controller) = hotkeys.as_mut() {
            if let Err(error) = controller.sync(&settings.shortcuts.clip) {
                pending_notifications.push((
                    NotificationType::Warning,
                    format!("Shortcut registration failed: {error}"),
                ));
            }
        }

        let subscriptions = vec![
            {
                let select = video_select.clone();
                cx.subscribe(
                    &select,
                    move |this, _, event: &SelectEvent<Vec<OptionItem>>, cx| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.draft.video_device_id = value.clone();
                            cx.notify();
                        }
                    },
                )
            },
            {
                let select = mic_select.clone();
                cx.subscribe(
                    &select,
                    move |this, _, event: &SelectEvent<Vec<OptionItem>>, cx| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.draft.mic_device_id = (value != "__none__").then(|| value.clone());
                            cx.notify();
                        }
                    },
                )
            },
            {
                let select = encoder_select.clone();
                cx.subscribe(
                    &select,
                    move |this, _, event: &SelectEvent<Vec<OptionItem>>, cx| {
                        if let SelectEvent::Confirm(Some(value)) = event {
                            this.draft.video_encoder_id = value.clone();
                            cx.notify();
                        }
                    },
                )
            },
            {
                let state = system_volume.clone();
                cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        this.draft.system_audio_volume = value.start();
                        cx.notify();
                    }
                })
            },
            {
                let state = mic_volume.clone();
                cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        this.draft.mic_volume = value.start();
                        cx.notify();
                    }
                })
            },
            {
                let state = bitrate.clone();
                cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(value) = event {
                        this.draft.bitrate_kbps = value.start().round() as u32;
                        cx.notify();
                    }
                })
            },
            {
                let state = framerate.clone();
                let read_state = state.clone();
                cx.subscribe(&state, move |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        if let Ok(value) = read_state.read(cx).value().parse::<u32>() {
                            this.draft.framerate = value;
                            cx.notify();
                        }
                    }
                })
            },
        ];

        let weak = cx.entity().downgrade();
        let timer = cx.spawn(async move |_view, cx| {
            loop {
                smol::Timer::after(Duration::from_millis(250)).await;
                if weak.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        let logs = backend.recent_logs();
        Self {
            backend,
            events,
            hotkeys,
            page: Page::Home,
            status,
            settings,
            draft,
            video_devices,
            microphones,
            encoders,
            logs,
            capture_status: if backend_available {
                UiCaptureStatus::Starting
            } else {
                UiCaptureStatus::Offline
            },
            capture_error: unavailable_reason,
            is_clipping: false,
            started_at: None,
            settings_pending: false,
            recording_shortcut: false,
            needs_sync: false,
            pending_notifications,
            video_select,
            mic_select,
            encoder_select,
            system_volume,
            mic_volume,
            bitrate,
            framerate,
            clips_dir,
            shortcut,
            _subscriptions: subscriptions,
            _timer: timer,
        }
    }

    fn poll_backend(&mut self) {
        if let Some(hotkeys) = self.hotkeys.as_ref() {
            hotkeys.poll();
        }

        while let Ok(event) = self.events.try_recv() {
            match event {
                BackendEvent::CaptureStatus { status, message } => {
                    self.capture_status = match status {
                        CaptureStatus::Running => {
                            self.started_at.get_or_insert_with(Instant::now);
                            UiCaptureStatus::Running
                        }
                        CaptureStatus::Stopped => {
                            self.started_at = None;
                            UiCaptureStatus::Stopped
                        }
                        CaptureStatus::Error => UiCaptureStatus::Error,
                    };
                    if message.is_some() {
                        self.capture_error = message;
                    } else if status == CaptureStatus::Running {
                        self.capture_error = None;
                    }
                }
                BackendEvent::SettingsUpdated(settings) => {
                    self.settings = settings.clone();
                    self.draft = settings.clone();
                    self.status.settings = settings.clone();
                    self.settings_pending = false;
                    self.needs_sync = true;
                    if let Some(hotkeys) = self.hotkeys.as_mut() {
                        if let Err(error) = hotkeys.sync(&settings.shortcuts.clip) {
                            self.pending_notifications.push((
                                NotificationType::Warning,
                                format!("Shortcut registration failed: {error}"),
                            ));
                        }
                    }
                    self.pending_notifications
                        .push((NotificationType::Success, "Settings applied".to_string()));
                }
                BackendEvent::ClipFinished {
                    filename,
                    duration_ms,
                } => {
                    self.is_clipping = false;
                    self.pending_notifications.push((
                        NotificationType::Success,
                        format!("Saved {filename} ({:.1}s)", duration_ms as f32 / 1000.),
                    ));
                }
                BackendEvent::OperationError { title, message } => {
                    if title == "Clip" {
                        self.is_clipping = false;
                    }
                    if title == "Settings" {
                        self.settings_pending = false;
                    }
                    if title == "Capture" {
                        self.capture_status = UiCaptureStatus::Error;
                        self.capture_error = Some(message.clone());
                    }
                    self.pending_notifications
                        .push((NotificationType::Error, format!("{title}: {message}")));
                }
                BackendEvent::FolderPicked(folder) => {
                    if let Some(folder) = folder {
                        self.draft.clips_dir = folder;
                        self.needs_sync = true;
                    }
                }
                BackendEvent::Log(event) => {
                    self.logs.push(event);
                    if self.logs.len() > 1000 {
                        let remove = self.logs.len() - 1000;
                        self.logs.drain(0..remove);
                    }
                }
            }
        }

        if !self.backend.is_available() {
            self.capture_status = UiCaptureStatus::Offline;
        }
        self.status = self.backend.snapshot();
        if self.status.buffering && self.capture_status != UiCaptureStatus::Running {
            self.capture_status = UiCaptureStatus::Running;
            self.started_at.get_or_insert_with(Instant::now);
        }
    }

    fn sync_components(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.needs_sync {
            return;
        }

        let video_id = self.draft.video_device_id.clone();
        let mic_id = self
            .draft
            .mic_device_id
            .clone()
            .unwrap_or_else(|| "__none__".to_string());
        let encoder_id = self.draft.video_encoder_id.clone();
        let framerate = self.draft.framerate.to_string();
        let clips_dir = self.draft.clips_dir.clone();
        let shortcut = self.draft.shortcuts.clip.clone();
        let system_volume = self.draft.system_audio_volume;
        let mic_volume = self.draft.mic_volume;
        let bitrate = self.draft.bitrate_kbps as f32;

        self.video_select.update(cx, |state, cx| {
            state.set_selected_value(&video_id, window, cx);
        });
        self.mic_select.update(cx, |state, cx| {
            state.set_selected_value(&mic_id, window, cx);
        });
        self.encoder_select.update(cx, |state, cx| {
            state.set_selected_value(&encoder_id, window, cx);
        });
        self.system_volume.update(cx, |state, cx| {
            state.set_value(system_volume, window, cx);
        });
        self.mic_volume.update(cx, |state, cx| {
            state.set_value(mic_volume, window, cx);
        });
        self.bitrate.update(cx, |state, cx| {
            state.set_value(bitrate, window, cx);
        });
        self.framerate.update(cx, |state, cx| {
            state.set_value(framerate, window, cx);
        });
        self.clips_dir.update(cx, |state, cx| {
            state.set_value(clips_dir, window, cx);
        });
        self.shortcut.update(cx, |state, cx| {
            state.set_value(shortcut, window, cx);
        });
        self.needs_sync = false;
    }

    fn push_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (kind, message) in self.pending_notifications.drain(..) {
            window.push_notification((kind, message), cx);
        }
    }

    fn render_home(&mut self, layout: LayoutMode, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.backend.is_available() && self.status.buffering && !self.is_clipping;
        let backend = self.backend.clone();
        let weak = cx.entity().downgrade();
        let clip_button = app_button("clip-button", "CLIP", ButtonTone::Primary, cx)
            .px_8()
            .py_4()
            .text_lg()
            .disabled(!enabled)
            .on_click(move |_, _, cx| {
                if enabled {
                    backend.request_clip();
                    let _ = weak.update(cx, |this, cx| {
                        this.is_clipping = true;
                        cx.notify();
                    });
                }
            });

        let more = ComponentButton::new("more-menu")
            .ghost()
            .label("More")
            .dropdown_caret(true)
            .dropdown_menu(|menu, _, _| {
                menu.menu("Log", Box::new(OpenLogs))
                    .menu("Settings", Box::new(OpenSettings))
            });

        let status_text = match self.capture_status {
            UiCaptureStatus::Offline => "Offline mode",
            UiCaptureStatus::Running => "Capturing",
            UiCaptureStatus::Starting => "Starting",
            UiCaptureStatus::Stopped => "Stopped",
            UiCaptureStatus::Error => "Capture error",
        };
        let elapsed = self
            .started_at
            .map(|started| format_duration(started.elapsed()))
            .unwrap_or_else(|| "00:00:00".to_string());
        let device = self.video_device_label();
        let encoder = self.encoder_label();
        let error = self.capture_error.clone();

        let status = h_flex()
            .gap_3()
            .items_center()
            .child(div().size(px(10.)).rounded_full().bg(
                if self.capture_status == UiCaptureStatus::Running {
                    cx.theme().green
                } else {
                    cx.theme().muted_foreground
                },
            ))
            .child(
                v_flex()
                    .gap_0p5()
                    .child(div().font_semibold().child(status_text))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(elapsed),
                    ),
            )
            .into_any_element();

        let metrics = h_flex()
            .gap_4()
            .items_center()
            .when(layout.narrow, |this| this.flex_wrap().gap_3())
            .child(metric("VIDEO", device, cx))
            .child(metric("ENCODER", encoder, cx))
            .child(metric("FPS", self.settings.framerate.to_string(), cx))
            .child(more)
            .into_any_element();

        let header = if layout.compact {
            v_flex()
                .w_full()
                .gap_3()
                .child(status)
                .child(metrics)
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(status)
                .child(metrics)
                .into_any_element()
        };

        v_flex()
            .size_full()
            .when(layout.tight_spacing(), |this| this.p_4())
            .when(!layout.tight_spacing(), |this| this.p_8())
            .gap_4()
            .child(header)
            .child(
                div().flex_1().items_center().justify_center().child(
                    v_flex()
                        .items_center()
                        .gap_3()
                        .child(clip_button)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("last {}s", self.status.buffer_seconds)),
                        )
                        .when_some(error, |this, error| {
                            this.child(div().text_sm().text_color(cx.theme().danger).child(error))
                        }),
                ),
            )
            .child(self.render_diagnostics(layout, cx))
    }

    fn render_diagnostics(&self, layout: LayoutMode, cx: &mut Context<Self>) -> impl IntoElement {
        let system = audio_caps_label(self.status.audio_caps.system.as_ref());
        let mic = audio_caps_label(self.status.audio_caps.mic.as_ref());
        let throughput = if self.status.ring_buffer_duration_ms > 0 {
            self.status.ring_buffer_bytes as f32 * 8.
                / self.status.ring_buffer_duration_ms as f32
                / 1000.
        } else {
            0.
        };

        h_flex()
            .w_full()
            .when(layout.narrow, |this| this.flex_col().items_start().gap_2())
            .when(!layout.narrow, |this| {
                this.flex_wrap().justify_between().gap_4()
            })
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .px_4()
            .py_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(diagnostic("SYS", system, cx))
            .child(diagnostic("MIC", mic, cx))
            .child(diagnostic(
                "BUF",
                format!("{}s", self.status.buffer_seconds),
                cx,
            ))
            .child(diagnostic("THR", format!("{throughput:.1} Mbps"), cx))
    }

    fn render_settings(&mut self, layout: LayoutMode, cx: &mut Context<Self>) -> impl IntoElement {
        let backend = self.backend.clone();
        let weak = cx.entity().downgrade();
        let apply = app_button(
            "apply-settings",
            if self.settings_pending {
                "Applying…"
            } else {
                "Apply settings"
            },
            ButtonTone::Primary,
            cx,
        )
        .disabled(self.settings_pending)
        .on_click(move |_, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.settings_pending = true;
                backend.update_settings(this.draft.clone());
                cx.notify();
            });
        });
        let choose_view = cx.entity().downgrade();
        let folder_button = app_button("choose-folder", "Choose folder", ButtonTone::Outline, cx)
            .on_click(move |_, _, cx| {
                let receiver = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Choose clips folder".into()),
                });
                let weak = choose_view.clone();
                cx.spawn(async move |cx| {
                    if let Ok(Ok(Some(paths))) = receiver.await {
                        if let Some(path) = paths.into_iter().next() {
                            let _ = weak.update(cx, |this, cx| {
                                this.draft.clips_dir = path.to_string_lossy().into_owned();
                                this.needs_sync = true;
                                cx.notify();
                            });
                        }
                    }
                })
                .detach();
            });
        let open_backend = self.backend.clone();
        let open_button = app_button("open-folder", "Open", ButtonTone::Ghost, cx)
            .on_click(move |_, _, _| open_backend.open_clips_folder());
        let system_view = cx.entity().downgrade();
        let system_audio_enabled = self.draft.system_audio_enabled;

        let folder_row = if layout.narrow {
            v_flex()
                .gap_2()
                .child(input_field(&self.clips_dir, cx))
                .child(h_flex().gap_2().child(folder_button).child(open_button))
                .into_any_element()
        } else {
            h_flex()
                .gap_2()
                .child(div().flex_1().child(input_field(&self.clips_dir, cx)))
                .child(folder_button)
                .child(open_button)
                .into_any_element()
        };

        let settings_body = v_flex()
            .w_full()
            .max_w(px(760.))
            .gap_4()
            .child(setting_field(
                "Video source",
                Select::new(&self.video_select).w_full(),
                cx,
            ))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(setting_label("System audio", cx))
                    .child(
                        BaseSwitch::new("system-audio")
                            .checked(system_audio_enabled)
                            .accessibility_label("System audio")
                            .on_change(move |checked, _, _, cx| {
                                let _ = system_view.update(cx, |this, cx| {
                                    this.draft.system_audio_enabled = checked;
                                    cx.notify();
                                });
                            })
                            .child(
                                BaseSwitchTrack::new("system-audio-track")
                                    .checked(system_audio_enabled)
                                    .w(px(36.))
                                    .h(px(20.))
                                    .p(px(2.))
                                    .rounded_full()
                                    .bg(if system_audio_enabled {
                                        cx.theme().primary
                                    } else {
                                        cx.theme().muted
                                    })
                                    .child(
                                        BaseSwitchThumb::new(system_audio_enabled)
                                            .size_4()
                                            .rounded_full()
                                            .bg(cx.theme().background)
                                            .ml(if system_audio_enabled {
                                                px(16.)
                                            } else {
                                                px(0.)
                                            }),
                                    ),
                            ),
                    ),
            )
            .child(setting_field(
                "Microphone",
                Select::new(&self.mic_select).w_full(),
                cx,
            ))
            .child(slider_field(
                "System volume",
                self.draft.system_audio_volume,
                slider_field_control(&self.system_volume, cx),
                cx,
            ))
            .child(slider_field(
                "Microphone volume",
                self.draft.mic_volume,
                slider_field_control(&self.mic_volume, cx),
                cx,
            ))
            .child(setting_field(
                "Framerate",
                input_field(&self.framerate, cx),
                cx,
            ))
            .child(setting_field(
                "Video encoder",
                Select::new(&self.encoder_select).w_full(),
                cx,
            ))
            .child(slider_field(
                "Bitrate",
                self.draft.bitrate_kbps as f32,
                slider_field_control(&self.bitrate, cx),
                cx,
            ))
            .child(
                v_flex()
                    .gap_2()
                    .child(setting_label("Clips folder", cx))
                    .child(folder_row),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(setting_label("Clip shortcut", cx))
                    .child(
                        div()
                            .id("shortcut-recorder")
                            .w_full()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.recording_shortcut = true;
                                this.shortcut
                                    .update(cx, |state, cx| state.focus(window, cx));
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(Self::on_shortcut_key_down))
                            .child(input_field(&self.shortcut, cx)),
                    ),
            )
            .into_any_element();

        v_flex()
            .size_full()
            .when(layout.tight_spacing(), |this| this.p_4())
            .when(!layout.tight_spacing(), |this| this.p_8())
            .gap_4()
            .child(self.settings_header(layout, cx))
            .child(
                v_flex()
                    .id("settings-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(settings_body),
            )
            .child(h_flex().w_full().justify_end().child(apply))
            .when(self.recording_shortcut, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Press Escape to cancel"),
                )
            })
    }

    fn settings_header(&self, layout: LayoutMode, cx: &mut Context<Self>) -> impl IntoElement {
        let title = h_flex()
            .gap_3()
            .items_center()
            .child(
                app_button("back-settings", "Back", ButtonTone::Ghost, cx).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.page = Page::Home;
                        cx.notify();
                    },
                )),
            )
            .child(div().text_2xl().font_semibold().child("Settings"))
            .into_any_element();
        let helper = div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(if self.backend.is_available() {
                if layout.narrow {
                    "Active capture"
                } else {
                    "Changes are applied to the active capture"
                }
            } else if layout.narrow {
                "Saved locally"
            } else {
                "Backend offline; changes are saved locally"
            })
            .into_any_element();

        if layout.narrow {
            v_flex()
                .w_full()
                .gap_2()
                .child(title)
                .child(helper)
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .flex_wrap()
                .justify_between()
                .items_center()
                .gap_3()
                .child(title)
                .child(helper)
                .into_any_element()
        }
    }

    fn render_logs(&mut self, layout: LayoutMode, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.logs.iter().rev().map(|event| {
            let time = format_timestamp(&event.timestamp);
            let (level, color) = match event.level {
                LogLevel::Debug => ("DEBUG", cx.theme().muted_foreground),
                LogLevel::Info => ("INFO", cx.theme().blue),
                LogLevel::Warning => ("WARN", cx.theme().yellow),
                LogLevel::Error => ("ERROR", cx.theme().danger),
            };

            if layout.narrow {
                v_flex()
                    .gap_1()
                    .pb_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(div().text_color(cx.theme().muted_foreground).child(time))
                            .child(div().text_color(color).child(level))
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(event.source.clone()),
                            ),
                    )
                    .child(div().child(event.message.clone()))
                    .into_any_element()
            } else {
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .w(px(70.))
                            .text_color(cx.theme().muted_foreground)
                            .child(time),
                    )
                    .child(div().w(px(58.)).text_color(color).child(level))
                    .child(
                        div()
                            .w(px(100.))
                            .text_color(cx.theme().muted_foreground)
                            .child(event.source.clone()),
                    )
                    .child(div().flex_1().child(event.message.clone()))
                    .into_any_element()
            }
        });

        v_flex()
            .size_full()
            .when(layout.tight_spacing(), |this| this.p_4())
            .when(!layout.tight_spacing(), |this| this.p_8())
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                app_button("back-logs", "Back", ButtonTone::Ghost, cx).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.page = Page::Home;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(div().text_2xl().font_semibold().child("Logs")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} recent events", self.logs.len())),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .id("logs-scroll")
                    .overflow_y_scroll()
                    .gap_1()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_xs()
                    .children(rows),
            )
    }

    fn video_device_label(&self) -> String {
        self.video_devices
            .iter()
            .find(|device| device.id == self.settings.video_device_id)
            .map(device_label)
            .unwrap_or_else(|| self.settings.video_device_id.clone())
    }

    fn encoder_label(&self) -> String {
        self.encoders
            .iter()
            .find(|encoder| encoder.id == self.settings.video_encoder_id)
            .map(|encoder| encoder.name.clone())
            .unwrap_or_else(|| self.settings.video_encoder_id.clone())
    }

    fn on_shortcut_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.recording_shortcut {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        if key == "escape" {
            self.recording_shortcut = false;
            cx.notify();
            return;
        }
        if matches!(
            key.as_str(),
            "control" | "alt" | "shift" | "meta" | "super" | "function"
        ) {
            return;
        }

        let modifiers = event.keystroke.modifiers;
        let mut parts = Vec::new();
        if modifiers.control {
            parts.push("Ctrl");
        }
        if modifiers.alt {
            parts.push("Alt");
        }
        if modifiers.shift {
            parts.push("Shift");
        }
        if modifiers.platform {
            parts.push("Super");
        }
        if modifiers.function {
            parts.push("Fn");
        }
        let display_key = display_key(&key);
        parts.push(&display_key);
        let shortcut = parts.join("+");
        self.draft.shortcuts = ShortcutSettings {
            clip: shortcut.clone(),
        };
        self.shortcut
            .update(cx, |state, cx| state.set_value(shortcut, window, cx));
        self.recording_shortcut = false;
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_backend();
        self.sync_components(window, cx);
        self.push_notifications(window, cx);

        let layout = LayoutMode::from_window(window);
        let content = match self.page {
            Page::Home => self.render_home(layout, cx).into_any_element(),
            Page::Logs => self.render_logs(layout, cx).into_any_element(),
            Page::Settings => self.render_settings(layout, cx).into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &OpenLogs, _, cx| {
                this.page = Page::Logs;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.page = Page::Settings;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &GoHome, _, cx| {
                this.page = Page::Home;
                cx.notify();
            }))
            .child(content)
            .children(Root::render_notification_layer(window, cx))
    }
}

fn selected_index(value: &str, values: &[String]) -> Option<IndexPath> {
    values
        .iter()
        .position(|candidate| candidate == value)
        .map(|row| IndexPath::default().row(row))
}

fn device_label(device: &VideoDevice) -> String {
    match (device.width, device.height) {
        (Some(width), Some(height)) => {
            format!("{} ({}×{})", device.label, width, height)
        }
        _ => device.label.clone(),
    }
}

#[derive(Clone, Copy)]
enum ButtonTone {
    Primary,
    Outline,
    Ghost,
}

fn app_button(
    id: &'static str,
    label: impl Into<SharedString>,
    tone: ButtonTone,
    cx: &Context<AppView>,
) -> BaseButton {
    let mut button = BaseButton::new(id)
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .text_sm()
        .styles(|styles| styles.disabled(|style| style.opacity(0.45)));

    button = match tone {
        ButtonTone::Primary => button
            .bg(cx.theme().button_primary)
            .text_color(cx.theme().button_primary_foreground)
            .hover(|style| style.bg(cx.theme().button_primary_hover)),
        ButtonTone::Outline => button
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .hover(|style| style.bg(cx.theme().accent)),
        ButtonTone::Ghost => button
            .text_color(cx.theme().muted_foreground)
            .hover(|style| style.bg(cx.theme().accent)),
    };

    button.child(label.into())
}

fn input_field(state: &Entity<InputState>, cx: &Context<AppView>) -> impl IntoElement {
    div()
        .h(px(36.))
        .w_full()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().input_background())
        .text_sm()
        .child(BaseInput::new(state))
}

fn slider_field_control(state: &Entity<SliderState>, cx: &Context<AppView>) -> impl IntoElement {
    let percentage = state.read(cx).percentage().end;
    let thumb_size = 14.;
    let track_color = cx.theme().border;
    let indicator_color = cx.theme().primary;
    let thumb_color = cx.theme().background;

    BaseSlider::new(state).w_full().h(px(28.)).child(
        BaseSliderTrack::new(state)
            .relative()
            .w_full()
            .h_full()
            .child(
                div()
                    .absolute()
                    .top(px(13.))
                    .left_0()
                    .w_full()
                    .h(px(2.))
                    .bg(track_color),
            )
            .child(
                BaseSliderIndicator::new(state)
                    .absolute()
                    .top(px(13.))
                    .left_0()
                    .w_full()
                    .h(px(2.))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .right(relative(1. - percentage))
                            .bg(indicator_color),
                    ),
            )
            .child(
                BaseSliderThumb::new(state)
                    .absolute()
                    .top(px(7.))
                    .left(relative(percentage))
                    .ml(px(-thumb_size / 2.))
                    .size(px(thumb_size))
                    .rounded_full()
                    .bg(thumb_color)
                    .border_1()
                    .border_color(indicator_color),
            ),
    )
}

fn metric(label: &str, value: String, cx: &Context<AppView>) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_sm().child(value))
}

fn diagnostic(label: &str, value: String, cx: &Context<AppView>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .child(
            div()
                .font_semibold()
                .text_color(cx.theme().foreground)
                .child(label.to_string()),
        )
        .child(value)
}

fn setting_label(label: &str, _cx: &Context<AppView>) -> impl IntoElement {
    div().text_sm().font_semibold().child(label.to_string())
}

fn setting_field(
    label: &str,
    control: impl IntoElement,
    cx: &Context<AppView>,
) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(setting_label(label, cx))
        .child(control)
}

fn slider_field(
    label: &str,
    value: f32,
    control: impl IntoElement,
    cx: &Context<AppView>,
) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .justify_between()
                .child(setting_label(label, cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{value:.2}")),
                ),
        )
        .child(control)
}

fn audio_caps_label(caps: Option<&AudioCapsSummary>) -> String {
    let Some(caps) = caps else {
        return "unavailable".to_string();
    };
    match (caps.rate, caps.channels) {
        (Some(rate), Some(channels)) => format!("{rate} Hz / {channels} ch"),
        _ => caps.raw.clone().unwrap_or_else(|| "connected".to_string()),
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

fn format_timestamp(timestamp: &str) -> String {
    DateTime::<FixedOffset>::parse_from_rfc3339(timestamp)
        .map(|date| date.format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| timestamp.to_string())
}

fn display_key(key: &str) -> String {
    match key {
        " " | "space" => "Space".to_string(),
        "enter" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "Backspace".to_string(),
        "delete" => "Delete".to_string(),
        "left" | "arrowleft" => "Left".to_string(),
        "right" | "arrowright" => "Right".to_string(),
        "up" | "arrowup" => "Up".to_string(),
        "down" | "arrowdown" => "Down".to_string(),
        other if other.starts_with('f') && other[1..].parse::<u8>().is_ok() => other.to_uppercase(),
        other => other
            .chars()
            .next()
            .map(|character| character.to_uppercase().collect())
            .unwrap_or_default(),
    }
}
