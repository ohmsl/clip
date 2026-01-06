use serde::Serialize;
use gstreamer as gst;

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

#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub label: String,
    pub is_input: bool,
    pub caps: gst::Caps,
    pub device: gst::Device,
}

#[cfg(target_os = "windows")]
mod windows {
    use crate::capture_devices::{AudioDevice, AudioDeviceInfo, VideoDevice, VideoDeviceKind};
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

    fn collect_audio_devices() -> Result<Vec<gst::Device>, String> {
        gst::init().map_err(|err| err.to_string())?;

        let monitor = gst::DeviceMonitor::new();
        let audio_caps = gst::Caps::builder("audio/x-raw").build();
        monitor.add_filter(None, Some(&audio_caps));

        monitor.start().map_err(|err| err.to_string())?;
        let devices: Vec<gst::Device> = monitor.devices().into_iter().collect();
        monitor.stop();

        Ok(devices)
    }

    fn device_id_from_props(props: &gst::Structure) -> Option<String> {
        props
            .get::<String>("device")
            .ok()
            .or_else(|| props.get::<String>("device-id").ok())
            .or_else(|| props.get::<String>("device.id").ok())
    }

    fn is_loopback_device(props: &gst::Structure) -> bool {
        props.get::<bool>("loopback").ok().unwrap_or(false)
    }

    fn is_default_device(props: &gst::Structure) -> bool {
        let keys = ["is-default", "is-default-device", "is-default-render", "default"];
        keys.iter()
            .any(|key| props.get::<bool>(*key).ok().unwrap_or(false))
    }

    fn build_audio_device_info(device: gst::Device, is_input: bool) -> Option<AudioDeviceInfo> {
        let props = device.properties()?;
        if is_loopback_device(&props) {
            return None;
        }

        let id = device_id_from_props(&props)?;
        let caps = device.caps().unwrap_or_else(gst::Caps::new_any);
        let label = device.display_name().to_string();

        Some(AudioDeviceInfo {
            id,
            label,
            is_input,
            caps,
            device,
        })
    }

    pub fn find_microphone_device(id: &str) -> Option<AudioDeviceInfo> {
        let devices = collect_audio_devices().ok()?;

        for device in devices {
            let device_class = device.device_class();
            if !device_class.contains("Audio/Source") || device_class.contains("Audio/Sink") {
                continue;
            }

            if let Some(info) = build_audio_device_info(device, true) {
                if info.id == id {
                    return Some(info);
                }
            }
        }

        None
    }

    pub fn find_default_output_device() -> Option<AudioDeviceInfo> {
        let devices = collect_audio_devices().ok()?;

        let mut fallback: Option<AudioDeviceInfo> = None;

        for device in devices {
            let device_class = device.device_class();
            if !device_class.contains("Audio/Sink") || device_class.contains("Audio/Source") {
                continue;
            }

            let props = match device.properties() {
                Some(props) => props,
                None => continue,
            };
            if is_loopback_device(&props) {
                continue;
            }

            if let Some(info) = build_audio_device_info(device, false) {
                if is_default_device(&props) {
                    return Some(info);
                }

                if fallback.is_none() {
                    fallback = Some(info);
                }
            }
        }

        fallback
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    use crate::capture_devices::{AudioDevice, AudioDeviceInfo, VideoDevice};

    pub fn list_video_devices() -> Vec<VideoDevice> {
        Vec::new()
    }

    pub fn list_audio_devices() -> Vec<AudioDevice> {
        Vec::new()
    }

    pub fn list_microphone_devices() -> Vec<AudioDevice> {
        Vec::new()
    }

    pub fn find_microphone_device(_id: &str) -> Option<AudioDeviceInfo> {
        None
    }

    pub fn find_default_output_device() -> Option<AudioDeviceInfo> {
        None
    }
}

pub fn list_video_devices() -> Vec<VideoDevice> {
    #[cfg(target_os = "windows")]
    return windows::list_video_devices();

    #[cfg(not(target_os = "windows"))]
    return other::list_video_devices();
}

pub fn list_microphone_devices() -> Vec<AudioDevice> {
    #[cfg(target_os = "windows")]
    return windows::list_microphone_devices().unwrap_or_default();

    #[cfg(not(target_os = "windows"))]
    return other::list_microphone_devices();
}

pub fn find_microphone_device(id: &str) -> Option<AudioDeviceInfo> {
    #[cfg(target_os = "windows")]
    return windows::find_microphone_device(id);

    #[cfg(not(target_os = "windows"))]
    return other::find_microphone_device(id);
}

pub fn find_default_output_device() -> Option<AudioDeviceInfo> {
    #[cfg(target_os = "windows")]
    return windows::find_default_output_device();

    #[cfg(not(target_os = "windows"))]
    return other::find_default_output_device();
}
