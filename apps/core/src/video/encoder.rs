use crate::gst_utils::GstLinkExt;
use gst::prelude::*;
use gstreamer as gst;
use std::io;

use super::graph::GraphOutput;
use crate::settings::UserSettings;

pub struct VideoEncoder {
    encoder_id: String,
    framerate: u32,
    bitrate_kbps: u32,
}

impl VideoEncoder {
    pub fn from_settings(config: &UserSettings) -> io::Result<Self> {
        Ok(Self {
            encoder_id: config.video_encoder_id.clone(),
            framerate: config.framerate,
            bitrate_kbps: config.bitrate_kbps,
        })
    }

    pub fn build(&self, pipeline: &gst::Pipeline, input: GraphOutput) -> io::Result<GraphOutput> {
        let enc = gst::ElementFactory::make(&self.encoder_id)
            .build()
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("missing encoder {}", self.encoder_id),
                )
            })?;

        let gop = self.framerate.max(1);

        set_u32(&enc, "bitrate", self.bitrate_kbps);
        set_i32(&enc, "gop-size", gop.try_into().unwrap());
        set_u32(&enc, "key-int-max", gop);
        set_bool(&enc, "zero-latency", true);
        set_bool(&enc, "insert-sps-pps", true);

        let h264parse = make("h264parse")?;
        h264parse.set_property("config-interval", &1i32);

        let capsfilter = make("capsfilter")?;
        let caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();

        capsfilter.set_property("caps", &caps);

        pipeline
            .add_many(&[&enc, &h264parse, &capsfilter])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        input.element.link_io(&enc)?;
        enc.link_io(&h264parse)?;
        h264parse.link_io(&capsfilter)?;

        Ok(GraphOutput {
            element: capsfilter,
        })
    }
}

fn make(name: &str) -> io::Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, format!("missing element {}", name)))
}

fn set_i32(element: &gst::Element, name: &str, value: i32) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}

fn set_u32(element: &gst::Element, name: &str, value: u32) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}

fn set_bool(element: &gst::Element, name: &str, value: bool) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}
