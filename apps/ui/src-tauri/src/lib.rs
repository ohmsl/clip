use std::{
    fs::{self},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::Local;
use clip_service::{
    audio::AudioSourceId,
    capture_devices::{
        list_microphone_devices as list_microphone_devices_inner,
        list_video_devices as list_video_devices_inner, AudioDevice, VideoDevice,
    },
    encoders::{list_video_encoders as list_video_encoders_inner, VideoEncoderDescriptor},
    gst_capture::{AudioCapsState, GstCapture},
    logger,
    ring_buffer::RingBuffer,
    settings::{
        apply_startup_fallbacks, default_settings, load_settings, save_settings, validate_settings,
        UserSettings,
    },
};

use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

struct CaptureRuntime {
    settings: UserSettings,
    capture: Option<GstCapture>,
    ring_buffer: Arc<Mutex<RingBuffer>>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    settings: UserSettings,
    buffering: bool,
    buffer_seconds: u32,
    ring_buffer_packets: usize,
    ring_buffer_bytes: u64,
    ring_buffer_duration_ms: u64,
    audio_caps: AudioCapsState,
}

#[derive(Serialize)]
struct ClipResponse {
    filename: String,
    packets: usize,
    duration_ms: u64,
    bytes: usize,
}

#[derive(Serialize)]
struct ClipInfo {
    filename: String,
    size_bytes: u64,
}

#[derive(Clone, Serialize)]
struct CaptureStatusEvent {
    status: String,
    message: Option<String>,
}

fn emit_capture_status(app: &AppHandle, status: &str, message: Option<String>) {
    let payload = CaptureStatusEvent {
        status: status.to_string(),
        message,
    };
    let _ = app.emit("capture-status", payload);
}

fn should_restart_capture(a: &UserSettings, b: &UserSettings) -> bool {
    a.video_device_id != b.video_device_id
        || a.system_audio_enabled != b.system_audio_enabled
        || a.mic_device_id != b.mic_device_id
        || a.video_encoder_id != b.video_encoder_id
        || a.framerate != b.framerate
        || a.bitrate_kbps != b.bitrate_kbps
}

fn apply_volume_elements(
    system_volume: Option<gst::Element>,
    mic_volume: Option<gst::Element>,
    settings: &UserSettings,
) {
    if let Some(element) = system_volume {
        let value = settings.system_audio_volume as f64;
        element.set_property("volume", &value);
    }
    if let Some(element) = mic_volume {
        let value = settings.mic_volume as f64;
        element.set_property("volume", &value);
    }
}

fn replace_capture(state: &State<'_, Mutex<CaptureRuntime>>, new_capture: Option<GstCapture>) {
    let mut guard = state.lock().unwrap();
    guard.capture = new_capture;
}

fn resolve_settings() -> Result<UserSettings, String> {
    let video_devices = list_video_devices_inner();
    let microphones = list_microphone_devices_inner();
    let encoders = list_video_encoders_inner().map_err(|err| err.to_string())?;

    let loaded_settings = load_settings().map_err(|err| err.to_string())?;
    let mut settings = match loaded_settings.as_ref() {
        Some(loaded) => {
            logger::info("settings", "loaded from disk");
            loaded.clone()
        }
        None => {
            let defaults =
                default_settings(&video_devices, &encoders).map_err(|err| err.to_string())?;
            logger::info("settings", "created defaults");
            defaults
        }
    };

    let (validated, changes) =
        apply_startup_fallbacks(settings.clone(), &video_devices, &microphones, &encoders);
    if !changes.is_empty() {
        for change in &changes {
            logger::info("settings", format!("{}", change));
        }
        settings = validated;
        save_settings(&settings).map_err(|err| err.to_string())?;
    } else if loaded_settings.is_none() {
        save_settings(&settings).map_err(|err| err.to_string())?;
    }

    Ok(settings)
}

fn build_runtime() -> Result<CaptureRuntime, String> {
    let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(30_000)));
    let settings = resolve_settings()?;
    Ok(CaptureRuntime {
        settings,
        capture: None,
        ring_buffer,
    })
}

fn spawn_log_forwarder(app: AppHandle) {
    let mut receiver = logger::subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            let _ = app.emit("capture-log", event);
        }
    });
}

#[tauri::command]
fn get_status(state: State<'_, Mutex<CaptureRuntime>>) -> StatusResponse {
    let guard = state.lock().unwrap();
    let rb = guard.ring_buffer.lock().unwrap();
    let buffer_seconds = (rb.duration_ms() / 1000) as u32;
    let audio_caps = guard
        .capture
        .as_ref()
        .map(|capture| capture.audio_caps())
        .unwrap_or_default();

    StatusResponse {
        settings: guard.settings.clone(),
        buffering: guard.capture.is_some(),
        buffer_seconds,
        ring_buffer_packets: rb.len(),
        ring_buffer_bytes: rb.total_bytes(),
        ring_buffer_duration_ms: rb.duration_ms(),
        audio_caps,
    }
}

#[tauri::command]
fn list_video_devices() -> Vec<VideoDevice> {
    list_video_devices_inner()
}

#[tauri::command]
fn list_microphone_devices() -> Vec<AudioDevice> {
    list_microphone_devices_inner()
}

#[tauri::command]
fn list_video_encoders() -> Result<Vec<VideoEncoderDescriptor>, String> {
    list_video_encoders_inner().map_err(|err| err.to_string())
}

#[tauri::command]
fn get_settings(state: State<'_, Mutex<CaptureRuntime>>) -> UserSettings {
    let guard = state.lock().unwrap();
    guard.settings.clone()
}

#[tauri::command]
fn get_recent_logs() -> Vec<clip_service::logger::LogEvent> {
    logger::recent_logs()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, Mutex<CaptureRuntime>>,
    new_settings: UserSettings,
) -> Result<UserSettings, String> {
    let video_devices = list_video_devices_inner();
    let microphones = list_microphone_devices_inner();
    let encoders = list_video_encoders_inner().map_err(|err| {
        logger::error("settings", format!("failed to list encoders: {}", err));
        err.to_string()
    })?;

    if let Err(message) = validate_settings(&new_settings, &video_devices, &microphones, &encoders)
    {
        return Err(message);
    }

    let (old_capture, should_restart, saved_settings, volume_targets, volume_changed) = {
        let mut guard = state.lock().unwrap();
        let restart = should_restart_capture(&guard.settings, &new_settings);
        let volume_changed = guard.settings.system_audio_volume != new_settings.system_audio_volume
            || guard.settings.mic_volume != new_settings.mic_volume;
        let volume_targets = if !restart {
            guard
                .capture
                .as_ref()
                .map(|capture| {
                    (
                        capture.volume_element(AudioSourceId::System),
                        capture.volume_element(AudioSourceId::Mic),
                    )
                })
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        guard.settings = new_settings.clone();
        let captured = if restart { guard.capture.take() } else { None };
        (
            captured,
            restart,
            guard.settings.clone(),
            volume_targets,
            volume_changed,
        )
    };

    save_settings(&saved_settings).map_err(|err| {
        logger::error("settings", format!("failed to save: {}", err));
        err.to_string()
    })?;

    if should_restart {
        logger::info("capture", "stopping existing pipeline");
    }
    drop(old_capture);
    if should_restart {
        let ring_buffer = {
            let guard = state.lock().unwrap();
            guard.ring_buffer.clone()
        };
        {
            let mut rb = ring_buffer.lock().unwrap();
            rb.clear();
        }
        let new_capture = GstCapture::start(&saved_settings, ring_buffer).map_err(|err| {
            let message = err.to_string();
            logger::error("settings", format!("restart failed: {}", message));
            emit_capture_status(&app, "error", Some(message.clone()));
            message
        })?;
        replace_capture(&state, Some(new_capture));
    } else if volume_changed {
        apply_volume_elements(volume_targets.0, volume_targets.1, &saved_settings);
    }

    logger::info("settings", "updated and capture restarted");
    emit_capture_status(&app, "running", None);

    Ok(saved_settings)
}

#[tauri::command]
fn start_capture(app: AppHandle, state: State<'_, Mutex<CaptureRuntime>>) -> Result<(), String> {
    let (settings, ring_buffer, has_capture) = {
        let guard = state.lock().unwrap();
        (
            guard.settings.clone(),
            guard.ring_buffer.clone(),
            guard.capture.is_some(),
        )
    };

    if has_capture {
        return Err("capture already running".to_string());
    }

    {
        let mut rb = ring_buffer.lock().unwrap();
        rb.clear();
    }

    let capture = GstCapture::start(&settings, ring_buffer).map_err(|err| {
        logger::error("capture", format!("start failed: {}", err));
        let message = err.to_string();
        emit_capture_status(&app, "error", Some(message.clone()));
        message
    })?;
    replace_capture(&state, Some(capture));
    emit_capture_status(&app, "running", None);
    Ok(())
}

#[tauri::command]
fn stop_capture(app: AppHandle, state: State<'_, Mutex<CaptureRuntime>>) -> Result<(), String> {
    let old_capture = {
        let mut guard = state.lock().unwrap();
        guard.capture.take()
    };
    if old_capture.is_some() {
        logger::info("capture", "stopping pipeline");
    }
    drop(old_capture);
    emit_capture_status(&app, "stopped", None);
    Ok(())
}

#[tauri::command]
fn restart_capture(app: AppHandle, state: State<'_, Mutex<CaptureRuntime>>) -> Result<(), String> {
    let (old_capture, settings, ring_buffer) = {
        let mut guard = state.lock().unwrap();
        (
            guard.capture.take(),
            guard.settings.clone(),
            guard.ring_buffer.clone(),
        )
    };

    if old_capture.is_some() {
        logger::info("capture", "stopping existing pipeline");
    }
    drop(old_capture);
    {
        let mut rb = ring_buffer.lock().unwrap();
        rb.clear();
    }

    let capture = GstCapture::start(&settings, ring_buffer).map_err(|err| {
        logger::error("capture", format!("restart failed: {}", err));
        let message = err.to_string();
        emit_capture_status(&app, "error", Some(message.clone()));
        message
    })?;
    replace_capture(&state, Some(capture));
    emit_capture_status(&app, "running", None);
    Ok(())
}

#[tauri::command]
fn set_audio_volume(
    state: State<'_, Mutex<CaptureRuntime>>,
    source: AudioSourceId,
    value: f32,
) -> Result<(), String> {
    if !(0.0..=2.0).contains(&value) {
        return Err("volume must be between 0.0 and 2.0".to_string());
    }

    let target = {
        let guard = state.lock().unwrap();
        guard
            .capture
            .as_ref()
            .and_then(|capture| capture.volume_element(source))
    };

    if let Some(element) = target {
        let value = value as f64;
        element.set_property("volume", &value);
        Ok(())
    } else {
        Err("volume control not available".to_string())
    }
}

#[tauri::command]
async fn clip(state: State<'_, Mutex<CaptureRuntime>>) -> Result<ClipResponse, String> {
    let (packets, clips_dir) = {
        let guard = state.lock().unwrap();
        let mut rb = guard.ring_buffer.lock().unwrap();
        let packets = rb.drain_from_keyframe();
        (packets, guard.settings.clips_dir.clone())
    };

    if packets.is_empty() {
        return Err("no packets available".to_string());
    }

    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("clip-{}.mp4", timestamp);

    let mut path = PathBuf::from(clips_dir);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(&filename);

    let packets_clone = packets.clone();
    let path_clone = path.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        clip_service::remux::remux_ts_to_mp4(&packets_clone, &path_clone)
    })
    .await
    .map_err(|e| e.to_string())??;

    logger::info("capture", format!("Clip saved to {}", path.display()));

    Ok(ClipResponse {
        filename,
        packets: packets.len(),
        duration_ms: result.duration_ms,
        bytes: result.bytes_written as usize,
    })
}

#[tauri::command]
fn list_clips(state: State<'_, Mutex<CaptureRuntime>>) -> Vec<ClipInfo> {
    let mut clips = Vec::new();
    let clips_dir = {
        let guard = state.lock().unwrap();
        PathBuf::from(guard.settings.clips_dir.clone())
    };

    if let Ok(entries) = fs::read_dir(&clips_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        clips.push(ClipInfo {
                            filename: name.to_string(),
                            size_bytes: metadata.len(),
                        });
                    }
                }
            }
        }
    }

    clips
}

#[tauri::command]
fn get_clips_dir(state: State<'_, Mutex<CaptureRuntime>>) -> Result<String, String> {
    let path = {
        let guard = state.lock().unwrap();
        PathBuf::from(guard.settings.clips_dir.clone())
    };
    fs::create_dir_all(&path).map_err(|err| err.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logger::init_logging();
    let runtime = build_runtime().expect("failed to initialize capture runtime");

    tauri::Builder::default()
        .manage(Mutex::new(runtime))
        .setup(|app| {
            spawn_log_forwarder(app.handle().clone());

            let state = app.state::<Mutex<CaptureRuntime>>();
            let (settings, ring_buffer, has_capture) = {
                let guard = state.lock().unwrap();
                (
                    guard.settings.clone(),
                    guard.ring_buffer.clone(),
                    guard.capture.is_some(),
                )
            };

            if !has_capture {
                {
                    let mut rb = ring_buffer.lock().unwrap();
                    rb.clear();
                }
                match GstCapture::start(&settings, ring_buffer) {
                    Ok(capture) => {
                        replace_capture(&state, Some(capture));
                        emit_capture_status(&app.handle(), "running", None);
                    }
                    Err(err) => {
                        let message = err.to_string();
                        logger::error("capture", format!("startup failed: {}", message));
                        emit_capture_status(&app.handle(), "error", Some(message));
                    }
                }
            }

            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            list_video_devices,
            list_microphone_devices,
            list_video_encoders,
            get_settings,
            get_recent_logs,
            update_settings,
            start_capture,
            stop_capture,
            restart_capture,
            set_audio_volume,
            clip,
            list_clips,
            get_clips_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
