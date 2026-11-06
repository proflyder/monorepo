//! The firmware GStreamer backend of the player: one playbin per channel, the GLib thread that
//! owns the default main context, and the process-wide one-time GStreamer initialisation.

use std::ffi::{CStr, CString, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use pulsar_model::Playback;
use pulsar_playback::{Output, OutputTuning};
use tracing::error;

use crate::PlayerInitError;
use crate::core::{Backend, Channel, Exec, GstState, Machine, Shared, Timer};
use crate::gst::Call;
use crate::gst::{self, Api, MainLoop, Pipeline};

/// The firmware's GStreamer: one playbin per channel.
struct GstBackend {
    api: &'static Api,
    playback: &'static Playback,
    // Dropped before `out`: the pipelines are in NULL when the output closes the fdsink's fd.
    pipes: [Pipeline; 2],
    out: Output,
}

impl Backend for GstBackend {
    fn set_state(&mut self, ch: Channel, s: GstState) -> bool {
        // MUSIC's alsasink opens the device asynchronously after this call, and dmix may switch
        // the amplifier off when another client closes meanwhile: the gates go on here and again
        // in `music_playing`. ANNOUNCE is not played by GStreamer (the output opens the PCM).
        if s == GstState::Playing && ch == Channel::Music {
            pulsar_alsa::enable_output(&crate::output_gates(self.playback));
        }
        self.pipes[ch.index()].set_state(s as i32)
    }

    fn music_playing(&mut self) {
        pulsar_alsa::enable_output(&crate::output_gates(self.playback));
    }

    fn set_uri(&mut self, ch: Channel, uri: &str) {
        // As a C string: up to the first NUL.
        if let Ok(uri) = CString::new(uri.split('\0').next().unwrap_or_default()) {
            self.pipes[ch.index()].set_uri(&uri);
        }
    }

    fn set_volume(&mut self, ch: Channel, v: f64) {
        match ch {
            Channel::Music => self.pipes[ch.index()].set_volume(v),
            // ANNOUNCE's playbin stays at 1.0: its output applies the gain.
            Channel::Announce => self.out.set_gain(v),
        }
    }

    fn query_position(&mut self, ch: Channel) -> Option<i64> {
        self.pipes[ch.index()].query_position()
    }

    fn can_seek(&self) -> bool {
        self.pipes[0].can_seek()
    }

    fn seek(&mut self, ch: Channel, pos_ns: i64) -> bool {
        self.pipes[ch.index()].seek(pos_ns)
    }

    fn resource_error_quark(&self) -> u32 {
        gst::resource_error_quark(self.api)
    }

    fn timeout_add(&mut self, ms: u32, t: Timer) -> u32 {
        gst::timeout_add(self.api, ms, t.tag())
    }

    fn source_remove(&mut self, id: u32) {
        gst::source_remove(self.api, id);
    }

    fn now_ms(&self) -> i64 {
        pulsar_core::now_ms()
    }

    fn element(&self, ch: Channel) -> *const c_void {
        self.pipes[ch.index()].addr()
    }

    fn out_begin(&mut self, item: u64) -> bool {
        let Some(fd) = self.out.begin(item) else {
            return false;
        };
        self.pipes[Channel::Announce.index()].set_out_fd(fd);
        true
    }

    fn out_end(&mut self) {
        self.out.end();
    }

    fn out_flush(&mut self) {
        self.out.flush();
    }
}

// The fdsink of ANNOUNCE's output; playsink converts to its caps.
const OUT_SINK: &CStr = c"pcmout";

fn new_pipeline(
    api: &'static Api,
    ch: Channel,
    playback: &'static Playback,
) -> Result<Pipeline, PlayerInitError> {
    let sink = match ch {
        Channel::Music => format!("alsasink device={}", playback.pcm),
        Channel::Announce => format!(
            "capsfilter caps=\\\"{}\\\" ! fdsink name={} sync=false",
            pulsar_playback::CAPS,
            OUT_SINK.to_string_lossy()
        ),
    };
    let desc = format!("playbin name=ch{} audio-sink=\"{sink}\"", ch.index());
    let mut p = Pipeline::launch(
        api,
        &CString::new(desc).map_err(|_| PlayerInitError::Pipeline)?,
    )
    .ok_or(PlayerInitError::Pipeline)?;
    if ch == Channel::Announce && !p.find_out_sink(OUT_SINK) {
        return Err(PlayerInitError::NoOutputSink(ch.index()));
    }
    p.set_flags(crate::core::PLAY_FLAGS);
    p.set_volume(1.0);
    p.add_bus_watch(ch.index());
    Ok(p)
}

/// Process-wide GStreamer initialisation: dlopen + gst_init + environment, once. The running
/// player (GLib thread, pipelines, ANNOUNCE output) belongs to the handle and stops with it, but
/// the slot is never released: as before, a process starts at most one player in its lifetime.
struct Process {
    api: &'static Api,
    running: AtomicBool,
}

fn process() -> Result<&'static Process, PlayerInitError> {
    static PROCESS: OnceLock<Result<Process, PlayerInitError>> = OnceLock::new();
    match PROCESS.get_or_init(|| {
        let api = gst::load().ok_or(PlayerInitError::Load)?;
        gst::set_ssl_cert_file();
        // Process-global: alsa-lib messages go to the debug log.
        pulsar_alsa::quiet_error_handler();
        if !gst::init(api) {
            return Err(PlayerInitError::GstInit);
        }
        Ok(Process {
            api,
            running: AtomicBool::new(false),
        })
    }) {
        Ok(p) => Ok(p),
        Err(e) => Err(e.clone()),
    }
}

/// Claims the one player slot of the process. It is never released: a failed or finished player
/// still means "already started", exactly like the previous static flag.
fn claim(process: &'static Process) -> Result<(), PlayerInitError> {
    if process.running.swap(true, Ordering::AcqRel) {
        return Err(PlayerInitError::AlreadyStarted);
    }
    Ok(())
}

/// The GLib thread with the machine of this player; stopping it (drop) quits the main loop and
/// joins the thread, whose exit tears the machine down and closes every output.
pub struct PlayerThread {
    api: &'static Api,
    main_loop: Arc<MainLoop>,
    thread: Option<JoinHandle<()>>,
}

impl Exec for PlayerThread {
    fn invoke(&self, call: Call) {
        gst::idle_add(self.api, call);
    }
}

impl Drop for PlayerThread {
    fn drop(&mut self) {
        let ml = Arc::clone(&self.main_loop);
        gst::idle_add(self.api, Box::new(move || ml.quit()));
        if let Some(t) = self.thread.take()
            && t.join().is_err()
        {
            error!("player: GLib thread panicked");
        }
    }
}

/// Starts the one player of the process. The caller owns the returned thread through its handle.
pub fn start(
    shared: &Arc<Shared>,
    playback: &'static Playback,
) -> Result<PlayerThread, PlayerInitError> {
    let process = process()?;
    claim(process)?;
    let api = process.api;
    let pipes = [
        new_pipeline(api, Channel::Music, playback)?,
        new_pipeline(api, Channel::Announce, playback)?,
    ];
    let out = Output::spawn(
        playback,
        OutputTuning::ANNOUNCE,
        Box::new(move |ev| gst::idle_add(api, Box::new(move || crate::core::on_out(ev)))),
    )
    .ok_or(PlayerInitError::Output)?;
    let main_loop = Arc::new(MainLoop::new(api).ok_or_else(|| {
        error!("player: cannot create the GLib main loop");
        PlayerInitError::MainLoop
    })?);
    let machine = Machine::new(
        Box::new(GstBackend {
            api,
            playback,
            pipes,
            out,
        }),
        Arc::clone(shared),
    );
    let ml = Arc::clone(&main_loop);
    let handle = std::thread::Builder::new()
        .name("gst".into())
        .spawn(move || {
            crate::core::MACHINE.set(Some(machine));
            ml.run();
            if let Some(mut m) = crate::core::MACHINE.take() {
                m.teardown();
            }
        })
        .map_err(|e| {
            error!("player: thread: {e}");
            PlayerInitError::Thread(e.to_string())
        })?;
    Ok(PlayerThread {
        api,
        main_loop,
        thread: Some(handle),
    })
}
