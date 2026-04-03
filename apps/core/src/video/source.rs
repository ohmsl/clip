use crate::gst_utils::GstLinkExt;
use gst::prelude::*;
use gstreamer as gst;
use std::io;

use super::graph::GraphOutput;
use crate::encoders;
use crate::settings::UserSettings;

/// What kind of video source are we builidng
pub enum VideoSource {
    Screen {
        monitor_id: String,
        framerate: u32,
        requires_d3d11: bool,
    },
}

impl VideoSource {
    pub fn from_settings(config: &UserSettings) -> io::Result<Self> {
        let encoder_info =
            encoders::find_video_encoder(&config.video_encoder_id)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "selected encoder not available")
            })?;

        let requires_d3d11 = encoder_info.required_memory.as_deref() == Some("D3D11Memory");

        if encoder_info.required_memory.as_deref() == Some("D3D12Memory") {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "D3D12 encoders are not supported",
            ));
        }

        Ok(Self::Screen {
            monitor_id: config.video_device_id.clone(),
            framerate: config.framerate,
            requires_d3d11,
        })
    }

    pub fn build(&self, pipeline: &gst::Pipeline) -> io::Result<GraphOutput> {
        match self {
            VideoSource::Screen {
                monitor_id,
                framerate,
                requires_d3d11,
            } => build_screen_source(pipeline, monitor_id, *framerate, *requires_d3d11),
        }
    }
}

fn build_screen_source(
    pipeline: &gst::Pipeline,
    monitor_id: &str,
    framerate: u32,
    requires_d3d11: bool,
) -> io::Result<GraphOutput> {
    let video_src = gst::ElementFactory::make("d3d11screencapturesrc")
        .build()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing d3d11screencapturesrc"))?;

    if let Some(index) = monitor_index_from_id(monitor_id) {
        video_src.set_property("monitor_index", &index);
    }

    video_src.set_property("do-timestamp", &true);

    let d3d11convert = make("d3d11convert")?;
    let capsfilter = make("capsfilter")?;

    let caps = if requires_d3d11 {
        let structure = gst::Structure::builder("video/x-raw")
            .field("format", "NV12")
            .field("framerate", gst::Fraction::new(framerate as i32, 1))
            .build();

        let features = gst::CapsFeatures::new(["memory:D3D11Memory"]);

        gst::Caps::builder_full_with_features(features.clone())
            .structure_with_features(structure, features)
            .build()
    } else {
        gst::Caps::builder("video/x-raw")
            .field("format", "NV12")
            .field("framerate", gst::Fraction::new(framerate as i32, 1))
            .build()
    };

    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many(&[&video_src, &d3d11convert, &capsfilter])
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    video_src.link_io(&d3d11convert)?;
    d3d11convert.link_io(&capsfilter)?;

    Ok(GraphOutput {
        element: capsfilter,
    })
}

fn make(name: &str) -> io::Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, format!("missing element {}", name)))
}

fn monitor_index_from_id(id: &str) -> Option<i32> {
    let mut parts = id.split(':');
    if parts.next()? != "screen" {
        return None;
    }
    parts.next()?.parse().ok()
}
