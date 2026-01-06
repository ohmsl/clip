use std::io;

use gstreamer as gst;

use crate::{
    audio::caps::{choose_caps_common, choose_caps_single, AudioCapsPolicy},
    capture_devices::{find_default_output_device, find_microphone_device},
    logger,
    settings::UserSettings,
};

use serde::{Deserialize, Serialize};

use super::{mic::MicAudioSource, system::SystemAudioSource};

pub enum AudioSource {
    System(SystemAudioSource),
    Mic(MicAudioSource),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceId {
    System,
    Mic,
}

pub struct AudioSourceOutput {
    pub element: gst::Element,
    pub volume: Option<gst::Element>,
    pub source: Option<gst::Element>,
}

// Audio sources should do the following:
// - Capture
// - Convert format
// - Normalize to device-supported rate/channel caps for mixer compatibility
impl AudioSource {
    pub fn from_settings(
        config: &UserSettings,
        caps_policy: &AudioCapsPolicy,
    ) -> io::Result<Vec<Self>> {
        let mut sources = Vec::new();

        let system_device = if config.system_audio_enabled {
            Some(find_default_output_device().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "no output audio device available")
            })?)
        } else {
            None
        };

        let mic_device = if let Some(id) = config.mic_device_id.as_ref().filter(|s| !s.is_empty()) {
            Some(find_microphone_device(id).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "selected microphone device not available",
                )
            })?)
        } else {
            None
        };

        let chosen_caps = match (&system_device, &mic_device) {
            (Some(system), Some(mic)) => choose_caps_common(&system.caps, &mic.caps, caps_policy)
                .ok_or_else(|| {
                    logger::error(
                        "audio",
                        format!(
                            "no common caps. system caps: {}, mic caps: {}",
                            system.caps.to_string(),
                            mic.caps.to_string()
                        ),
                    );
                    io::Error::new(
                        io::ErrorKind::Other,
                        "no common audio caps between system and mic devices",
                    )
                })?,
            (Some(system), None) => choose_caps_single(&system.caps, caps_policy).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "no compatible audio caps for system device",
                )
            })?,
            (None, Some(mic)) => choose_caps_single(&mic.caps, caps_policy).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "no compatible audio caps for microphone device",
                )
            })?,
            (None, None) => return Ok(sources),
        };

        if let Some(system_device) = system_device {
            sources.push(AudioSource::System(SystemAudioSource::from_device(
                system_device,
                chosen_caps,
            )?));
        }

        if let Some(mic_device) = mic_device {
            sources.push(AudioSource::Mic(MicAudioSource::from_device(
                mic_device,
                chosen_caps,
            )?));
        }

        Ok(sources)
    }

    pub fn build(self, pipeline: &gst::Pipeline, volume: f32) -> io::Result<AudioSourceOutput> {
        match self {
            AudioSource::System(s) => s.build(pipeline, volume),
            AudioSource::Mic(s) => s.build(pipeline, volume),
        }
    }
}
