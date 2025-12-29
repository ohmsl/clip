use std::io;

use crate::{gst_capture::GstCapture, logger, state::DaemonState};

/// Start or restart capture to match the current UserSettings.
/// This function is the ONLY place that starts or stops capture.
pub fn restart_capture(state: &mut DaemonState) -> Result<(), io::Error> {
    // Stop existing capture (if any)
    if let Some(mut old) = state.capture.take() {
        logger::info("capture", "stopping existing pipeline");
        old.stop();
    }

    // Clear ring buffer (new timeline)
    {
        let mut rb = state.ring_buffer.lock().unwrap();
        rb.clear();
    }

    if state.settings.video_device_id.is_empty() {
        logger::warn("capture", "no video device available; capture disabled");
        return Ok(());
    }

    // Start capture pipeline
    let capture = GstCapture::start(&state.settings, state.ring_buffer.clone())?;
    state.capture = Some(capture);
    logger::info("capture", "pipeline started");

    Ok(())
}
