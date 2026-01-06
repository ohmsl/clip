use std::io;

use gstreamer as gst;
use gst::prelude::*;

use crate::{
    audio::{
        caps::AudioCapsPolicy, encoder::AudioEncoder, mixer::AudioMixer, source::AudioSource,
    },
    settings::UserSettings,
};

pub struct AudioVolumes {
    pub system: Option<gst::Element>,
    pub mic: Option<gst::Element>,
}

pub struct GraphOutput {
    pub element: gst::Element,
}

pub struct AudioGraph {
    pub output: GraphOutput,
    pub volumes: AudioVolumes,
    pub sources: Vec<gst::Element>,
}

impl AudioGraph {
    pub fn build(
        pipeline: &gst::Pipeline,
        config: &UserSettings,
        caps_policy: &AudioCapsPolicy,
    ) -> io::Result<Option<Self>> {
        let sources = AudioSource::from_settings(config, caps_policy)?;

        if sources.is_empty() {
            return Ok(None);
        }

        let mut built_sources = Vec::new();
        let mut source_elements = Vec::new();
        let mut volumes = AudioVolumes {
            system: None,
            mic: None,
        };
        for source in sources {
            match source {
                AudioSource::System(s) => {
                    let built = s.build(pipeline, config.system_audio_volume)?;
                    volumes.system = built.volume.clone();
                    if let Some(src) = built.source.clone() {
                        source_elements.push(src);
                    }
                    built_sources.push(built);
                }
                AudioSource::Mic(s) => {
                    let built = s.build(pipeline, config.mic_volume)?;
                    volumes.mic = built.volume.clone();
                    if let Some(src) = built.source.clone() {
                        source_elements.push(src);
                    }
                    built_sources.push(built);
                }
            }
        }

        let mixed = if built_sources.len() == 1 {
            built_sources.into_iter().next().unwrap()
        } else {
            let mixer = AudioMixer::from_settings(config)?.expect("mixer required but not created");

            mixer.build(pipeline, built_sources)?
        };

        let encoder = AudioEncoder::from_settings(config)?;
        let encoded = encoder.build(pipeline, mixed)?;

        Ok(Some(Self {
            output: encoded,
            volumes,
            sources: source_elements,
        }))
    }

    pub fn log_negotiated_caps(&self) {
        for (index, source) in self.sources.iter().enumerate() {
            if let Some(pad) = source.static_pad("src") {
                let caps = pad.current_caps().map(|c| c.to_string());
                crate::logger::info(
                    "audio",
                    format!("source {} negotiated caps: {:?}", index, caps),
                );
            } else {
                crate::logger::warn("audio", format!("source {} has no src pad", index));
            }
        }
    }
}
