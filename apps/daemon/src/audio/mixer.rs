use std::io;

use gst::prelude::*;
use gstreamer as gst;

use super::source::AudioSourceOutput;
use crate::settings::UserSettings;

pub struct AudioMixer;

impl AudioMixer {
    pub fn from_settings(config: &UserSettings) -> io::Result<Option<Self>> {
        let mut sources = 0;

        if config.system_audio_enabled {
            sources += 1;
        }
        if config.mic_device_id.as_ref().is_some_and(|s| !s.is_empty()) {
            sources += 1;
        }

        // No mixer needed if <= 1 source
        if sources <= 1 {
            return Ok(None);
        }

        Ok(Some(Self))
    }

    pub fn build(
        &self,
        pipeline: &gst::Pipeline,
        inputs: Vec<AudioSourceOutput>,
    ) -> io::Result<AudioSourceOutput> {
        let mixer = gst::ElementFactory::make("audiomixer")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audiomixer"))?;

        mixer.set_property("ignore-inactive-pads", &true);

        pipeline
            .add(&mixer)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to add audiomixer"))?;

        for input in inputs {
            let src_pad = input
                .element
                .static_pad("src")
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing src pad"))?;

            let sink_pad = mixer.request_pad_simple("sink_%u").ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "failed to request mixer pad")
            })?;

            src_pad
                .link(&sink_pad)
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link into mixer"))?;
        }

        Ok(AudioSourceOutput {
            element: mixer,
            volume: None,
            source: None,
        })
    }
}
