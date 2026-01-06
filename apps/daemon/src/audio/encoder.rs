use std::io;

use gst::prelude::*;
use gstreamer as gst;

use crate::settings::UserSettings;

use super::graph::GraphOutput;
use super::source::AudioSourceOutput;

pub struct AudioEncoder;

impl AudioEncoder {
    pub fn from_settings(_config: &UserSettings) -> io::Result<Self> {
        Ok(Self)
    }

    pub fn build(
        &self,
        pipeline: &gst::Pipeline,
        input: AudioSourceOutput,
    ) -> io::Result<GraphOutput> {
        let convert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audioconvert"))?;

        let format_capsfilter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing capsfilter"))?;

        let format_caps = gst::Caps::builder("audio/x-raw")
            .field("format", "S16LE")
            .field("layout", "interleaved")
            .build();
        format_capsfilter.set_property("caps", &format_caps);

        let encoder = make_audio_encoder()?;

        let parser = gst::ElementFactory::make("aacparse")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing aacparse element"))?;

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing capsfilter"))?;

        let caps = gst::Caps::builder("audio/mpeg")
            .field("mpegversion", 4i32)
            .field("stream-format", "adts")
            .build();

        capsfilter.set_property("caps", &caps);

        pipeline
            .add_many(&[&convert, &format_capsfilter, &encoder, &parser, &capsfilter])
            .map_err(|_| {
                io::Error::new(io::ErrorKind::Other, "failed to add elements to pipeline")
            })?;

        gst::Element::link_many(&[
            &input.element,
            &convert,
            &format_capsfilter,
            &encoder,
            &parser,
            &capsfilter,
        ])
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link elements"))?;

        Ok(GraphOutput {
            element: capsfilter,
        })
    }
}

fn make_audio_encoder() -> io::Result<gst::Element> {
    if let Ok(enc) = gst::ElementFactory::make("voaacenc").build() {
        enc.set_property_from_str("bitrate", "192000");
        return Ok(enc);
    }

    if let Ok(enc) = gst::ElementFactory::make("avenc_aac").build() {
        enc.set_property_from_str("bitrate", "192000");
        return Ok(enc);
    }

    Err(io::Error::new(io::ErrorKind::Other, "missing AAC encoder"))
}
