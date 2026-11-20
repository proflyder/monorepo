use clap::Parser;
use std::cell::Cell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use pulsar_esphome::{
    ApiConfig, ApiEvent, ApiHandle, BinarySensorId, ButtonId, Capabilities, DeviceIdentity,
    EventId, LightState, MediaCommand, MediaState, NumberId, SelectId, SensorId, SwitchId, TextId,
    TextSensorId, WakeWordInfo, spawn,
};
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::{Instant, interval_at, sleep_until};

const BUILD_TIME: &str = match option_env!("PULSAR_BUILD_TIME") {
    Some(v) => v,
    None => "unknown",
};

fn read_mac() -> String {
    match std::fs::read_to_string("/sys/class/net/wlan0/address") {
        Ok(s) if s.trim().len() == 17 => s.trim().to_ascii_uppercase(),
        _ => "02:00:00:C0:FF:EE".to_owned(),
    }
}

fn b(v: bool) -> u8 {
    u8::from(v)
}

struct Echo {
    api: ApiHandle,
    media: MediaState,
    volume: f32,
    muted: bool,
    stopped: Rc<Cell<bool>>,
}

impl Echo {
    fn on_event(&mut self, ev: ApiEvent) -> bool {
        match ev {
            ApiEvent::ClientConnected { client_info } => println!("HOOK connected '{client_info}'"),
            ApiEvent::ClientDisconnected => println!("HOOK disconnected"),
            ApiEvent::VoiceSubscribed(sub) => {
                println!("HOOK voice_subscribed {}", b(sub));
                if sub {
                    let run = voice_run(self.api.clone(), self.stopped.clone());
                    pulsar_core::supervisor().spawn_local("selftest-voice", run);
                }
            }
            ApiEvent::VoiceEvent { event, data } => {
                let mut kv: Vec<_> = data.into_iter().collect();
                kv.sort();
                let kv: String = kv.iter().map(|(k, v)| format!(" {k}={v}")).collect();
                println!("HOOK voice_event {}{kv}", event.code());
            }
            ApiEvent::Announce(r) => {
                println!(
                    "HOOK announce media={} text='{}' pre={} start_conv={}",
                    r.media_id,
                    r.text,
                    r.preannounce_media_id,
                    b(r.start_conversation)
                );
                let (api, stopped) = (self.api.clone(), self.stopped.clone());
                pulsar_core::supervisor().spawn_local("selftest-announce", async move {
                    let at = Instant::now() + Duration::from_millis(300);
                    if wait_until(&stopped, at).await {
                        api.voice_announce_finished(true);
                    }
                });
            }
            ApiEvent::Timer(t) => println!(
                "HOOK timer event={} id={} name={} total={} left={} active={}",
                t.event as u32,
                t.id,
                t.name,
                t.total_seconds,
                t.seconds_left,
                b(t.is_active)
            ),
            ApiEvent::ActiveWakeWords(ids) => {
                println!("HOOK active_wake_words {}", ids.join(" "));
                self.api.set_active_wake_words(ids);
            }
            ApiEvent::MediaCommand(c) => {
                println!(
                    "HOOK media has_cmd={} cmd={} has_vol={} vol={:.2} has_url={} url={} has_ann={} ann={}",
                    b(c.command.is_some()),
                    c.command.map_or(0, |c| c as u32),
                    b(c.volume.is_some()),
                    c.volume.unwrap_or(0.0),
                    b(c.media_url.is_some()),
                    c.media_url.as_deref().unwrap_or(""),
                    b(c.announcement.is_some()),
                    b(c.announcement.unwrap_or(false))
                );
                if let Some(v) = c.volume {
                    self.volume = v;
                }
                match c.command {
                    Some(MediaCommand::Pause) => self.media = MediaState::Paused,
                    Some(MediaCommand::Play) => self.media = MediaState::Playing,
                    Some(MediaCommand::Stop) => self.media = MediaState::Idle,
                    Some(MediaCommand::Mute) => self.muted = true,
                    Some(MediaCommand::Unmute) => self.muted = false,
                    _ => {}
                }
                if c.media_url.is_some() {
                    self.media = MediaState::Playing;
                }
                self.api.publish_media(self.media, self.volume, self.muted);
            }
            ApiEvent::Switch(id, on) => {
                println!("HOOK switch {} {}", id as u32, b(on));
                self.api.publish_switch(id, on);
            }
            ApiEvent::Light {
                state: s,
                transition_ms,
            } => println!(
                "HOOK light on={} bri={:.2} rgb={:.2},{:.2},{:.2} effect='{}' transition={transition_ms} ms",
                b(s.on),
                s.brightness,
                s.r,
                s.g,
                s.b,
                s.effect
            ),
            ApiEvent::Number(id, v) => {
                println!("HOOK number {} {v:.1}", id as u32);
                self.api.publish_number(id, v);
            }
            ApiEvent::Text(id, v) => {
                println!("HOOK text {} '{v}'", id as u32);
                self.api.publish_text(id, &v);
            }
            ApiEvent::Select(id, o) => {
                println!("HOOK select {} '{o}'", id as u32);
                self.api.publish_select(id, &o);
            }
            ApiEvent::Action { id, value, seconds } => {
                println!("HOOK action {} '{value}' {seconds}", id as u32);
            }
            ApiEvent::LedShow {
                effect,
                color,
                seconds,
            } => println!("HOOK led_show '{effect}' '{color}' {seconds}"),
            ApiEvent::Time { epoch, posix_tz } => {
                println!("HOOK time epoch={epoch} tz='{posix_tz}'");
            }
            ApiEvent::VoiceStartFailed { .. } => println!("HOOK voice_start_failed"),
            ApiEvent::VoiceAudioOverflow { .. } => println!("HOOK voice_audio_overflow"),
            ApiEvent::Button(id) => {
                println!("HOOK button {}", id as u32);
                let status = if id == ButtonId::ZigbeeJoin {
                    "router: joining"
                } else {
                    "router: not in a network"
                };
                self.api.publish_text_sensor(TextSensorId::Zigbee, status);
            }
            ApiEvent::Restart => {
                println!("HOOK restart");
                return false;
            }
            ApiEvent::Update(_) | ApiEvent::MdnsTxtChanged(_) => {}
        }
        true
    }
}

async fn wait_until(stopped: &Rc<Cell<bool>>, at: Instant) -> bool {
    sleep_until(at).await;
    !stopped.get()
}

async fn voice_run(api: ApiHandle, stopped: Rc<Cell<bool>>) {
    if !wait_until(&stopped, Instant::now() + Duration::from_millis(500)).await {
        return;
    }
    println!("TEST voice_request_start");
    api.voice_request_start("Алиса");
    let t0 = Instant::now();
    for i in 0..20u32 {
        if !wait_until(&stopped, t0 + Duration::from_millis(50 + u64::from(i) * 30)).await {
            return;
        }
        let pcm = (0..512u32)
            .map(|k| (1000.0 * (f64::from(i * 512 + k) * 0.1).sin()) as i16)
            .collect();
        api.voice_send_audio(pcm);
    }
    if !wait_until(&stopped, t0 + Duration::from_millis(50 + 20 * 30 + 100)).await {
        return;
    }
    println!("TEST voice_request_stop");
    api.voice_request_stop();
}

#[derive(Parser)]
#[command(about = "ESPHome API self-test server for a real client (no mDNS)")]
struct Cli {
    #[arg(default_value_t = 16053)]
    port: u16,
    #[arg(default_value = "pulsar-selftest")]
    name: String,
    #[arg(default_value_t = 120)]
    run_seconds: i64,
    key_file: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (port, name, run_s, key_path) = (cli.port, cli.name, cli.run_seconds, cli.key_file);
    pulsar_core::init_logger();
    pulsar_core::add_own_target(module_path!());
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };
    let local = tokio::task::LocalSet::new();
    let code = local.block_on(&rt, run(port, name, run_s, key_path));
    local.block_on(
        &rt,
        pulsar_core::supervisor().shutdown(Duration::from_secs(3)),
    );
    {
        let _rt = rt.enter();
        drop(local);
    }
    rt.shutdown_timeout(Duration::from_secs(1));
    code
}

async fn run(port: u16, name: String, run_s: i64, key_path: Option<PathBuf>) -> ExitCode {
    let id = DeviceIdentity {
        name,
        friendly_name: "Яндекс Станция Миди (selftest)".into(),
        mac: read_mac(),
        model: "Yandex Station Midi".into(),
        board: "cucumber".into(),
        manufacturer: "Yandex".into(),
        project_version: "0.1.0".into(),
        compilation_time: BUILD_TIME.into(),
    };
    let ww = |id: &str, phrase: &str| WakeWordInfo {
        id: id.into(),
        phrase: phrase.into(),
        languages: vec!["ru".into()],
    };
    let strings = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
    let cfg = ApiConfig {
        port,
        wake_words: vec![ww("alisa", "Алиса"), ww("yandex", "Яндекс")],
        active_wake_words: strings(&["alisa"]),
        light_effects: strings(&["Rainbow", "Pulse"]),
        clock_faces: strings(&["None", "Smiley"]),
        music_styles: strings(&["Off", "All styles", "Spectrum"]),
        case_colors: strings(&["None", "blue"]),
        clock_music_styles: strings(&["Off", "Levels"]),
        voice_colors: strings(&["Violet", "Blue"]),
        zigbee_modes: strings(&["Coordinator", "Router"]),
        capabilities: Capabilities {
            clock: true,
            lux: true,
            auto_brightness: true,
            wake_preroll: true,
            hw_mic_mute: true,
            buttons: true,
            case_colors: true,
            voice_colors: true,
            zigbee: true,
        },
        key_path,
    };
    let Ok((api, mut events)) = spawn(id, cfg) else {
        return ExitCode::from(1);
    };
    api.publish_media(MediaState::Idle, 0.5, false);
    api.publish_switch(SwitchId::Microphones, true);
    api.publish_switch(SwitchId::Clock, true);
    api.publish_switch(SwitchId::VoiceIndication, true);
    api.publish_number(NumberId::LedSpeed, 1.0);
    api.publish_select(SelectId::MusicViz, "All styles");
    api.publish_select(SelectId::CaseColor, "None");
    api.publish_select(SelectId::ClockMusic, "Off");
    api.publish_select(SelectId::VoiceColor, "Violet");
    api.publish_binary_sensor(BinarySensorId::HwMicMute, false);
    api.publish_number(NumberId::ClockBrightness, 50.0);
    api.publish_light(&LightState {
        on: true,
        brightness: 0.8,
        r: 0.0,
        g: 0.5,
        b: 1.0,
        effect: String::new(),
    });
    api.publish_number(NumberId::ClockScroll, 400.0);
    api.publish_text(TextId::ClockText, "");
    api.publish_select(SelectId::ClockFace, "None");
    api.publish_select(SelectId::ZigbeeMode, "Coordinator");
    api.publish_text_sensor(TextSensorId::Zigbee, "coordinator: PAN 0x1234, channel 15");

    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        eprintln!("signals unavailable");
        return ExitCode::from(1);
    };
    let deadline = Instant::now() + Duration::from_secs(run_s.max(0) as u64);
    let mut every_2s = interval_at(
        Instant::now() + Duration::from_secs(2),
        Duration::from_secs(2),
    );
    let mut lux = 100.0f32;
    let stopped = Rc::new(Cell::new(false));
    let mut echo = Echo {
        api: api.clone(),
        media: MediaState::Idle,
        volume: 0.5,
        muted: false,
        stopped: stopped.clone(),
    };
    loop {
        tokio::select! {
            ev = events.recv() => {
                if !ev.is_some_and(|ev| echo.on_event(ev)) {
                    break;
                }
            }
            _ = every_2s.tick() => {
                if api.connected() {
                    lux += 1.5;
                    api.publish_sensor(SensorId::Illuminance, lux);
                    api.fire_event(EventId::Button, "action");
                }
            }
            _ = term.recv() => break,
            _ = int.recv() => break,
            () = sleep_until(deadline), if run_s > 0 => break,
        }
    }
    stopped.set(true);
    println!("TEST exiting");
    ExitCode::SUCCESS
}
