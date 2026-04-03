use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;

use crate::{gst_utils, ring_buffer::Packet};

pub struct RemuxResult {
    pub duration_ms: u64,
    pub bytes_written: u64,
}

pub fn remux_ts_to_mp4(
    packets: &[Packet],
    output_path: &std::path::Path,
) -> Result<RemuxResult, String> {
    gst::init().map_err(gst_utils::err)?;

    if packets.is_empty() {
        return Err("no packets to remux".to_string());
    }

    let pipeline = gst::Pipeline::new();

    // --- elements ---

    let appsrc = gst_utils::make("appsrc")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| "failed to downcast appsrc")?;

    let tsdemux = gst_utils::make("tsdemux")?;
    let h264parse = gst_utils::make("h264parse")?;
    let aacparse = gst_utils::make("aacparse")?;
    let mp4mux = gst_utils::make("mp4mux")?;
    let video_queue = gst_utils::make("queue")?;
    let audio_queue = gst_utils::make("queue")?;
    let filesink = gst_utils::make("filesink")?;

    // --- config ---

    appsrc.set_property("is-live", &false);
    appsrc.set_property("format", &gst::Format::Time);
    appsrc.set_property("block", &true);

    mp4mux.set_property("faststart", &true);

    let ts_caps = gst::Caps::builder("video/mpegts")
        .field("systemstream", true)
        .field("packetsize", 188i32)
        .build();

    appsrc.set_caps(Some(&ts_caps));

    let location = output_path
        .to_str()
        .ok_or("output path is not valid UTF-8")?;
    filesink.set_property("location", &location);

    // --- pipeline assembly ---

    pipeline
        .add_many(&[
            &appsrc.upcast_ref(),
            &tsdemux,
            &video_queue,
            &audio_queue,
            &h264parse,
            &aacparse,
            &mp4mux,
            &filesink,
        ])
        .map_err(gst_utils::err)?;

    appsrc.link(&tsdemux).map_err(gst_utils::err)?;
    mp4mux.link(&filesink).map_err(gst_utils::err)?;

    // --- dynamic pad handling ---

    let video_queue_sink = video_queue
        .static_pad("sink")
        .ok_or("missing video queue sink")?;
    let audio_queue_sink = audio_queue
        .static_pad("sink")
        .ok_or("missing audio queue sink")?;

    tsdemux.connect_pad_added(move |_, src_pad| {
        let Some(caps) = src_pad.current_caps() else {
            return;
        };
        let Some(s) = caps.structure(0) else { return };

        let name = s.name();

        if name.starts_with("video/") {
            let _ = src_pad.link(&video_queue_sink);
        } else if name.starts_with("audio/") {
            let _ = src_pad.link(&audio_queue_sink);
        }
    });

    video_queue.link(&h264parse).map_err(gst_utils::err)?;
    audio_queue.link(&aacparse).map_err(gst_utils::err)?;

    // --- muxer pads ---

    let video_pad = mp4mux
        .request_pad_simple("video_%u")
        .ok_or("failed to request mp4mux video pad")?;
    let audio_pad = mp4mux
        .request_pad_simple("audio_%u")
        .ok_or("failed to request mp4mux audio pad")?;

    h264parse
        .static_pad("src")
        .ok_or("missing h264parse on src pad")?
        .link(&video_pad)
        .map_err(gst_utils::err)?;
    aacparse
        .static_pad("src")
        .ok_or("missing aacparse on src pad")?
        .link(&audio_pad)
        .map_err(gst_utils::err)?;

    // --- start pipeline ---

    pipeline
        .set_state(gst::State::Playing)
        .map_err(gst_utils::err)?;

    // --- push packets ---

    let mut bytes_written = 0u64;

    for packet in packets {
        let mut buffer = gst::Buffer::with_size(packet.data.len()).map_err(gst_utils::err)?;

        {
            let buffer_ref = buffer.make_mut();

            {
                let mut map = buffer_ref.map_writable().map_err(gst_utils::err)?;

                map.as_mut_slice().copy_from_slice(&packet.data);
            }

            let ts = gst::ClockTime::from_mseconds(packet.pts_ms);
            buffer_ref.set_pts(ts);
            buffer_ref.set_dts(ts);
        }

        bytes_written += packet.data.len() as u64;

        appsrc.push_buffer(buffer).map_err(gst_utils::err)?;
    }

    appsrc.end_of_stream().map_err(gst_utils::err)?;

    // --- BLOCK until EOS ---

    let bus = pipeline.bus().ok_or("missing bus")?;

    loop {
        match bus.timed_pop(gst::ClockTime::from_seconds(5)) {
            Some(msg) => match msg.view() {
                gst::MessageView::Eos(..) => break,
                gst::MessageView::Error(err) => {
                    pipeline.set_state(gst::State::Null).ok();
                    return Err(err.error().to_string());
                }
                _ => {}
            },
            None => {
                pipeline.set_state(gst::State::Null).ok();
                return Err("remux timed out waiting for EOS".to_string());
            }
        }
    }

    pipeline.set_state(gst::State::Null).ok();

    let duration_ms = packets
        .last()
        .unwrap()
        .pts_ms
        .saturating_sub(packets.first().unwrap().pts_ms);

    Ok(RemuxResult {
        duration_ms,
        bytes_written,
    })
}
