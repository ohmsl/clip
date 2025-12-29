use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use gst::prelude::*;
use gstreamer as gst;
use gstreamer::glib;
use gstreamer_app as gst_app;

use crate::{
    encoders, logger,
    ring_buffer::{Packet, RingBuffer},
    settings::UserSettings,
};

/// Capture core boundary:
/// - Owns the GStreamer pipeline lifecycle and elements.
/// - Emits encoded packets into the ring buffer.
/// - Exposes explicit capture state (Starting/Running/Failed/Stopped).
/// - Does NOT know about routes, UI state, or settings mutation.
#[derive(Debug, Clone)]
pub enum CaptureState {
    Starting,
    Running,
    Failed(String),
    Stopped,
}

pub struct GstCapture {
    pipeline: gst::Pipeline,
    appsink: gst_app::AppSink,
    state: Arc<Mutex<CaptureState>>,
    stop_flag: Arc<AtomicBool>,
    bus_thread: Option<std::thread::JoinHandle<()>>,
    callback_guard: Arc<AtomicBool>,
    h264_probe: Option<(gst::Pad, gst::PadProbeId)>,
}

impl GstCapture {
    pub fn start(config: &UserSettings, ring_buffer: Arc<Mutex<RingBuffer>>) -> io::Result<Self> {
        let callback_guard = Arc::new(AtomicBool::new(false));

        gst::init().map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        validate_config(config)?;

        let pipeline = gst::Pipeline::new();
        let state = Arc::new(Mutex::new(CaptureState::Starting));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let video_src = make_video_src(&config.video_device_id)?;

        let d3d11convert = if cfg!(target_os = "windows") {
            Some(make_element("d3d11convert")?)
        } else {
            None
        };

        let video_capsfilter = make_element("capsfilter")?;

        let encoder_info =
            encoders::find_video_encoder(&config.video_encoder_id)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "selected encoder not available")
            })?;
        let requires_d3d11 = encoder_info.required_memory.as_deref() == Some("D3D11Memory");

        if encoder_info.required_memory.as_deref() == Some("D3D12Memory") {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "selected encoder requires D3D12 memory, which is not supported",
            ));
        }

        if requires_d3d11 && !cfg!(target_os = "windows") {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "selected encoder requires D3D11 memory, which is only supported on Windows",
            ));
        }

        let caps = if requires_d3d11 {
            let structure = gst::Structure::builder("video/x-raw")
                .field("format", "NV12")
                .field("framerate", gst::Fraction::new(config.framerate as i32, 1))
                .build();
            let features = gst::CapsFeatures::new(["memory:D3D11Memory"]);
            gst::Caps::builder_full_with_features(features.clone())
                .structure_with_features(structure, features)
                .build()
        } else {
            gst::Caps::builder("video/x-raw")
                .field("format", "NV12")
                .field("framerate", gst::Fraction::new(config.framerate as i32, 1))
                .build()
        };

        video_capsfilter.set_property("caps", &caps);

        let video_encoder = make_video_encoder(
            &config.video_encoder_id,
            config.framerate,
            config.bitrate_kbps,
        )?;

        let h264parse = make_element("h264parse")?;
        set_i32_property(&h264parse, "config-interval", 1);

        let h264_capsfilter = make_element("capsfilter")?;
        let h264_caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();
        h264_capsfilter.set_property("caps", &h264_caps);

        let video_queue = make_queue()?;

        let system_audio_enabled = config.system_audio_enabled;
        let mic_device = config.mic_device_id.as_ref().filter(|id| !id.is_empty());
        let mic_enabled = mic_device.is_some();
        let has_audio = system_audio_enabled || mic_enabled;
        let mix_audio = system_audio_enabled && mic_enabled;

        let audio_caps = if has_audio {
            Some(
                gst::Caps::builder("audio/x-raw")
                    .field("rate", 48_000i32)
                    .field("channels", 2i32)
                    .build(),
            )
        } else {
            None
        };

        let audio_src = if system_audio_enabled {
            Some(make_audio_src(true, None)?)
        } else {
            None
        };

        let audioconvert = if system_audio_enabled {
            Some(make_element("audioconvert")?)
        } else {
            None
        };
        let audioresample = if system_audio_enabled {
            Some(make_element("audioresample")?)
        } else {
            None
        };
        let audiorate = if system_audio_enabled {
            Some(make_element("audiorate")?)
        } else {
            None
        };
        let audio_capsfilter = if system_audio_enabled {
            Some(make_element("capsfilter")?)
        } else {
            None
        };
        if let (Some(filter), Some(caps)) = (audio_capsfilter.as_ref(), audio_caps.as_ref()) {
            filter.set_property("caps", caps);
        }

        let audio_mixer = if mix_audio {
            Some(make_element("audiomixer")?)
        } else {
            None
        };

        let system_queue = if mix_audio { Some(make_queue()?) } else { None };

        let mic_src = if mic_enabled {
            Some(make_audio_src(false, mic_device.map(|id| id.as_str()))?)
        } else {
            None
        };

        let mic_convert = mic_src
            .as_ref()
            .map(|_| make_element("audioconvert"))
            .transpose()?;
        let mic_resample = mic_src
            .as_ref()
            .map(|_| make_element("audioresample"))
            .transpose()?;
        let mic_rate = mic_src
            .as_ref()
            .map(|_| make_element("audiorate"))
            .transpose()?;
        let mic_capsfilter = mic_src
            .as_ref()
            .map(|_| make_element("capsfilter"))
            .transpose()?;
        if let (Some(filter), Some(caps)) = (mic_capsfilter.as_ref(), audio_caps.as_ref()) {
            filter.set_property("caps", caps);
        }
        let mic_queue = mic_src.as_ref().map(|_| make_queue()).transpose()?;

        let audio_encoder = if has_audio {
            Some(make_audio_encoder()?)
        } else {
            None
        };
        let aacparse = if has_audio {
            let parser = make_element("aacparse")?;
            set_str_property(&parser, "output-format", "adts");
            set_str_property(&parser, "format", "adts");
            Some(parser)
        } else {
            None
        };
        let audio_queue = if has_audio { Some(make_queue()?) } else { None };

        let mux = make_element("mpegtsmux")?;
        set_bool_property(&mux, "streamable", true);

        let appsink = make_element("appsink")?;
        let appsink = appsink
            .downcast::<gst_app::AppSink>()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "appsink downcast failed"))?;

        appsink.set_property("emit-signals", &true);
        appsink.set_property("sync", &false);
        appsink.set_property("async", &false);
        appsink.set_property("drop", &true);
        appsink.set_property("max-buffers", &4u32);

        let (d3d11download, videoconvert) = if cfg!(target_os = "windows") && !requires_d3d11 {
            (Some(make_element("d3d11download")?), Some(make_element("videoconvert")?))
        } else if cfg!(target_os = "windows") {
            (None, None)
        } else {
            (None, Some(make_element("videoconvert")?))
        };

        // Video chain: src -> optional transforms -> capsfilter -> encoder -> parser -> caps -> queue.
        let mut elements: Vec<&gst::Element> = vec![&video_src];
        if let Some(convert) = d3d11convert.as_ref() {
            elements.push(convert);
        }
        if let Some(download) = d3d11download.as_ref() {
            elements.push(download);
        }
        if let Some(convert) = videoconvert.as_ref() {
            elements.push(convert);
        }
        elements.extend_from_slice(&[
            &video_capsfilter,
            &video_encoder,
            &h264parse,
            &h264_capsfilter,
            &video_queue,
            &mux,
            appsink.upcast_ref(),
        ]);

        if let Some(element) = audio_src.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audioconvert.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audioresample.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audiorate.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audio_capsfilter.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audio_mixer.as_ref() {
            elements.push(element);
        }
        if let Some(element) = system_queue.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_src.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_convert.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_resample.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_rate.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_capsfilter.as_ref() {
            elements.push(element);
        }
        if let Some(element) = mic_queue.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audio_encoder.as_ref() {
            elements.push(element);
        }
        if let Some(element) = aacparse.as_ref() {
            elements.push(element);
        }
        if let Some(element) = audio_queue.as_ref() {
            elements.push(element);
        }

        pipeline
            .add_many(&elements)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        fn link(a: &gst::Element, b: &gst::Element) -> io::Result<()> {
            a.link(b).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to link {} -> {}", a.name(), b.name()),
                )
            })
        }

        let mut last = &video_src;
        if let Some(convert) = d3d11convert.as_ref() {
            link(last, convert)?;
            last = convert;
        }
        if let Some(download) = d3d11download.as_ref() {
            link(last, download)?;
            last = download;
        }
        if let Some(convert) = videoconvert.as_ref() {
            link(last, convert)?;
            last = convert;
        }
        link(last, &video_capsfilter)?;
        link(&video_capsfilter, &video_encoder)?;
        link(&video_encoder, &h264parse)?;
        link(&h264parse, &h264_capsfilter)?;
        link(&h264_capsfilter, &video_queue)?;

        if has_audio {
            if mix_audio {
                if let (
                    Some(audio_src),
                    Some(audioconvert),
                    Some(audioresample),
                    Some(audiorate),
                    Some(audio_capsfilter),
                    Some(system_queue),
                    Some(audio_mixer),
                ) = (
                    audio_src.as_ref(),
                    audioconvert.as_ref(),
                    audioresample.as_ref(),
                    audiorate.as_ref(),
                    audio_capsfilter.as_ref(),
                    system_queue.as_ref(),
                    audio_mixer.as_ref(),
                ) {
                    gst::Element::link_many(&[
                        audio_src,
                        audioconvert,
                        audioresample,
                        audiorate,
                        audio_capsfilter,
                        system_queue,
                    ])
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "failed to link system audio")
                    })?;

                    link_queue_to_mixer(system_queue, audio_mixer, "system")?;
                }

                if let (
                    Some(mic),
                    Some(mic_convert),
                    Some(mic_resample),
                    Some(mic_rate),
                    Some(mic_caps),
                    Some(mic_queue),
                    Some(audio_mixer),
                ) = (
                    mic_src.as_ref(),
                    mic_convert.as_ref(),
                    mic_resample.as_ref(),
                    mic_rate.as_ref(),
                    mic_capsfilter.as_ref(),
                    mic_queue.as_ref(),
                    audio_mixer.as_ref(),
                ) {
                    gst::Element::link_many(&[
                        mic,
                        mic_convert,
                        mic_resample,
                        mic_rate,
                        mic_caps,
                        mic_queue,
                    ])
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "failed to link mic chain")
                    })?;

                    link_queue_to_mixer(mic_queue, audio_mixer, "mic")?;
                }

                if let (Some(audio_mixer), Some(audio_encoder), Some(aacparse), Some(audio_queue)) = (
                    audio_mixer.as_ref(),
                    audio_encoder.as_ref(),
                    aacparse.as_ref(),
                    audio_queue.as_ref(),
                ) {
                    gst::Element::link_many(&[audio_mixer, audio_encoder, aacparse, audio_queue])
                        .map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "failed to link mixer chain")
                    })?;
                }
            } else if system_audio_enabled {
                if let (
                    Some(audio_src),
                    Some(audioconvert),
                    Some(audioresample),
                    Some(audiorate),
                    Some(audio_capsfilter),
                    Some(audio_encoder),
                    Some(aacparse),
                    Some(audio_queue),
                ) = (
                    audio_src.as_ref(),
                    audioconvert.as_ref(),
                    audioresample.as_ref(),
                    audiorate.as_ref(),
                    audio_capsfilter.as_ref(),
                    audio_encoder.as_ref(),
                    aacparse.as_ref(),
                    audio_queue.as_ref(),
                ) {
                    gst::Element::link_many(&[
                        audio_src,
                        audioconvert,
                        audioresample,
                        audiorate,
                        audio_capsfilter,
                        audio_encoder,
                        aacparse,
                        audio_queue,
                    ])
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "failed to link audio chain")
                    })?;
                }
            } else if mic_enabled {
                if let (
                    Some(mic),
                    Some(mic_convert),
                    Some(mic_resample),
                    Some(mic_rate),
                    Some(mic_caps),
                    Some(audio_encoder),
                    Some(aacparse),
                    Some(audio_queue),
                ) = (
                    mic_src.as_ref(),
                    mic_convert.as_ref(),
                    mic_resample.as_ref(),
                    mic_rate.as_ref(),
                    mic_capsfilter.as_ref(),
                    audio_encoder.as_ref(),
                    aacparse.as_ref(),
                    audio_queue.as_ref(),
                ) {
                    gst::Element::link_many(&[
                        mic,
                        mic_convert,
                        mic_resample,
                        mic_rate,
                        mic_caps,
                        audio_encoder,
                        aacparse,
                        audio_queue,
                    ])
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "failed to link mic chain")
                    })?;
                }
            }
        }

        link_queue_to_mux(&video_queue, &mux, "video")?;
        if let Some(audio_queue) = audio_queue.as_ref() {
            link_queue_to_mux(audio_queue, &mux, "audio")?;
        }

        mux.link(&appsink)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link mux to appsink"))?;

        let keyframe_ring_buffer = ring_buffer.clone();
        let h264parse_src = h264parse
            .static_pad("src")
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing h264parse src pad"))?;

        let guard = callback_guard.clone();

        let probe_id = h264parse_src
            .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if guard.load(Ordering::SeqCst) {
                return gst::PadProbeReturn::Remove;
            }

            if let Some(buffer) = info.buffer() {
                if !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT) {
                    if let Some(pts) = buffer.dts_or_pts() {
                        let mut guard = keyframe_ring_buffer.lock().unwrap();
                        guard.push_keyframe_pts(pts.mseconds());
                    }
                }
            }

            gst::PadProbeReturn::Ok
        })
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to attach probe"))?;

        let started_at = Instant::now();
        let ring_buffer_clone = ring_buffer.clone();

        let callback_guard_clone = callback_guard.clone();

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    if callback_guard_clone.load(Ordering::SeqCst) {
                        return Ok(gst::FlowSuccess::Ok);
                    }

                    let sample = match sink.pull_sample() {
                        Ok(sample) => sample,
                        Err(_) => return Err(gst::FlowError::Error),
                    };

                    let buffer = match sample.buffer() {
                        Some(buffer) => buffer,
                        None => return Ok(gst::FlowSuccess::Ok),
                    };

                    let pts_ms = buffer
                        .dts_or_pts()
                        .map(|pts| pts.mseconds())
                        .unwrap_or_else(|| started_at.elapsed().as_millis() as u64);

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let data = map.as_slice().to_vec();

                    if let Ok(mut rb) = ring_buffer_clone.lock() {
                        rb.push(Packet { pts_ms, data });
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        let state_change = pipeline.set_state(gst::State::Playing);
        if state_change.is_err() {
            set_state(
                &state,
                CaptureState::Failed("failed to start GStreamer pipeline".to_string()),
            );
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to start GStreamer pipeline",
            ));
        }

        set_state(&state, CaptureState::Running);

        let bus_thread = spawn_bus_thread(
            &pipeline,
            state.clone(),
            stop_flag.clone(),
            callback_guard.clone(),
        )?;

        Ok(Self {
            pipeline,
            appsink: appsink.clone(),
            state,
            stop_flag,
            bus_thread: Some(bus_thread),
            callback_guard,
            h264_probe: Some((h264parse_src, probe_id)),
        })
    }

    pub fn is_running(&self) -> bool {
        matches!(*self.state.lock().unwrap(), CaptureState::Running)
    }

    pub fn state(&self) -> CaptureState {
        self.state.lock().unwrap().clone()
    }

    pub fn stop(&mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        let should_stop = {
            let guard = self.state.lock().unwrap();
            !matches!(*guard, CaptureState::Stopped)
        };

        if !should_stop {
            return;
        }

        logger::info("capture", "stopping pipeline");

        // 1) Stop callbacks FIRST
        self.appsink
            .set_callbacks(gst_app::AppSinkCallbacks::builder().build());
        self.appsink.set_property("emit-signals", &false);
        self.callback_guard.store(true, Ordering::SeqCst);

        if let Some((pad, probe_id)) = self.h264_probe.take() {
            pad.remove_probe(probe_id);
        }

        // 2) Stop bus thread
        self.stop_flag.store(true, Ordering::SeqCst);

        // 3) Stop pipeline
        let _ = self.pipeline.set_state(gst::State::Null);

        // 4) Join bus thread
        if let Some(handle) = self.bus_thread.take() {
            let _ = handle.join();
        }

        set_state(&self.state, CaptureState::Stopped);
    }
}

impl Drop for GstCapture {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

fn make_element(name: &str) -> io::Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, format!("missing element {}", name)))
}

fn make_video_encoder(
    encoder_id: &str,
    framerate: u32,
    bitrate_kbps: u32,
) -> io::Result<gst::Element> {
    let enc = gst::ElementFactory::make(encoder_id).build().map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("missing encoder {}", encoder_id),
        )
    })?;

    let gop_size: u32 = framerate.max(1);

    set_u32_property(&enc, "bitrate", bitrate_kbps);
    set_u32_property(&enc, "gop-size", gop_size);
    set_u32_property(&enc, "key-int-max", gop_size);
    set_bool_property(&enc, "zero-latency", true);
    set_bool_property(&enc, "insert-sps-pps", true);

    Ok(enc)
}

fn make_audio_encoder() -> io::Result<gst::Element> {
    if let Ok(enc) = gst::ElementFactory::make("voaacenc").build() {
        set_str_property(&enc, "bitrate", "192000");
        return Ok(enc);
    }

    if let Ok(enc) = gst::ElementFactory::make("avenc_aac").build() {
        set_str_property(&enc, "bitrate", "192000");
        return Ok(enc);
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        "missing AAC encoder (voaacenc or avenc_aac)",
    ))
}

fn make_queue() -> io::Result<gst::Element> {
    let queue = make_element("queue")?;
    set_u32_property(&queue, "max-size-buffers", 0);
    set_u32_property(&queue, "max-size-bytes", 0);
    set_u64_property(&queue, "max-size-time", 1_000_000_000);
    Ok(queue)
}

fn link_queue_to_mux(queue: &gst::Element, mux: &gst::Element, label: &str) -> io::Result<()> {
    let queue_src = queue
        .static_pad("src")
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing queue src pad"))?;
    let mux_sink = mux
        .request_pad_simple("sink_%d")
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing mux sink pad"))?;

    queue_src.link(&mux_sink).map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to link {} to mux", label),
        )
    })?;

    Ok(())
}

fn link_queue_to_mixer(queue: &gst::Element, mixer: &gst::Element, label: &str) -> io::Result<()> {
    let queue_src = queue
        .static_pad("src")
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing queue src pad"))?;
    let mixer_sink = mixer
        .request_pad_simple("sink_%u")
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing mixer sink pad"))?;

    queue_src.link(&mixer_sink).map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to link {} to mixer", label),
        )
    })?;

    Ok(())
}

fn validate_config(config: &UserSettings) -> io::Result<()> {
    if let Some(mic_id) = &config.mic_device_id {
        if mic_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "microphone device id is empty",
            ));
        }
    }

    Ok(())
}

fn make_video_src(device_id: &str) -> io::Result<gst::Element> {
    #[cfg(target_os = "windows")]
    let src = {
        let src = make_element("d3d11screencapturesrc")?;
        set_bool_property(&src, "do-timestamp", true);
        if let Some(monitor) = monitor_index_from_id(device_id) {
            set_i32_property(&src, "monitor-index", monitor);
        }
        src
    };

    #[cfg(target_os = "macos")]
    let src = {
        let src = make_element("avfvideosrc")?;
        // Screen capture with an optional display id if the element supports it.
        set_bool_property(&src, "capture-screen", true);
        set_bool_property(&src, "do-timestamp", true);
        if let Some(display_id) = monitor_index_from_id(device_id).map(|id| id.max(0) as u32) {
            set_u32_property(&src, "display-id", display_id);
            set_u32_property(&src, "screen-index", display_id);
            set_u32_property(&src, "screen", display_id);
        }
        src
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let src = {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "video capture is not supported on this platform",
        ));
    };

    Ok(src)
}

fn make_audio_src(loopback: bool, device_id: Option<&str>) -> io::Result<gst::Element> {
    #[cfg(target_os = "windows")]
    let src = {
        let src = make_element("wasapisrc")?;
        set_bool_property(&src, "loopback", loopback);
        set_bool_property(&src, "do-timestamp", true);
        set_bool_property(&src, "provide-clock", loopback);
        set_bool_property(&src, "low-latency", false);
        src
    };

    #[cfg(target_os = "macos")]
    let src = {
        let src = make_element("osxaudiosrc")?;
        // macOS has no system-audio loopback here; reuse the input device.
        let _ = loopback;
        set_bool_property(&src, "do-timestamp", true);
        src
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let src = {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "audio capture is not supported on this platform",
        ));
    };

    if let Some(id) = device_id {
        set_str_property(&src, "device", id);
        set_str_property(&src, "device-id", id);
    }

    Ok(src)
}

fn set_state(state: &Arc<Mutex<CaptureState>>, new_state: CaptureState) {
    let mut guard = state.lock().unwrap();
    logger::info("capture", format!("state: {:?} -> {:?}", *guard, new_state));
    *guard = new_state;
}

fn spawn_bus_thread(
    pipeline: &gst::Pipeline,
    state: Arc<Mutex<CaptureState>>,
    stop_flag: Arc<AtomicBool>,
    callback_guard: Arc<AtomicBool>,
) -> io::Result<std::thread::JoinHandle<()>> {
    let bus = pipeline
        .bus()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing pipeline bus"))?;
    let pipeline_name = pipeline.name();

    let handle = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::SeqCst) {
            let message = bus.timed_pop(gst::ClockTime::from_mseconds(200));
            let Some(message) = message else { continue };

            match message.view() {
                gst::MessageView::Error(err) => {
                    callback_guard.store(true, Ordering::SeqCst);

                    let src = err
                        .src()
                        .map(|s| s.path_string())
                        .unwrap_or_else(|| "unknown".to_string().into());
                    logger::error("gst", format!("error from {}: {}", src, err.error()));
                    if let Some(debug) = err.debug() {
                        logger::debug("gst", format!("debug: {}", debug));
                    }
                    set_state(&state, CaptureState::Failed(err.error().to_string()));
                    stop_flag.store(true, Ordering::SeqCst);
                    break;
                }
                gst::MessageView::Warning(warn) => {
                    let src = warn
                        .src()
                        .map(|s| s.path_string())
                        .unwrap_or_else(|| "unknown".to_string().into());
                    logger::warn("gst", format!("warning from {}: {}", src, warn.error()));
                    if let Some(debug) = warn.debug() {
                        logger::debug("gst", format!("debug: {}", debug));
                    }
                }
                gst::MessageView::StateChanged(state) => {
                    if message
                        .src()
                        .map(|s| s.name() == pipeline_name)
                        .unwrap_or(false)
                    {
                        logger::info(
                            "gst",
                            format!("pipeline state: {:?} -> {:?}", state.old(), state.current()),
                        );
                    }
                }
                gst::MessageView::Eos(..) => {
                    logger::error("gst", "eos");
                    set_state(&state, CaptureState::Failed("eos".to_string()));
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(handle)
}

fn monitor_index_from_id(id: &str) -> Option<i32> {
    let mut parts = id.split(':');
    let kind = parts.next()?;
    let index = parts.next()?;
    if kind != "screen" {
        return None;
    }

    index.parse::<i32>().ok()
}

fn set_i32_property(element: &gst::Element, name: &str, value: i32) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}

fn set_u32_property(element: &gst::Element, name: &str, value: u32) {
    let Some(spec) = element.find_property(name) else {
        return;
    };

    let ty = spec.value_type();

    if ty == glib::Type::U32 {
        element.set_property(name, &value);
    } else if ty == glib::Type::I32 {
        let v = value.min(i32::MAX as u32) as i32;
        element.set_property(name, &v);
    } else if ty == glib::Type::U64 {
        element.set_property(name, &(value as u64));
    } else if ty == glib::Type::I64 {
        element.set_property(name, &(value as i64));
    }
}

fn set_u64_property(element: &gst::Element, name: &str, value: u64) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}

fn set_bool_property(element: &gst::Element, name: &str, value: bool) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}

fn set_str_property(element: &gst::Element, name: &str, value: &str) {
    if element.find_property(name).is_some() {
        element.set_property_from_str(name, value);
    }
}
