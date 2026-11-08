//! Сборка контроллера: загрузка настроек, порядок спавна сервисов и сборка `Deps`.
//!
//! Порядок старта — это порядок вызовов шагов в [`wire`], а не комментарий: каждый шаг
//! возвращает уже созданный сервис (типизированный результат), и следующий шаг не может
//! выполниться раньше. `pulsar_alsa::init` и `ALSA_CONFIG_PATH` — в [`prepare_process`], до
//! рантайма и любых потоков. `SSL_CERT_FILE` ставит `pulsar_media::spawn` (`set_var`), поэтому
//! плеер поднимается до любых других потоков — вторым шагом после bind'а API (актор API живёт в
//! `LocalSet` и потоков не создаёт).

use std::path::{Path, PathBuf};

use pulsar_arbiter::{ArbiterEvent, ArbiterHandle};
use pulsar_audio::{FrontendEvent, FrontendHandle};
use pulsar_clock::ClockHandle;
use pulsar_esphome::{ApiConfig, ApiEvent, ApiHandle, DeviceIdentity, WakeWordInfo};
use pulsar_hw::{InputEvent, InputHandle, LedsHandle};
use pulsar_mdns::{MdnsHandle, MdnsService};
use pulsar_media::{Mixer, PlayerEvent, PlayerHandle};
use pulsar_model::{DeviceProfile, ZigbeeRadio};
use pulsar_sendspin::{SendspinConfig, SendspinEvent, SendspinHandle};
use pulsar_update::{UpdateEvent, UpdaterHandle};
use pulsar_zigbee::{ZigbeeConfig, ZigbeeEvent, ZigbeeHandle};
use thiserror::Error;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{debug, error, info, warn};

use super::logic::{self, Ports};
use super::ports::{
    Arbiter, Clock, Display, Frontend, HaLink, Leds, Mdns, Mixer as MixerPort, Player, Radio,
    Sendspin, Updater,
};
use super::{RICARDO_MODEL, Rx, WAKE_EVENTS_DIR, media, model_dir};
use crate::settings::Store;
use crate::version::{BUILD_TIME, VERSION};

/// Намеренная конфигурация форка: станция — только ESPHome/Sendspin-эндпойнт, радио Яндекса не
/// трогаем (координатор Zigbee живёт в Home Assistant). Единственное место, где это решается.
pub struct Features {
    pub zigbee: bool,
}

pub const FEATURES: Features = Features { zigbee: false };

/// Why the controller could not be built; `main` logs it and exits 1.
#[derive(Debug, Error)]
pub enum StartError {
    #[error("signals: {0}")]
    Signals(#[source] std::io::Error),
    #[error("invalid PULSAR_PORT_OFFSET")]
    Ports,
    #[error("ESPHome API server failed to start")]
    Api(#[source] pulsar_esphome::SpawnError),
}

/// Профиль до рантайма: ALSA-окружение и загрузка libasound до потоков и библиотек.
pub fn prepare_process(model: &DeviceProfile) {
    model.audio.playback.apply_env();
    pulsar_alsa::init(model.audio.playback.alsa_lib());
}

/// Zigbee-план, посчитанный один раз: роль есть, только если её включили в [`FEATURES`].
struct ZigbeePlan {
    radio: &'static ZigbeeRadio,
    labels: Vec<String>,
}

impl ZigbeePlan {
    fn new(model: &'static DeviceProfile) -> Option<Self> {
        // A constant check on the spawn path: with Zigbee off the compiler drops the whole
        // `pulsar_zigbee`/`deconz`/`zbhci` call graph from the binary (it was ~5 KB of symbols
        // in HEAD, and an `Option` in a field alone does not let LLVM prove it unreachable).
        if !FEATURES.zigbee {
            return None;
        }
        let radio = model.zigbee.as_ref()?;
        Some(Self {
            radio,
            labels: pulsar_zigbee::ROLES
                .iter()
                .map(|r| r.label.to_owned())
                .collect(),
        })
    }
}

fn read_line(path: &str) -> String {
    std::fs::read(path)
        .map(|b| {
            let line = b.split(|&c| c == b'\n').next().unwrap_or_default();
            String::from_utf8_lossy(line).into_owned()
        })
        .unwrap_or_default()
}

/// Всё, что нужно собрать до первого сервиса.
struct Boot {
    model: &'static DeviceProfile,
    ports: Ports,
    store: Store,
    applied_tz: String,
    dir: PathBuf,
    id: DeviceIdentity,
    case_colors: Vec<String>,
    voice_colors: Vec<String>,
    zigbee: Option<ZigbeePlan>,
    /// Настройки, исправленные при старте, надо записать (`Store` через 1 с).
    changed: bool,
    media: media::Policy,
    term: Signal,
    int: Signal,
}

impl Boot {
    /// Signals, ports, settings, TZ, identity. Any failure here exits before services start.
    fn load(model: &'static DeviceProfile, state_path: PathBuf) -> Result<Self, StartError> {
        // Before anything starts: a SIGTERM during the start-up still ends the daemon cleanly.
        let term = signal(SignalKind::terminate()).map_err(StartError::Signals)?;
        let int = signal(SignalKind::interrupt()).map_err(StartError::Signals)?;
        let ports = logic::ports(std::env::var_os("PULSAR_PORT_OFFSET").as_deref())
            .ok_or(StartError::Ports)?;
        let dir = logic::state_dir(state_path.as_os_str());
        let mut store = Store::load(state_path);
        let mut applied_tz = String::new();
        if !store.tz.is_empty() {
            // The clock shows local time before HA connects.
            pulsar_core::tz::set_tz(&store.tz);
            applied_tz.clone_from(&store.tz);
        }
        // After the TZ: the log's timestamps are local time.
        if ports.offset != 0 {
            info!(
                "PULSAR_PORT_OFFSET={}: API {}, Sendspin {}, deCONZ {}, arbiter {}, no mDNS",
                ports.offset, ports.api, ports.sendspin, ports.deconz, ports.arbiter
            );
        }
        let id = logic::identity(
            &model.identity,
            &read_line("/sys/class/net/wlan0/address"),
            VERSION,
            BUILD_TIME,
        );
        if store.wake_word != "alisa" && store.wake_word != "yandex" && store.wake_word != "ricardo"
        {
            store.wake_word = "ricardo".into();
        }
        let voice_colors = pulsar_hw::VoiceColor::ALL
            .iter()
            .map(|c| c.as_str().to_owned())
            .collect();
        let announce_volume = store.announce_volume;
        Ok(Self {
            model,
            ports,
            store,
            applied_tz,
            dir,
            id,
            case_colors: pulsar_hw::case_colors(),
            voice_colors,
            zigbee: ZigbeePlan::new(model),
            changed: false,
            media: media::Policy::new(&model.volume, announce_volume),
            term,
            int,
        })
    }

    /// Light sensor before the entity list: the Clock auto brightness switch exists only when the
    /// sensor opened, and where the model makes the sensor optional, so does the Illuminance
    /// entity.
    fn open_sensor(&mut self) -> Option<UnboundedReceiver<f32>> {
        match pulsar_clock::spawn_light_sensor(super::LUX_PERIOD_MS) {
            Ok(rx) => Some(rx),
            Err(e) if self.model.lux.as_ref().is_some_and(|l| !l.optional) => {
                error!("{e}");
                None
            }
            Err(e) => {
                info!("{e}");
                None
            }
        }
    }

    /// Binds the ESPHome port first: a failure exits before the hardware starts. Nothing reaches
    /// the server before `run`.
    fn bind_api(
        &self,
        lux_present: bool,
    ) -> Result<(ApiHandle, UnboundedReceiver<ApiEvent>), StartError> {
        let strings = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let with_none = |l: Vec<String>| std::iter::once("None".to_owned()).chain(l).collect();
        let panel_present = self
            .model
            .clock
            .as_ref()
            .is_some_and(pulsar_clock::panel_present);
        let capabilities = logic::capabilities(
            self.model,
            panel_present,
            lux_present,
            self.case_colors.len() > 1,
            !self.voice_colors.is_empty(),
            self.zigbee.is_some(),
        );
        let cfg = ApiConfig {
            port: self.ports.api,
            capabilities,
            wake_words: vec![
                WakeWordInfo {
                    id: "alisa".into(),
                    phrase: "Алиса".into(),
                    languages: vec!["ru".into()],
                },
                WakeWordInfo {
                    id: "yandex".into(),
                    phrase: "Яндекс".into(),
                    languages: vec!["ru".into()],
                },
                WakeWordInfo {
                    id: "ricardo".into(),
                    phrase: "Рикардо".into(),
                    languages: vec!["ru".into()],
                },
            ],
            active_wake_words: vec![self.store.wake_word.clone()],
            light_effects: strings(&pulsar_hw::effect_names()),
            clock_faces: with_none(strings(&pulsar_clock::face_names())),
            music_styles: strings(&pulsar_hw::music_styles()),
            case_colors: with_none(self.case_colors.clone()),
            clock_music_styles: strings(&pulsar_clock::music_styles()),
            voice_colors: self.voice_colors.clone(),
            zigbee_modes: self
                .zigbee
                .as_ref()
                .map_or_else(Vec::new, |z| z.labels.clone()),
            key_path: Some(logic::in_dir(&self.dir, "api_key")),
        };
        pulsar_esphome::spawn(self.id.clone(), cfg).map_err(StartError::Api)
    }

    /// Before any other thread exists: the player's spawn sets `SSL_CERT_FILE` (`set_var`), which
    /// C code on another thread could be reading.
    fn spawn_player(&self) -> (PlayerHandle, UnboundedReceiver<PlayerEvent>) {
        pulsar_media::spawn(&self.model.audio.playback)
    }

    fn spawn_arbiter(&self) -> (ArbiterHandle, UnboundedReceiver<ArbiterEvent>) {
        pulsar_arbiter::spawn(self.id.name.clone(), self.ports.arbiter)
    }

    /// Hardware first: UI feedback should work even if HA is not connected.
    fn start_hardware(&mut self) -> (Box<dyn MixerPort>, ClockHandle, LedsHandle) {
        let leds = pulsar_hw::spawn_leds(&self.model.leds);
        // spawn handles a missing panel (None) with a no-op handle.
        let clock = pulsar_clock::spawn(self.model.clock.as_ref());
        clock.set_enabled(self.store.clock_on);
        clock.set_brightness(self.store.clock_brightness);
        clock.set_auto_brightness(self.store.clock_auto_brightness);
        clock.set_scroll_ms(self.store.clock_scroll_ms);
        clock.set_voice_indication(self.store.voice_indication);
        clock.set_music_style(super::clock_music(&self.store));
        leds.set_speed(self.store.led_speed);
        leds.set_voice_indication(self.store.voice_indication);
        leds.set_voice_color(super::voice_color(&self.store));
        leds.set_music_style(super::music_viz(&self.store));
        leds.set_user_light(
            self.store.light.on,
            super::light_rgb(self.store.light.r, self.store.light.g, self.store.light.b),
            self.store.light.brightness,
            super::light_effect(&self.store.light.effect),
            0,
        );

        let mut mixer: Box<dyn MixerPort> =
            match Mixer::open(&self.model.audio.playback, &self.model.volume) {
                Ok(mixer) => Box::new(mixer),
                Err(e) => {
                    error!("mixer: {e}");
                    Box::new(Mixer::fallback(&self.model.volume))
                }
            };
        if !mixer.available() {
            warn!("mixer unavailable");
        }
        let (volume, migrated) = logic::initial_volume(
            self.store.has_volume,
            self.store.volume,
            self.store.volume_legacy,
            self.store.legacy_on_stock,
            mixer.volume(),
            |v| pulsar_core::volume::from_legacy(&self.model.volume, v),
        );
        self.store.volume = volume;
        // Changes made while starting are written like any other (Store, 1 s later).
        self.changed |= migrated;
        if migrated {
            info!(
                "volume {:.3} from an earlier build -> {:.3}",
                self.store.volume_legacy, self.store.volume
            );
        }
        mixer.set_volume(self.store.volume);
        mixer.set_muted(self.store.muted);
        if !self.store.led_case_color.is_empty() {
            // After the volume: the write covers it too.
            let listed = self.case_colors.contains(&self.store.led_case_color);
            if !listed {
                warn!(
                    "case colour {} not in this firmware, none used",
                    self.store.led_case_color
                );
            }
            if listed {
                // The actor reads the file; an unreadable one leaves the correction off.
                leds.set_case_color(pulsar_hw::CaseColor::new(&self.store.led_case_color));
            } else {
                self.store.led_case_color.clear();
                self.changed = true;
            }
        }
        if !self.store.update_attempt.is_empty() {
            if self.store.update_attempt == VERSION {
                info!("updated to {VERSION}");
                self.store.update_attempt.clear();
                self.changed = true;
            } else {
                warn!(
                    "update to {} did not stay installed (rolled back?); automatic updates skip it",
                    self.store.update_attempt
                );
            }
        }
        (mixer, clock, leds)
    }

    fn start_updater(&self) -> (UpdaterHandle, UnboundedReceiver<UpdateEvent>) {
        pulsar_update::spawn(pulsar_update::UPDATE_REPO.into(), VERSION)
    }

    fn start_input(&self) -> (Option<InputHandle>, Option<UnboundedReceiver<InputEvent>>) {
        match pulsar_hw::spawn_input(&self.model.input) {
            Ok((h, rx)) => (Some(h), Some(rx)),
            Err(e) => {
                warn!("{e}");
                (None, None)
            }
        }
    }

    fn start_frontend(&self) -> (FrontendHandle, UnboundedReceiver<FrontendEvent>) {
        let custom_wake = self.store.wake_word == "ricardo";
        let (mic, rx) = pulsar_audio::spawn(
            &self.model.audio.capture,
            self.model.wake_preroll.as_ref(),
            &if custom_wake {
                // Native spotters stay available for a rollback to «Алиса»/«Яндекс».
                model_dir("alisa")
            } else {
                model_dir(&self.store.wake_word)
            },
            Some(Path::new(RICARDO_MODEL)),
            custom_wake,
            Some(PathBuf::from(super::COMMAND_MODEL)),
            Some(&pulsar_audio::WakeEventOptions {
                dir: logic::in_dir(&self.dir, WAKE_EVENTS_DIR),
                version: VERSION.into(),
                model: PathBuf::from(RICARDO_MODEL),
            }),
        );
        mic.set_preroll(self.store.wake_preroll);
        mic.set_wake_recording(self.store.wake_recording);
        mic.set_wake_dry_run(self.store.wake_dry_run);
        (mic, rx)
    }

    fn start_sendspin(&self) -> Option<(SendspinHandle, UnboundedReceiver<SendspinEvent>)> {
        let cfg = SendspinConfig {
            name: self.id.friendly_name.clone(),
            product_name: self.id.model.clone(),
            manufacturer: self.id.manufacturer.clone(),
            software_version: self.id.project_version.clone(),
            mac: self.id.mac.to_ascii_lowercase(),
            state_dir: logic::in_dir(&self.dir, "sendspin"),
            playback: &self.model.audio.playback,
            port: self.ports.sendspin,
        };
        // The one master-gain formula: the same value `Policy` applies to both channels.
        let master = self.media.master_gain(self.store.volume);
        match pulsar_sendspin::spawn(&cfg, self.store.volume, self.store.muted, master) {
            Ok((h, rx)) => Some((h, rx)),
            Err(e) => {
                debug!("{e}");
                error!("Sendspin player unavailable");
                None
            }
        }
    }

    /// A test instance (port offset) stays invisible to HA and Music Assistant.
    fn advertise(&self, api: &ApiHandle, sendspin: bool) -> Option<MdnsHandle> {
        if self.ports.offset != 0 {
            return None;
        }
        match pulsar_mdns::spawn(&self.id.name) {
            Ok(m) => {
                m.add_service(MdnsService {
                    service: "_esphomelib._tcp".into(),
                    instance: self.id.name.clone(),
                    port: self.ports.api,
                    txt: HaLink::mdns_txt(api),
                });
                if sendspin {
                    m.add_service(MdnsService {
                        service: "_sendspin._tcp".into(),
                        instance: self.id.name.clone(),
                        port: self.ports.sendspin,
                        txt: vec![
                            "path=/sendspin".into(),
                            format!("name={}", self.id.friendly_name),
                        ],
                    });
                }
                Some(m)
            }
            Err(e) => {
                warn!("api: mDNS advertising unavailable: {e}");
                None
            }
        }
    }

    fn start_radio(
        &self,
        updater: &dyn Updater,
        mdns: Option<&MdnsHandle>,
    ) -> Option<(ZigbeeHandle, UnboundedReceiver<ZigbeeEvent>)> {
        // See `ZigbeePlan::new`: the gate is a constant, not an `Option` read.
        if !FEATURES.zigbee {
            return None;
        }
        let plan = self.zigbee.as_ref()?;
        let zcfg = ZigbeeConfig {
            lines: pulsar_zigbee::RadioPaths {
                tty: plan.radio.tty.into(),
                reset: plan.radio.reset.into(),
                boot: plan.radio.boot.into(),
            },
            coordinator_image: logic::in_dir(&self.dir, "zigbee-coordinator.ybl"),
            router_image: logic::in_dir(&self.dir, "zigbee-router.ybl"),
            deconz_port: self.ports.deconz,
            failed_version: self.store.zigbee_flash_failed,
        };
        updater.add_extra_asset("zigbee-coordinator.ybl", zcfg.coordinator_image.clone());
        updater.add_extra_asset("zigbee-router.ybl", zcfg.router_image.clone());
        let mode = super::zigbee_role(&self.store);
        let (h, rx) = pulsar_zigbee::spawn(zcfg, mode);
        // ZHA discovers the coordinator (radio_type selects zigpy-deconz, serial_number
        // is its unique id).
        if mode == pulsar_zigbee::Mode::Coordinator
            && let Some(m) = mdns
        {
            m.add_service(MdnsService {
                service: "_zigbee-coordinator._tcp".into(),
                instance: self.id.name.clone(),
                port: self.ports.deconz,
                txt: vec![
                    "radio_type=deconz".into(),
                    format!("serial_number={}", self.id.name),
                    format!("name={}", self.id.friendly_name),
                ],
            });
        }
        Some((h, rx))
    }
}

/// Everything `Controller` needs, built once the services are up.
pub struct Wired {
    pub deps: super::ports::Deps,
    pub rx: Rx,
    pub term: Signal,
    pub int: Signal,
    pub model: &'static DeviceProfile,
    pub store: Store,
    pub media: media::Policy,
    pub applied_tz: String,
    pub hw_mic_muted: bool,
    /// Startup fixed a setting (volume migration, case colour, update_attempt): write it.
    pub persist_on_start: bool,
}

/// Starts every service in the C++ `start()` order and returns the controller's dependencies.
pub fn wire(model: &'static DeviceProfile, state_path: PathBuf) -> Result<Wired, StartError> {
    let mut boot = Boot::load(model, state_path)?;
    let lux_rx = boot.open_sensor();
    let (api, api_rx) = boot.bind_api(lux_rx.is_some())?;
    let (player, player_rx) = boot.spawn_player();
    let (arbiter, arbiter_rx) = boot.spawn_arbiter();
    let (mixer, clock, leds) = boot.start_hardware();
    let (updater, updater_rx) = boot.start_updater();
    let (input, input_rx) = boot.start_input();
    let (frontend, frontend_rx) = boot.start_frontend();
    let sendspin = boot.start_sendspin();
    let mdns = boot.advertise(&api, sendspin.is_some());
    let radio = boot.start_radio(&updater, mdns.as_ref());

    let (sendspin_handle, sendspin_rx) = sendspin.unzip();
    let (radio_handle, radio_rx) = radio.unzip();
    let hw_mic_muted = input.as_ref().is_some_and(InputHandle::hw_muted);
    let Boot {
        ports: _,
        store,
        applied_tz,
        id: _,
        case_colors: _,
        voice_colors: _,
        zigbee: _,
        changed,
        media,
        term,
        int,
        dir: _,
        model: _,
    } = boot;
    Ok(Wired {
        deps: super::ports::Deps {
            sendspin: sendspin_handle.map(|h| Box::new(h) as Box<dyn Sendspin>),
            radio: radio_handle.map(|h| Box::new(h) as Box<dyn Radio>),
            updater: Box::new(updater) as Box<dyn Updater>,
            arbiter: Box::new(arbiter) as Box<dyn Arbiter>,
            _input: input,
            display: Display {
                clock: Box::new(clock) as Box<dyn Clock>,
                leds: Box::new(leds) as Box<dyn Leds>,
            },
            mixer,
            player: Box::new(player) as Box<dyn Player>,
            frontend: Box::new(frontend) as Box<dyn Frontend>,
            mdns: mdns.map(|m| Box::new(m) as Box<dyn Mdns>),
            ha: Box::new(api) as Box<dyn HaLink>,
        },
        rx: Rx {
            api: Some(api_rx),
            frontend: Some(frontend_rx),
            player: Some(player_rx),
            arbiter: Some(arbiter_rx),
            input: input_rx,
            lux: lux_rx,
            sendspin: sendspin_rx,
            zigbee: radio_rx,
            updater: Some(updater_rx),
        },
        term,
        int,
        model,
        store,
        media,
        applied_tz,
        hw_mic_muted,
        persist_on_start: changed,
    })
}
