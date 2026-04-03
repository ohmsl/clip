use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;

use crossbeam_channel::Sender;

use crate::audio::{AudioGraph, AudioSourceId};
use crate::video::VideoGraph;

use crate::{
    logger,
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
    // gstreamer
    pipeline: gst::Pipeline,
    appsink: gst_app::AppSink,

    // state / lifecycle
    state: Arc<Mutex<CaptureState>>,
    stop_flag: Arc<AtomicBool>,
    callback_guard: Arc<AtomicBool>,

    // threads
    bus_thread: Option<std::thread::JoinHandle<()>>,
    worker_thread: Option<std::thread::JoinHandle<()>>,

    // packet pipeline
    packet_tx: Option<Sender<Packet>>,

    // audio controls
    system_volume: Option<gst::Element>,
    mic_volume: Option<gst::Element>,
    audio_caps: Vec<(AudioSourceId, gst::Element)>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioCapsSummary {
    pub rate: Option<i32>,
    pub channels: Option<i32>,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AudioCapsState {
    pub system: Option<AudioCapsSummary>,
    pub mic: Option<AudioCapsSummary>,
}

impl GstCapture {
    pub fn start(config: &UserSettings, ring_buffer: Arc<Mutex<RingBuffer>>) -> io::Result<Self> {
        gst::init().map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        logger::info("audio", "capture attempt: configured sources");
        Self::start_with_config(config, ring_buffer)
    }

    fn start_with_config(
        config: &UserSettings,
        ring_buffer: Arc<Mutex<RingBuffer>>,
    ) -> io::Result<Self> {
        let callback_guard = Arc::new(AtomicBool::new(false));

        validate_config(config)?;

        let pipeline = gst::Pipeline::new();
        let state = Arc::new(Mutex::new(CaptureState::Starting));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let mux = make_element("mpegtsmux")?;
        set_bool_property(&mux, "streamable", true);

        pipeline
            .add(&mux)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to add mux"))?;

        let video = VideoGraph::build(&pipeline, config)?;
        let audio = AudioGraph::build(&pipeline, config)?;

        link_queue_to_mux(&video.output.element, &mux, "video")?;

        let (system_volume, mic_volume) = match audio.as_ref() {
            Some(graph) => (graph.volumes.system.clone(), graph.volumes.mic.clone()),
            None => (None, None),
        };

        let audio_caps = match audio.as_ref() {
            Some(graph) => graph.capsfilters.clone(),
            None => Vec::new(),
        };

        if let Some(audio) = audio.as_ref() {
            if let Some(src_pad) = audio.output.element.static_pad("src") {
                let caps = src_pad.current_caps();
                logger::info("audio", format!("audio caps before mux: {:?}", caps));
            } else {
                logger::warn("audio", "audio output has no src pad");
            }
            link_queue_to_mux(&audio.output.element, &mux, "audio")?;
        }

        let appsink = make_element("appsink")?;
        let appsink = appsink
            .downcast::<gst_app::AppSink>()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "appsink downcast failed"))?;

        appsink.set_property("emit-signals", &true);
        appsink.set_property("sync", &false);
        appsink.set_property("async", &false);
        appsink.set_property("drop", &true);
        appsink.set_property("max-buffers", &4u32);

        pipeline
            .add(&appsink)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to add appsink"))?;

        mux.link(&appsink)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to link mux to appsink"))?;

        video.attach_keyframe_tracker(ring_buffer.clone())?;

        let (packet_tx, packet_rx) = crossbeam_channel::bounded::<Packet>(1024);
        let tx = packet_tx.clone();

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;

                    let pts_ms = buffer.dts_or_pts().map(|pts| pts.mseconds()).unwrap_or(0);

                    if let Ok(map) = buffer.map_readable() {
                        let packet = Packet {
                            pts_ms,
                            data: map.as_slice().to_vec(),
                        };

                        // Non blocking send, if full: drop.
                        let _ = tx.try_send(packet);
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        let state_change = pipeline.set_state(gst::State::Playing);
        if let Err(err) = state_change {
            if let Some(message) = collect_startup_error(&pipeline) {
                logger::error("gst", message.clone());
                set_pipeline_to_null_and_wait(&pipeline);
                set_state(&state, CaptureState::Failed(message.clone()));
                return Err(io::Error::new(io::ErrorKind::Other, message));
            }
            set_pipeline_to_null_and_wait(&pipeline);
            set_state(
                &state,
                CaptureState::Failed("failed to start GStreamer pipeline".to_string()),
            );
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed to start GStreamer pipeline: {err:?}"),
            ));
        }

        if let Some(audio) = audio.as_ref() {
            audio.log_negotiated_caps();
        }

        // Worker thread owns the ring buffer
        let ring_buffer_clone = ring_buffer.clone();
        let stop_flag_clone = stop_flag.clone();

        let worker_thread = std::thread::spawn(move || {
            while !stop_flag_clone.load(Ordering::SeqCst) {
                match packet_rx.recv() {
                    Ok(packet) => {
                        if let Ok(mut rb) = ring_buffer_clone.lock() {
                            rb.push(packet);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

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
            callback_guard,

            bus_thread: Some(bus_thread),
            worker_thread: Some(worker_thread),

            packet_tx: Some(packet_tx),

            system_volume,
            mic_volume,
            audio_caps,
        })
    }

    pub fn is_running(&self) -> bool {
        matches!(*self.state.lock().unwrap(), CaptureState::Running)
    }

    pub fn audio_caps(&self) -> AudioCapsState {
        let mut state = AudioCapsState::default();
        for (source_id, capsfilter) in &self.audio_caps {
            let caps = capsfilter.property::<gst::Caps>("caps");
            let summary = caps_to_summary(&caps);
            match source_id {
                AudioSourceId::System => state.system = Some(summary),
                AudioSourceId::Mic => state.mic = Some(summary),
            }
        }
        state
    }

    pub fn state(&self) -> CaptureState {
        self.state.lock().unwrap().clone()
    }

    pub fn stop(&mut self) {
        self.stop_inner();
    }

    pub fn volume_element(&self, source: AudioSourceId) -> Option<gst::Element> {
        match source {
            AudioSourceId::System => self.system_volume.clone(),
            AudioSourceId::Mic => self.mic_volume.clone(),
        }
    }

    pub fn set_volume(&self, source: AudioSourceId, value: f32) -> bool {
        if let Some(element) = self.volume_element(source) {
            let value = value as f64;
            element.set_property("volume", &value);
            return true;
        }
        false
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

        // 1) Prevent further appsink callbacks
        self.callback_guard.store(true, Ordering::SeqCst);
        self.appsink
            .set_callbacks(gst_app::AppSinkCallbacks::builder().build());
        self.appsink.set_property("emit-signals", &false);

        // 2) Signal threads to stop
        self.stop_flag.store(true, Ordering::SeqCst);

        // 3) Drop sender unblocks worker thread
        if let Some(sender) = self.packet_tx.take() {
            drop(sender);
        }

        // 4) Flush pipeline
        let _ = self.pipeline.send_event(gst::event::Eos::new());
        set_pipeline_to_null_and_wait(&self.pipeline);

        // 5) Join worker thread
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }

        // 6) Join bus thread
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

fn caps_to_summary(caps: &gst::Caps) -> AudioCapsSummary {
    let mut rate = None;
    let mut channels = None;
    if let Some(structure) = caps.structure(0) {
        rate = structure.get::<i32>("rate").ok();
        channels = structure.get::<i32>("channels").ok();
    }
    AudioCapsSummary {
        rate,
        channels,
        raw: Some(caps.to_string()),
    }
}

fn make_element(name: &str) -> io::Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, format!("missing element {}", name)))
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

fn set_state(state: &Arc<Mutex<CaptureState>>, new_state: CaptureState) {
    let mut guard = state.lock().unwrap();
    logger::info("capture", format!("state: {:?} -> {:?}", *guard, new_state));
    *guard = new_state;
}

fn collect_startup_error(pipeline: &gst::Pipeline) -> Option<String> {
    let bus = pipeline.bus()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

    while std::time::Instant::now() < deadline {
        let message = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(200),
            &[gst::MessageType::Error],
        );
        let Some(message) = message else { continue };

        if let gst::MessageView::Error(err) = message.view() {
            let src = err
                .src()
                .map(|s| s.path_string())
                .unwrap_or_else(|| "unknown".to_string().into());
            let debug = err.debug().unwrap_or_default();
            return Some(format!(
                "startup error from {}: {} {}",
                src,
                err.error(),
                if debug.is_empty() {
                    "".to_string()
                } else {
                    format!("(debug: {})", debug)
                }
            ));
        }
    }

    None
}

fn set_pipeline_to_null_and_wait(pipeline: &gst::Pipeline) {
    let _ = pipeline.set_state(gst::State::Null);
    let _ = pipeline.state(gst::ClockTime::from_seconds(2));
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

fn set_bool_property(element: &gst::Element, name: &str, value: bool) {
    if element.find_property(name).is_some() {
        element.set_property(name, &value);
    }
}
