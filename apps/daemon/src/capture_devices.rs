use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum VideoDeviceKind {
    Screen,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoDevice {
    pub id: String,
    pub label: String,
    pub kind: VideoDeviceKind,

    #[cfg(target_os = "windows")]
    pub monitor_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub label: String,
    pub is_input: bool,
}

#[cfg(target_os = "windows")]
mod windows {
    use crate::capture_devices::{AudioDevice, VideoDevice, VideoDeviceKind};
    use gst::prelude::*;
    use gstreamer as gst;
    use windows::{
        Win32::Foundation::{BOOL, LPARAM},
        Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        },
    };

    pub fn list_video_devices() -> Vec<VideoDevice> {
        let mut devices = Vec::new();

        unsafe extern "system" fn enum_monitor(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut windows::Win32::Foundation::RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let data = unsafe { &mut *(lparam.0 as *mut Vec<VideoDevice>) };

            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if unsafe { GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut _ as _) } == false {
                return BOOL(1);
            }

            let label = String::from_utf16_lossy(
                &info
                    .szDevice
                    .iter()
                    .take_while(|c| **c != 0)
                    .cloned()
                    .collect::<Vec<u16>>(),
            );

            let index = data.len() as u32;

            data.push(VideoDevice {
                id: format!("screen:{}", index),
                label,
                kind: VideoDeviceKind::Screen,
                monitor_index: Some(index),
            });

            BOOL(1)
        }

        unsafe {
            EnumDisplayMonitors(
                HDC(0),
                None,
                Some(enum_monitor),
                LPARAM(&mut devices as *mut _ as isize),
            );
        }

        devices
    }

    pub fn list_microphone_devices() -> Result<Vec<AudioDevice>, String> {
        gst::init().map_err(|err| err.to_string())?;

        let monitor = gst::DeviceMonitor::new();
        let audio_caps = gst::Caps::builder("audio/x-raw").build();
        monitor.add_filter(None, Some(&audio_caps));

        monitor.start().map_err(|err| err.to_string())?;
        let devices = monitor.devices();
        monitor.stop();

        let mut microphones = Vec::new();

        for device in devices {
            let device_class = device.device_class();
            if !device_class.contains("Audio/Source") || device_class.contains("Audio/Sink") {
                continue;
            }

            let props = device.properties();
            let is_loopback = props
                .as_ref()
                .and_then(|props| props.get::<bool>("loopback").ok())
                .unwrap_or(false);

            if is_loopback {
                continue;
            }

            let id = props
                .as_ref()
                .and_then(|props| props.get::<String>("device").ok())
                .or_else(|| {
                    props
                        .as_ref()
                        .and_then(|props| props.get::<String>("device-id").ok())
                })
                .or_else(|| {
                    props
                        .as_ref()
                        .and_then(|props| props.get::<String>("device.id").ok())
                });

            let Some(id) = id else {
                continue;
            };

            let label = device.display_name().to_string();
            microphones.push(AudioDevice {
                id,
                label,
                is_input: true,
            });
        }

        Ok(microphones)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use crate::capture_devices::{AudioDevice, VideoDevice, VideoDeviceKind};
    use core_graphics::display::CGDisplay;
    use gst::prelude::*;
    use gstreamer as gst;

    pub fn list_video_devices() -> Vec<VideoDevice> {
        let displays = CGDisplay::active_displays().unwrap_or_default();
        displays
            .into_iter()
            .enumerate()
            .map(|(index, display_id)| VideoDevice {
                // usee the system display id so selection stays stable across runs.
                id: format!("screen:{}", display_id),
                label: format!("Display {}", index + 1),
                kind: VideoDeviceKind::Screen,
            })
            .collect()
    }

    pub fn list_microphone_devices() -> Vec<AudioDevice> {
        if gst::init().is_err() {
            return Vec::new();
        }

        let monitor = gst::DeviceMonitor::new();
        let audio_caps = gst::Caps::builder("audio/x-raw").build();
        monitor.add_filter(None, Some(&audio_caps));

        if monitor.start().is_err() {
            return Vec::new();
        }

        let devices = monitor.devices();
        monitor.stop();

        let mut microphones = Vec::new();

        for device in devices {
            let device_class = device.device_class();
            if !device_class.contains("Audio/Source") || device_class.contains("Audio/Sink") {
                continue;
            }

            let props = device.properties();
            let is_loopback = props
                .as_ref()
                .and_then(|props| props.get::<bool>("loopback").ok())
                .unwrap_or(false);

            if is_loopback {
                continue;
            }

            let id = props
                .as_ref()
                .and_then(|props| props.get::<String>("device").ok())
                .or_else(|| {
                    props
                        .as_ref()
                        .and_then(|props| props.get::<String>("device-id").ok())
                })
                .or_else(|| {
                    props
                        .as_ref()
                        .and_then(|props| props.get::<String>("device.id").ok())
                });

            let Some(id) = id else {
                continue;
            };

            let label = device.display_name().to_string();
            microphones.push(AudioDevice {
                id,
                label,
                is_input: true,
            });
        }

        microphones
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod other {
    use crate::capture_devices::{AudioDevice, VideoDevice};

    pub fn list_video_devices() -> Vec<VideoDevice> {
        Vec::new()
    }

    pub fn list_audio_devices() -> Vec<AudioDevice> {
        Vec::new()
    }

    pub fn list_microphone_devices() -> Vec<AudioDevice> {
        Vec::new()
    }
}

pub fn list_video_devices() -> Vec<VideoDevice> {
    #[cfg(target_os = "windows")]
    return windows::list_video_devices();

    #[cfg(target_os = "macos")]
    return macos::list_video_devices();

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return other::list_video_devices();
}

pub fn list_microphone_devices() -> Vec<AudioDevice> {
    #[cfg(target_os = "windows")]
    return windows::list_microphone_devices().unwrap_or_default();

    #[cfg(target_os = "macos")]
    return macos::list_microphone_devices();

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return other::list_microphone_devices();
}
