use gst::prelude::*;
use gstreamer as gst;
use std::{
    io,
    sync::{Arc, Mutex},
};

use super::{encoder::VideoEncoder, source::VideoSource};
use crate::{ring_buffer::RingBuffer, settings::UserSettings};

// A small wrapper meaning:
// "This element has a usable src pad"
pub struct GraphOutput {
    pub element: gst::Element,
}

// Complete video grapH:
// source -> transforms -> encoder -> parser -> caps
pub struct VideoGraph {
    pub output: GraphOutput,
}

impl VideoGraph {
    pub fn build(pipeline: &gst::Pipeline, config: &UserSettings) -> io::Result<Self> {
        // 1) Build source
        let source = VideoSource::from_settings(config)?;
        let src_out = source.build(pipeline)?;

        // 2) Build encoder
        let encoder = VideoEncoder::from_settings(config)?;
        let encoded_out = encoder.build(pipeline, src_out)?;

        let queue = gst::ElementFactory::make("queue")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to create queue element"))?;
        pipeline
            .add(&queue)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to add queue element"))?;
        encoded_out
            .element
            .link(&queue)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link queue element"))?;

        Ok(Self {
            output: GraphOutput { element: queue },
        })
    }

    pub fn attach_keyframe_tracker(&self, _ring_buffer: Arc<Mutex<RingBuffer>>) -> io::Result<()> {
        Ok(())
    }
}
