use std::io;

use gst::prelude::*;
use gstreamer as gst;

use super::AudioSourceOutput;

pub struct MicAudioSource {
    device_id: String,
}

impl MicAudioSource {
    pub fn from_device(device_id: &str) -> io::Result<Self> {
        if device_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mic device id is empty",
            ));
        }

        Ok(Self {
            device_id: device_id.to_string(),
        })
    }

    pub fn build(
        &self,
        pipeline: &gst::Pipeline,
        volume_value: f32,
    ) -> io::Result<AudioSourceOutput> {
        let src = gst::ElementFactory::make("wasapisrc")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing wasapisrc"))?;

        src.set_property("do-timestamp", &true);
        src.set_property_from_str("device", &self.device_id);

        let convert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audioconvert"))?;

        let resample = gst::ElementFactory::make("audioresample")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audioresample"))?;

        resample.set_property("quality", &10i32);

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing capsfilter"))?;

        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", 48_000i32)
            .field("channels", 2i32)
            .field("layout", "interleaved")
            .build();
        capsfilter.set_property("caps", &caps);

        let volume = gst::ElementFactory::make("volume")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing volume element"))?;
        let volume_value = volume_value as f64;
        volume.set_property("volume", &volume_value);

        let queue = gst::ElementFactory::make("queue")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing queue element"))?;

        queue.set_property("max-size-time", &100_000_000u64);
        queue.set_property_from_str("leaky", "downstream");

        pipeline
            .add_many(&[&src, &convert, &resample, &capsfilter, &volume, &queue])
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to add elements"))?;
        gst::Element::link_many(&[&src, &convert, &resample, &capsfilter, &volume, &queue])
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link elements"))?;

        Ok(AudioSourceOutput {
            element: queue,
            volume: Some(volume),
        })
    }
}
