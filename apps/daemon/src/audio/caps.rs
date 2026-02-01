#[derive(Debug, Clone, Copy)]
pub struct AudioCapsChoice {
    pub rate: i32,
    pub channels: i32,
    pub layout: &'static str,
}

pub fn mix_format_choice() -> AudioCapsChoice {
    AudioCapsChoice {
        rate: 48_000,
        channels: 2,
        layout: "interleaved",
    }
}
