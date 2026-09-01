#[cfg(feature = "backend")]
mod real {
    use chrono::Local;
    use clip_service::{
        audio::AudioSourceId,
        capture_devices::{list_microphone_devices, list_video_devices},
        encoders::list_video_encoders,
        gst_capture::GstCapture,
        logger,
        ring_buffer::RingBuffer,
        settings::{
            apply_startup_fallbacks, default_settings, load_settings, save_settings,
            validate_settings,
        },
    };
    pub use clip_service::{
        capture_devices::{AudioDevice, VideoDevice},
        encoders::VideoEncoderDescriptor,
        gst_capture::{AudioCapsState, AudioCapsSummary},
        logger::{LogEvent, LogLevel},
        settings::{ShortcutSettings, UserSettings},
    };
    use crossbeam_channel::{Receiver, Sender};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
        thread,
    };

    pub type RuntimeHandle = Arc<Mutex<CaptureRuntime>>;

    pub struct CaptureRuntime {
        pub settings: UserSettings,
        pub capture: Option<GstCapture>,
        pub ring_buffer: Arc<Mutex<RingBuffer>>,
    }

    #[derive(Clone)]
    pub struct Backend {
        runtime: RuntimeHandle,
        events: Sender<BackendEvent>,
        catalog: Arc<DeviceCatalog>,
        operation_lock: Arc<Mutex<()>>,
        available: bool,
        unavailable_reason: Option<String>,
    }

    #[derive(Clone)]
    struct DeviceCatalog {
        video_devices: Vec<VideoDevice>,
        microphones: Vec<AudioDevice>,
        encoders: Vec<VideoEncoderDescriptor>,
    }

    pub struct BackendConnection {
        pub backend: Backend,
        pub events: Receiver<BackendEvent>,
    }

    #[derive(Debug, Clone)]
    pub enum BackendEvent {
        CaptureStatus {
            status: CaptureStatus,
            message: Option<String>,
        },
        SettingsUpdated(UserSettings),
        ClipFinished {
            filename: String,
            duration_ms: u64,
        },
        OperationError {
            title: String,
            message: String,
        },
        FolderPicked(Option<String>),
        Log(LogEvent),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CaptureStatus {
        Running,
        Stopped,
        Error,
    }

    #[derive(Clone)]
    pub struct StatusSnapshot {
        pub settings: UserSettings,
        pub buffering: bool,
        pub buffer_seconds: u32,
        pub ring_buffer_packets: usize,
        pub ring_buffer_bytes: u64,
        pub ring_buffer_duration_ms: u64,
        pub audio_caps: AudioCapsState,
    }

    pub struct HotkeyController {
        manager: GlobalHotKeyManager,
        active: Option<HotKey>,
        backend: Backend,
    }

    impl Backend {
        pub fn initialize() -> Result<BackendConnection, String> {
            logger::init_logging();
            let (settings, catalog) = resolve_settings()?;
            let runtime = Arc::new(Mutex::new(CaptureRuntime {
                settings,
                capture: None,
                ring_buffer: Arc::new(Mutex::new(RingBuffer::new(30_000))),
            }));
            let (events, receiver) = crossbeam_channel::unbounded();
            spawn_log_forwarder(events.clone());

            let backend = Self {
                runtime,
                events,
                catalog: Arc::new(catalog),
                operation_lock: Arc::new(Mutex::new(())),
                available: true,
                unavailable_reason: None,
            };
            backend.start_capture();

            Ok(BackendConnection {
                backend,
                events: receiver,
            })
        }

        pub fn offline(reason: impl Into<String>) -> BackendConnection {
            logger::init_logging();

            let reason = reason.into();
            let settings = offline_settings();
            let runtime = Arc::new(Mutex::new(CaptureRuntime {
                settings,
                capture: None,
                ring_buffer: Arc::new(Mutex::new(RingBuffer::new(30_000))),
            }));
            let (events, receiver) = crossbeam_channel::unbounded();
            spawn_log_forwarder(events.clone());

            logger::warn("backend", format!("backend unavailable: {reason}"));
            let backend = Self {
                runtime,
                events,
                catalog: Arc::new(DeviceCatalog {
                    video_devices: Vec::new(),
                    microphones: Vec::new(),
                    encoders: Vec::new(),
                }),
                operation_lock: Arc::new(Mutex::new(())),
                available: false,
                unavailable_reason: Some(reason),
            };

            BackendConnection {
                backend,
                events: receiver,
            }
        }

        pub fn is_available(&self) -> bool {
            self.available
        }

        pub fn unavailable_reason(&self) -> Option<String> {
            self.unavailable_reason.clone()
        }

        pub fn settings(&self) -> UserSettings {
            self.runtime.lock().unwrap().settings.clone()
        }

        pub fn snapshot(&self) -> StatusSnapshot {
            let guard = self.runtime.lock().unwrap();
            let ring_buffer = guard.ring_buffer.lock().unwrap();
            let audio_caps = guard
                .capture
                .as_ref()
                .map(GstCapture::audio_caps)
                .unwrap_or_default();

            StatusSnapshot {
                settings: guard.settings.clone(),
                buffering: guard.capture.is_some(),
                buffer_seconds: (ring_buffer.duration_ms() / 1000) as u32,
                ring_buffer_packets: ring_buffer.len(),
                ring_buffer_bytes: ring_buffer.total_bytes(),
                ring_buffer_duration_ms: ring_buffer.duration_ms(),
                audio_caps,
            }
        }

        pub fn devices(&self) -> (Vec<VideoDevice>, Vec<AudioDevice>) {
            if !self.available {
                return (Vec::new(), Vec::new());
            }

            (
                self.catalog.video_devices.clone(),
                self.catalog.microphones.clone(),
            )
        }

        pub fn encoders(&self) -> Result<Vec<VideoEncoderDescriptor>, String> {
            if !self.available {
                return Ok(Vec::new());
            }

            Ok(self.catalog.encoders.clone())
        }

        pub fn recent_logs(&self) -> Vec<LogEvent> {
            logger::recent_logs()
        }

        pub fn start_capture(&self) {
            if !self.available {
                send_error(&self.events, "Backend", self.unavailable_message());
                return;
            }

            let runtime = self.runtime.clone();
            let events = self.events.clone();
            let operation_lock = self.operation_lock.clone();
            spawn(move || {
                let _operation = operation_lock.lock().unwrap();
                let (settings, ring_buffer, already_running) = {
                    let guard = runtime.lock().unwrap();
                    (
                        guard.settings.clone(),
                        guard.ring_buffer.clone(),
                        guard.capture.is_some(),
                    )
                };

                if already_running {
                    send_error(&events, "Capture", "capture is already running");
                    return;
                }

                ring_buffer.lock().unwrap().clear();
                match GstCapture::start(&settings, ring_buffer) {
                    Ok(capture) => {
                        runtime.lock().unwrap().capture = Some(capture);
                        logger::info("capture", "capture started");
                        send_status(&events, CaptureStatus::Running, None);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        logger::error("capture", format!("start failed: {message}"));
                        send_status(&events, CaptureStatus::Error, Some(message));
                    }
                }
            });
        }

        pub fn stop_capture(&self) {
            if !self.available {
                send_error(&self.events, "Backend", self.unavailable_message());
                return;
            }

            let runtime = self.runtime.clone();
            let events = self.events.clone();
            let operation_lock = self.operation_lock.clone();
            spawn(move || {
                let _operation = operation_lock.lock().unwrap();
                let old_capture = runtime.lock().unwrap().capture.take();
                if old_capture.is_some() {
                    logger::info("capture", "stopping pipeline");
                }
                drop(old_capture);
                send_status(&events, CaptureStatus::Stopped, None);
            });
        }

        pub fn restart_capture(&self) {
            if !self.available {
                send_error(&self.events, "Backend", self.unavailable_message());
                return;
            }

            let runtime = self.runtime.clone();
            let events = self.events.clone();
            let operation_lock = self.operation_lock.clone();
            spawn(move || {
                let _operation = operation_lock.lock().unwrap();
                restart_capture_inner(&runtime, &events, true);
            });
        }

        pub fn update_settings(&self, new_settings: UserSettings) {
            let runtime = self.runtime.clone();
            let events = self.events.clone();
            let catalog = self.catalog.clone();
            let operation_lock = self.operation_lock.clone();

            if !self.available {
                spawn(move || {
                    if let Err(error) = save_settings(&new_settings) {
                        let message = error.to_string();
                        logger::error("settings", format!("failed to save: {message}"));
                        send_error(&events, "Settings", message);
                        return;
                    }

                    runtime.lock().unwrap().settings = new_settings.clone();
                    logger::info("settings", "saved settings while backend is offline");
                    send_event(&events, BackendEvent::SettingsUpdated(new_settings));
                });
                return;
            }

            spawn(move || {
                let _operation = operation_lock.lock().unwrap();
                update_settings_inner(&runtime, &events, catalog.as_ref(), new_settings);
            });
        }

        pub fn request_clip(&self) {
            if !self.available {
                send_error(&self.events, "Backend", self.unavailable_message());
                return;
            }

            let runtime = self.runtime.clone();
            let events = self.events.clone();
            let operation_lock = self.operation_lock.clone();
            spawn(move || {
                let _operation = operation_lock.lock().unwrap();
                match clip_sync(&runtime) {
                    Ok((filename, duration_ms)) => {
                        send_event(
                            &events,
                            BackendEvent::ClipFinished {
                                filename,
                                duration_ms,
                            },
                        );
                    }
                    Err(error) => send_error(&events, "Clip", error),
                }
            });
        }

        pub fn open_clips_folder(&self) {
            let path = PathBuf::from(self.settings().clips_dir);
            spawn(move || {
                if let Err(error) = open::that(&path) {
                    logger::error("clips", format!("failed to open folder: {error}"));
                }
            });
        }

        fn unavailable_message(&self) -> String {
            self.unavailable_reason
                .clone()
                .unwrap_or_else(|| "capture backend is unavailable".to_string())
        }
    }

    impl HotkeyController {
        pub fn new(backend: Backend) -> Result<Self, String> {
            let manager = GlobalHotKeyManager::new().map_err(|error| error.to_string())?;
            Ok(Self {
                manager,
                active: None,
                backend,
            })
        }

        pub fn poll(&self) {
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                let Some(active) = self.active else {
                    continue;
                };
                if event.state() != HotKeyState::Pressed || event.id() != active.id() {
                    continue;
                }

                logger::info("shortcut", "clip shortcut triggered");
                self.backend.request_clip();
            }
        }

        pub fn sync(&mut self, accelerator: &str) -> Result<(), String> {
            let hotkey: HotKey = accelerator
                .parse::<HotKey>()
                .map_err(|error| error.to_string())?;

            if self.active == Some(hotkey) {
                return Ok(());
            }

            self.manager
                .register(hotkey)
                .map_err(|error| format!("failed to register {accelerator}: {error}"))?;

            if let Some(active) = self.active {
                if let Err(error) = self.manager.unregister(active) {
                    let _ = self.manager.unregister(hotkey);
                    return Err(error.to_string());
                }
            }

            self.active = Some(hotkey);
            logger::info("shortcut", format!("registered {accelerator}"));
            Ok(())
        }
    }

    fn resolve_settings() -> Result<(UserSettings, DeviceCatalog), String> {
        let video_devices = list_video_devices();
        let microphones = list_microphone_devices();
        let encoders = list_video_encoders().map_err(|error| error.to_string())?;
        let catalog = DeviceCatalog {
            video_devices: video_devices.clone(),
            microphones: microphones.clone(),
            encoders: encoders.clone(),
        };

        let loaded = load_settings().map_err(|error| error.to_string())?;
        let mut settings = match loaded.as_ref() {
            Some(settings) => {
                logger::info("settings", "loaded from disk");
                settings.clone()
            }
            None => {
                let defaults = default_settings(&video_devices, &encoders)
                    .map_err(|error| error.to_string())?;
                logger::info("settings", "created defaults");
                defaults
            }
        };

        let (validated, changes) =
            apply_startup_fallbacks(settings.clone(), &video_devices, &microphones, &encoders);
        if !changes.is_empty() {
            for change in changes {
                logger::info("settings", change);
            }
            settings = validated;
            save_settings(&settings).map_err(|error| error.to_string())?;
        } else if loaded.is_none() {
            save_settings(&settings).map_err(|error| error.to_string())?;
        }

        Ok((settings, catalog))
    }

    fn offline_settings() -> UserSettings {
        load_settings()
            .ok()
            .flatten()
            .unwrap_or_else(|| UserSettings {
                video_device_id: "screen:0".to_string(),
                system_audio_enabled: true,
                system_audio_volume: 1.0,
                mic_device_id: None,
                mic_volume: 1.0,
                video_encoder_id: "offline".to_string(),
                framerate: 60,
                bitrate_kbps: 20_000,
                clips_dir: "clips".to_string(),
                shortcuts: ShortcutSettings {
                    clip: "Ctrl+F10".to_string(),
                },
            })
    }

    fn update_settings_inner(
        runtime: &RuntimeHandle,
        events: &Sender<BackendEvent>,
        catalog: &DeviceCatalog,
        new_settings: UserSettings,
    ) {
        if let Err(message) = validate_settings(
            &new_settings,
            &catalog.video_devices,
            &catalog.microphones,
            &catalog.encoders,
        ) {
            send_error(events, "Settings", message);
            return;
        }

        if let Err(error) = new_settings.shortcuts.clip.parse::<HotKey>() {
            send_error(
                events,
                "Settings",
                format!("invalid clip shortcut: {error}"),
            );
            return;
        }

        // Persist first. Runtime state is only changed after this succeeds, so a
        // failed write cannot leave the UI and the on-disk configuration split.
        if let Err(error) = save_settings(&new_settings) {
            let message = error.to_string();
            logger::error("settings", format!("failed to save: {message}"));
            send_error(events, "Settings", message);
            return;
        }

        let (old_capture, restart, was_running) = {
            let mut guard = runtime.lock().unwrap();
            let restart = should_restart_capture(&guard.settings, &new_settings);
            let was_running = guard.capture.is_some();
            let old_capture = if restart { guard.capture.take() } else { None };

            if !restart {
                if guard.settings.system_audio_volume != new_settings.system_audio_volume {
                    if let Some(capture) = guard.capture.as_ref() {
                        if !capture
                            .set_volume(AudioSourceId::System, new_settings.system_audio_volume)
                        {
                            logger::warn("settings", "system audio volume is not active");
                        }
                    }
                }
                if guard.settings.mic_volume != new_settings.mic_volume {
                    if let Some(capture) = guard.capture.as_ref() {
                        if !capture.set_volume(AudioSourceId::Mic, new_settings.mic_volume) {
                            logger::warn("settings", "microphone volume is not active");
                        }
                    }
                }
            }

            guard.settings = new_settings.clone();
            (old_capture, restart, was_running)
        };

        drop(old_capture);
        if restart && was_running {
            if let Err(error) = start_capture_inner(runtime) {
                let message = error.to_string();
                logger::error("settings", format!("restart failed: {message}"));
                send_status(events, CaptureStatus::Error, Some(message));
                send_error(
                    events,
                    "Settings",
                    "settings saved, but capture could not restart",
                );
                return;
            }
        }

        logger::info("settings", "updated settings");
        send_event(events, BackendEvent::SettingsUpdated(new_settings));
        if runtime.lock().unwrap().capture.is_some() {
            send_status(events, CaptureStatus::Running, None);
        }
    }

    fn should_restart_capture(old: &UserSettings, new: &UserSettings) -> bool {
        old.video_device_id != new.video_device_id
            || old.system_audio_enabled != new.system_audio_enabled
            || old.mic_device_id != new.mic_device_id
            || old.video_encoder_id != new.video_encoder_id
            || old.framerate != new.framerate
            || old.bitrate_kbps != new.bitrate_kbps
    }

    fn restart_capture_inner(
        runtime: &RuntimeHandle,
        events: &Sender<BackendEvent>,
        start_if_stopped: bool,
    ) {
        let (old_capture, was_running) = {
            let mut guard = runtime.lock().unwrap();
            let was_running = guard.capture.is_some();
            (guard.capture.take(), was_running)
        };
        drop(old_capture);

        if was_running || start_if_stopped {
            if let Err(error) = start_capture_inner(runtime) {
                let message = error.to_string();
                logger::error("capture", format!("restart failed: {message}"));
                send_status(events, CaptureStatus::Error, Some(message));
                return;
            }
            send_status(events, CaptureStatus::Running, None);
        } else {
            send_status(events, CaptureStatus::Stopped, None);
        }
    }

    fn start_capture_inner(runtime: &RuntimeHandle) -> std::io::Result<()> {
        let (settings, ring_buffer) = {
            let guard = runtime.lock().unwrap();
            (guard.settings.clone(), guard.ring_buffer.clone())
        };
        ring_buffer.lock().unwrap().clear();
        let capture = GstCapture::start(&settings, ring_buffer)?;
        runtime.lock().unwrap().capture = Some(capture);
        Ok(())
    }

    fn clip_sync(runtime: &RuntimeHandle) -> Result<(String, u64), String> {
        let (packets, clips_dir) = {
            let guard = runtime.lock().unwrap();
            let mut ring_buffer = guard.ring_buffer.lock().unwrap();
            (
                ring_buffer.drain_from_keyframe(),
                guard.settings.clips_dir.clone(),
            )
        };

        if packets.is_empty() {
            return Err("no packets available".to_string());
        }

        let filename = format!("clip-{}.mp4", Local::now().format("%Y-%m-%d_%H-%M-%S"));
        let mut path = PathBuf::from(clips_dir);
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        path.push(&filename);

        let result = clip_service::remux::remux_ts_to_mp4(&packets, &path)?;
        logger::info("capture", format!("clip saved to {}", path.display()));
        Ok((filename, result.duration_ms))
    }

    fn spawn_log_forwarder(events: Sender<BackendEvent>) {
        let mut receiver = logger::subscribe();
        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("failed to start log forwarder: {error}");
                    return;
                }
            };

            runtime.block_on(async move {
                while let Ok(event) = receiver.recv().await {
                    if events.send(BackendEvent::Log(event)).is_err() {
                        break;
                    }
                }
            });
        });
    }

    fn spawn(operation: impl FnOnce() + Send + 'static) {
        let _ = thread::Builder::new()
            .name("clip-ui-operation".to_string())
            .spawn(operation);
    }

    fn send_status(events: &Sender<BackendEvent>, status: CaptureStatus, message: Option<String>) {
        send_event(events, BackendEvent::CaptureStatus { status, message });
    }

    fn send_error(
        events: &Sender<BackendEvent>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        send_event(
            events,
            BackendEvent::OperationError {
                title: title.into(),
                message: message.into(),
            },
        );
    }

    fn send_event(events: &Sender<BackendEvent>, event: BackendEvent) {
        let _ = events.send(event);
    }
}

#[cfg(feature = "backend")]
pub use real::*;

#[cfg(not(feature = "backend"))]
mod offline {
    use chrono::Local;
    use crossbeam_channel::{Receiver, Sender};
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    pub struct AudioCapsSummary {
        pub rate: Option<u32>,
        pub channels: Option<u32>,
        pub raw: Option<String>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct AudioCapsState {
        pub system: Option<AudioCapsSummary>,
        pub mic: Option<AudioCapsSummary>,
    }

    #[derive(Debug, Clone)]
    pub struct VideoDevice {
        pub id: String,
        pub label: String,
        pub width: Option<u32>,
        pub height: Option<u32>,
    }

    #[derive(Debug, Clone)]
    pub struct AudioDevice {
        pub id: String,
        pub label: String,
        pub is_input: bool,
    }

    #[derive(Debug, Clone)]
    pub struct VideoEncoderDescriptor {
        pub id: String,
        pub name: String,
    }

    #[derive(Debug, Clone)]
    pub struct ShortcutSettings {
        pub clip: String,
    }

    #[derive(Debug, Clone)]
    pub struct UserSettings {
        pub video_device_id: String,
        pub system_audio_enabled: bool,
        pub system_audio_volume: f32,
        pub mic_device_id: Option<String>,
        pub mic_volume: f32,
        pub video_encoder_id: String,
        pub framerate: u32,
        pub bitrate_kbps: u32,
        pub clips_dir: String,
        pub shortcuts: ShortcutSettings,
    }

    #[derive(Debug, Clone)]
    pub enum LogLevel {
        Debug,
        Info,
        Warning,
        Error,
    }

    #[derive(Debug, Clone)]
    pub struct LogEvent {
        pub timestamp: String,
        pub level: LogLevel,
        pub source: String,
        pub message: String,
    }

    #[derive(Clone)]
    pub struct Backend {
        settings: Arc<Mutex<UserSettings>>,
        events: Sender<BackendEvent>,
        unavailable_reason: String,
    }

    pub struct BackendConnection {
        pub backend: Backend,
        pub events: Receiver<BackendEvent>,
    }

    #[derive(Debug, Clone)]
    pub enum BackendEvent {
        CaptureStatus {
            status: CaptureStatus,
            message: Option<String>,
        },
        SettingsUpdated(UserSettings),
        ClipFinished {
            filename: String,
            duration_ms: u64,
        },
        OperationError {
            title: String,
            message: String,
        },
        FolderPicked(Option<String>),
        Log(LogEvent),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CaptureStatus {
        Running,
        Stopped,
        Error,
    }

    #[derive(Clone)]
    pub struct StatusSnapshot {
        pub settings: UserSettings,
        pub buffering: bool,
        pub buffer_seconds: u32,
        pub ring_buffer_packets: usize,
        pub ring_buffer_bytes: u64,
        pub ring_buffer_duration_ms: u64,
        pub audio_caps: AudioCapsState,
    }

    pub struct HotkeyController;

    impl Backend {
        pub fn initialize() -> Result<BackendConnection, String> {
            Err("clip backend is disabled at compile time".to_string())
        }

        pub fn offline(reason: impl Into<String>) -> BackendConnection {
            let reason = reason.into();
            let (events, receiver) = crossbeam_channel::unbounded();
            let backend = Self {
                settings: Arc::new(Mutex::new(offline_settings())),
                events,
                unavailable_reason: reason,
            };
            let _ = backend
                .events
                .send(BackendEvent::Log(backend.offline_log()));

            BackendConnection {
                backend,
                events: receiver,
            }
        }

        pub fn is_available(&self) -> bool {
            false
        }

        pub fn unavailable_reason(&self) -> Option<String> {
            Some(self.unavailable_reason.clone())
        }

        pub fn settings(&self) -> UserSettings {
            self.settings.lock().unwrap().clone()
        }

        pub fn snapshot(&self) -> StatusSnapshot {
            StatusSnapshot {
                settings: self.settings(),
                buffering: false,
                buffer_seconds: 0,
                ring_buffer_packets: 0,
                ring_buffer_bytes: 0,
                ring_buffer_duration_ms: 0,
                audio_caps: AudioCapsState::default(),
            }
        }

        pub fn devices(&self) -> (Vec<VideoDevice>, Vec<AudioDevice>) {
            (Vec::new(), Vec::new())
        }

        pub fn encoders(&self) -> Result<Vec<VideoEncoderDescriptor>, String> {
            Ok(Vec::new())
        }

        pub fn recent_logs(&self) -> Vec<LogEvent> {
            vec![self.offline_log()]
        }

        pub fn start_capture(&self) {
            send_error(&self.events, self.unavailable_message());
        }

        pub fn stop_capture(&self) {
            send_error(&self.events, self.unavailable_message());
        }

        pub fn restart_capture(&self) {
            send_error(&self.events, self.unavailable_message());
        }

        pub fn update_settings(&self, settings: UserSettings) {
            *self.settings.lock().unwrap() = settings.clone();
            let _ = self.events.send(BackendEvent::SettingsUpdated(settings));
        }

        pub fn request_clip(&self) {
            send_error(&self.events, self.unavailable_message());
        }

        pub fn open_clips_folder(&self) {}

        fn unavailable_message(&self) -> String {
            format!(
                "capture backend is unavailable: {}",
                self.unavailable_reason
            )
        }

        fn offline_log(&self) -> LogEvent {
            LogEvent {
                timestamp: Local::now().to_rfc3339(),
                level: LogLevel::Warning,
                source: "backend".to_string(),
                message: self.unavailable_message(),
            }
        }
    }

    impl HotkeyController {
        pub fn new(_backend: Backend) -> Result<Self, String> {
            Err("global shortcuts are disabled in offline mode".to_string())
        }

        pub fn poll(&self) {}

        pub fn sync(&mut self, _accelerator: &str) -> Result<(), String> {
            Err("global shortcuts are disabled in offline mode".to_string())
        }
    }

    fn offline_settings() -> UserSettings {
        UserSettings {
            video_device_id: "screen:0".to_string(),
            system_audio_enabled: true,
            system_audio_volume: 1.0,
            mic_device_id: None,
            mic_volume: 1.0,
            video_encoder_id: "offline".to_string(),
            framerate: 60,
            bitrate_kbps: 20_000,
            clips_dir: "clips".to_string(),
            shortcuts: ShortcutSettings {
                clip: "Ctrl+F10".to_string(),
            },
        }
    }

    fn send_error(events: &Sender<BackendEvent>, message: String) {
        let _ = events.send(BackendEvent::OperationError {
            title: "Backend".to_string(),
            message,
        });
    }
}

#[cfg(not(feature = "backend"))]
pub use offline::*;
