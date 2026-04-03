use std::io;

use gst::prelude::*;
use gstreamer as gst;

use crate::{
    audio::{caps::AudioCapsChoice, AudioSourceId},
    capture_devices::AudioDeviceInfo,
    logger,
};

use super::AudioSourceOutput;

pub struct SystemAudioSource {
    device: AudioDeviceInfo,
    caps: AudioCapsChoice,
}

impl SystemAudioSource {
    pub fn from_device(device: AudioDeviceInfo, caps: AudioCapsChoice) -> io::Result<Self> {
        Ok(Self { device, caps })
    }

    pub fn build(
        &self,
        pipeline: &gst::Pipeline,
        volume_value: f32,
    ) -> io::Result<AudioSourceOutput> {
        let make_wasapi_src = || -> io::Result<gst::Element> {
            if let Ok(src) = gst::ElementFactory::make("wasapi2src").build() {
                return Ok(src);
            }
            gst::ElementFactory::make("wasapisrc")
                .build()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing wasapisrc"))
        };
        let endpoint_note = self
            .device
            .endpoint_id
            .as_ref()
            .map(|id| format!(" endpoint={}", id))
            .unwrap_or_default();
        logger::info(
            "audio",
            format!(
                "system device selected: {} ({}){}",
                self.device.label, self.device.id, endpoint_note
            ),
        );
        logger::info(
            "audio",
            format!("system device caps: {}", self.device.caps.to_string()),
        );
        logger::info(
            "audio",
            format!(
                "system target mix caps: rate={}, channels={}, layout={}",
                self.caps.rate, self.caps.channels, self.caps.layout
            ),
        );

        let mut src = match self.device.device.create_element(None) {
            Ok(src) => src,
            Err(err) => {
                logger::warn(
                    "audio",
                    format!("system create_element failed, falling back: {}", err),
                );
                let src = make_wasapi_src()?;
                if src.find_property("device").is_some() {
                    let device_id = self.device.endpoint_id.as_ref().unwrap_or(&self.device.id);
                    src.set_property_from_str("device", device_id);
                }
                src
            }
        };

        if src.find_property("loopback").is_none() {
            logger::warn(
                "audio",
                "system device element is not a wasapisrc; recreating for loopback",
            );
            src = make_wasapi_src()?;
            if src.find_property("device").is_some() {
                let device_id = self.device.endpoint_id.as_ref().unwrap_or(&self.device.id);
                src.set_property_from_str("device", device_id);
            }
        }

        src.set_property("loopback", &true);
        src.set_property("do-timestamp", &true);

        let convert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audioconvert"))?;

        let resample = gst::ElementFactory::make("audioresample")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audioresample"))?;

        resample.set_property("quality", &10i32);

        let rate = gst::ElementFactory::make("audiorate")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing audiorate"))?;

        let capsfilter = gst::ElementFactory::make("capsfilter")
            .build()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "missing capsfilter"))?;

        let caps = gst::Caps::builder("audio/x-raw")
            .field("rate", self.caps.rate)
            .field("channels", self.caps.channels)
            .field("layout", self.caps.layout)
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

        queue.set_property("max-size-time", &500_000_000u64);
        queue.set_property_from_str("leaky", "downstream");

        pipeline
            .add_many(&[
                &src,
                &convert,
                &resample,
                &rate,
                &capsfilter,
                &volume,
                &queue,
            ])
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to add elements"))?;

        gst::Element::link_many(&[
            &src,
            &convert,
            &resample,
            &rate,
            &capsfilter,
            &volume,
            &queue,
        ])
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link elements"))?;

        logger::info(
            "audio",
            format!(
                "system audio coerced to mix caps: rate={}, channels={}, layout={}",
                self.caps.rate, self.caps.channels, self.caps.layout
            ),
        );

        Ok(AudioSourceOutput {
            element: queue,
            volume: Some(volume),
            source: Some(src),
            capsfilter: Some(capsfilter),
            source_id: Some(AudioSourceId::System),
        })
    }
}
