use serde::Serialize;
use std::io;

use gst::prelude::GstObjectExt;
use gstreamer as gst;

#[derive(Debug, Clone, Serialize)]
pub struct VideoEncoderDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_hardware: bool,
    pub required_memory: Option<String>,
}

pub fn list_video_encoders() -> io::Result<Vec<VideoEncoderDescriptor>> {
    gst::init().map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    let factories = gst::ElementFactory::factories_with_type(
        gst::ElementFactoryType::VIDEO_ENCODER,
        gst::Rank::NONE,
    );

    let mut encoders = Vec::new();

    for factory in factories {
        if !supports_h264(&factory) {
            continue;
        }

        let factory_name = factory.name();
        if gst::ElementFactory::make(factory_name.as_str())
            .build()
            .is_err()
        {
            continue;
        }

        let required_memory = required_memory_type(&factory);
        if required_memory.as_deref() == Some("D3D12Memory") {
            continue;
        }
        let is_hardware = factory.has_type(gst::ElementFactoryType::HARDWARE)
            || factory.klass().contains("Hardware");

        encoders.push(VideoEncoderDescriptor {
            id: factory_name.to_string(),
            name: factory.longname().to_string(),
            description: factory.description().to_string(),
            is_hardware,
            required_memory,
        });
    }

    encoders.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(encoders)
}

pub fn find_video_encoder(id: &str) -> io::Result<Option<VideoEncoderDescriptor>> {
    let encoders = list_video_encoders()?;
    Ok(encoders.into_iter().find(|enc| enc.id == id))
}

fn supports_h264(factory: &gst::ElementFactory) -> bool {
    for template in factory.static_pad_templates() {
        if template.direction() != gst::PadDirection::Src {
            continue;
        }
        let caps = template.caps();
        for (structure, _) in caps.iter_with_features() {
            if structure.name() == "video/x-h264" {
                return true;
            }
        }
    }
    false
}

fn required_memory_type(factory: &gst::ElementFactory) -> Option<String> {
    for template in factory.static_pad_templates() {
        if template.direction() != gst::PadDirection::Sink {
            continue;
        }
        let caps = template.caps();
        for (_, features) in caps.iter_with_features() {
            if features.contains("memory:D3D12Memory") {
                return Some("D3D12Memory".to_string());
            }
            if features.contains("memory:D3D11Memory") {
                return Some("D3D11Memory".to_string());
            }
        }
    }

    if factory.name().as_str() == "nvd3d11h264enc" {
        return Some("D3D11Memory".to_string());
    }

    None
}
