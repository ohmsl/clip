use gstreamer as gst;

#[derive(Debug, Clone, Copy)]
pub struct AudioCapsChoice {
    pub rate: i32,
    pub channels: i32,
}

#[derive(Debug, Clone)]
pub struct AudioCapsPolicy {
    pub preferred_rates: Vec<i32>,
    pub preferred_channels: Vec<i32>,
}

impl AudioCapsPolicy {
    pub fn default() -> Self {
        Self {
            preferred_rates: vec![48_000, 44_100, 16_000],
            preferred_channels: vec![2, 1],
        }
    }

    pub fn safe_fallback() -> Self {
        Self {
            preferred_rates: vec![16_000, 44_100, 48_000],
            preferred_channels: vec![1, 2],
        }
    }
}

fn caps_supports_rate_channels(caps: &gst::Caps, rate: i32, channels: i32) -> bool {
    let desired = gst::Caps::builder("audio/x-raw")
        .field("rate", rate)
        .field("channels", channels)
        .build();
    caps.can_intersect(&desired)
}

pub fn choose_caps_single(
    device_caps: &gst::Caps,
    policy: &AudioCapsPolicy,
) -> Option<AudioCapsChoice> {
    for &rate in &policy.preferred_rates {
        for &channels in &policy.preferred_channels {
            if caps_supports_rate_channels(device_caps, rate, channels) {
                return Some(AudioCapsChoice { rate, channels });
            }
        }
    }
    None
}

pub fn choose_caps_common(
    caps_a: &gst::Caps,
    caps_b: &gst::Caps,
    policy: &AudioCapsPolicy,
) -> Option<AudioCapsChoice> {
    for &rate in &policy.preferred_rates {
        for &channels in &policy.preferred_channels {
            if caps_supports_rate_channels(caps_a, rate, channels)
                && caps_supports_rate_channels(caps_b, rate, channels)
            {
                return Some(AudioCapsChoice { rate, channels });
            }
        }
    }
    None
}
