use gst::prelude::*;
use gstreamer as gst;
use std::io;

pub trait GstLinkExt {
    fn link_io(&self, other: &gst::Element) -> io::Result<()>;
}

impl GstLinkExt for gst::Element {
    fn link_io(&self, other: &gst::Element) -> io::Result<()> {
        self.link(other).map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("failed to link {} -> {}", self.name(), other.name()),
            )
        })
    }
}

pub fn make(name: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| format!("missing gstreamer element: {}", name))
}

pub fn err<T: ToString>(e: T) -> String {
    e.to_string()
}
