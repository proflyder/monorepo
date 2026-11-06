//! Актор API-сервера: TCP-транспорт (plaintext или Noise по первому байту клиента), лимиты
//! соединений и keepalive, владелец голосового пайплайна среди нескольких клиентов, поток
//! логов, ключ шифрования. Сообщения сущностей — в `server/entities.rs`, разбор команд — в
//! `server/commands.rs`.

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use esphome_proto::entity::EntityKey;
use esphome_proto::pb::msg;
use esphome_proto::session::{Session, SessionConfig};
use home_actor::Handle;
use pulsar_core::Timers;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;
use tracing::{Level, error, info, warn};

use crate::key::{self, Key, SetKey, os_error_text};
use crate::transport::{Listener, TcpListener, Transport};
use crate::types::{ApiConfig, ApiEvent, DeviceIdentity};
use crate::voice::{self, Standing, VoiceOwnership};

mod commands;
mod entities;

pub use entities::{Diagnostics, Publish};
use entities::{EntityState, parse_wireless};

pub use esphome_proto::{ESPHOME_VERSION, PROJECT_NAME};

/// Завершить голосовой run, если сокет отстал настолько: дыра в фразе заставит HA
/// транскрибировать мусор.
const AUDIO_BACKLOG_LIMIT: usize = 256 << 10;
/// Отказаться от клиента, который перестал читать.
const MAX_BACKLOG: usize = 8 << 20;
/// На одно пробуждение и клиента: потоковый пир не должен морить голодом runtime.
const READ_CHUNK: usize = 16 << 10;
/// HA плюс, например, просмотрщик логов ESPHome dashboard или aioesphomeapi.
const MAX_CLIENTS: usize = 4;
/// Соединения, ещё не приславшие валидный hello.
const MAX_PENDING: usize = 4;
const HELLO_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_PING: Duration = Duration::from_secs(45);
const IDLE_DROP: Duration = Duration::from_secs(100);
const TIME_RESYNC: Duration = Duration::from_secs(6 * 3600);
const TICK_MS: u64 = 5000;
/// Ответ SetKey уходит в старую сессию до перечитывания файла ключа.
const KEY_RELOAD_MS: u64 = 100;
/// Пауза accept после ошибки (EMFILE, ENFILE, ENOBUFS, ENOMEM): слушатель остаётся готовым,
/// повтор сразу закрутил бы цикл.
const ACCEPT_RETRY_MS: u64 = 500;

/// Accept не удался из-за нехватки дескрипторов или памяти (слушатель остаётся готовым), а не
/// из-за одного принимаемого соединения.
fn accept_exhausted(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EMFILE | libc::ENFILE | libc::ENOBUFS | libc::ENOMEM)
    )
}

/// Состояние, которое читают геттеры handle; актор обновляет его до отправки события.
///
/// Это минимальное зеркало, а не второй источник истины: актор владеет им, геттеры обязаны
/// быть синхронными и неблокирующими.
pub struct Shared {
    pub connected: Cell<bool>,
    pub voice_subscribed: Cell<bool>,
    pub mdns_txt: RefCell<Vec<String>>,
    /// Последний выданный id голосового run (пишет только handle).
    pub run_seq: Cell<u64>,
}

pub enum Cmd {
    VoiceRequestStart {
        run: u64,
        phrase: String,
    },
    VoiceRequestStop,
    VoiceSendAudio(Vec<i16>),
    /// VoiceAssistantAudio{end=true}: мягкий конец аудио, не abort.
    VoiceAudioEnd,
    VoiceAnnounceFinished(bool),
    VoiceAnswerFinished(bool),
    SetActiveWakeWords(Vec<String>),
    Publish(Publish),
    /// Последний `ApiHandle` упал: закрыть порт и попросить клиентов отключиться.
    Shutdown,
    ClientData {
        id: u64,
        bytes: Vec<u8>,
    },
    ClientEof {
        id: u64,
    },
    ClientError {
        id: u64,
        kind: io::ErrorKind,
    },
    RefreshDiagnostics,
    Diagnostics(Diagnostics),
}

/// Привязывает порт (все IPv4-интерфейсы), читает ключ шифрования и запускает актор в текущем
/// `LocalSet`.
pub fn spawn(
    id: DeviceIdentity,
    cfg: ApiConfig,
) -> Result<(crate::ApiHandle, UnboundedReceiver<ApiEvent>), crate::SpawnError> {
    let listener = TcpListener::bind(cfg.port)?;
    spawn_with(Box::new(listener), id, cfg)
}

pub fn spawn_with(
    listener: Box<dyn Listener>,
    id: DeviceIdentity,
    mut cfg: ApiConfig,
) -> Result<(crate::ApiHandle, UnboundedReceiver<ApiEvent>), crate::SpawnError> {
    info!("api: listening on :{} as '{}'", cfg.port, id.name);
    // Пустой путь означает «без шифрования», как в C++-конфигурации.
    cfg.key_path = cfg.key_path.filter(|p| !p.as_os_str().is_empty());
    let key = key::load_key(cfg.key_path.as_deref());
    let shared = Rc::new(Shared {
        connected: Cell::new(false),
        voice_subscribed: Cell::new(false),
        mdns_txt: RefCell::new(txt_records(&id, cfg.key_path.is_some(), key.has_key)),
        run_seq: Cell::new(0),
    });
    let (cmds, cmd_rx) = home_actor::channel();
    let (ev_tx, ev_rx) = unbounded_channel();
    let mut timers = Timers::new();
    timers.set_repeating(Timer::Tick, TICK_MS, TICK_MS);
    timers.set_repeating(Timer::Diag, DIAG_PERIOD_MS, DIAG_PERIOD_MS);
    let server = Server {
        mac_plain: mac_plain(&id.mac),
        id,
        cfg,
        shared: shared.clone(),
        events: ev_tx,
        cmds: cmd_rx,
        cmd_handle: cmds.clone(),
        listener,
        timers,
        log_notify: Arc::new(Notify::new()),
        clients: Vec::new(),
        next_client: 0,
        key,
        voice_seq: 0,
        run: 0,
        audio_overflowed: false,
        tx_scratch: Vec::new(),
        accept_paused: false,
        accept_failing: false,
        voice: VoiceOwnership::new(),
        states: std::collections::BTreeMap::new(),
        diagnostics: Diagnostics::default(),
        started: Instant::now(),
        stop: false,
    };
    pulsar_core::supervisor().spawn_local("api", server.run());
    // Первичные диагностики приходят из blocking-пула, не из цикла актора.
    let _ = cmds.send(Cmd::RefreshDiagnostics);
    Ok((
        crate::ApiHandle {
            cmds,
            shared,
            live: Rc::new(()),
        },
        ev_rx,
    ))
}

/// "aabbccddeeff", как ESPHome кладёт в mDNS TXT и Noise server hello.
pub fn mac_plain(mac: &str) -> String {
    mac.chars()
        .filter(|&c| c != ':')
        .map(|c| {
            if ('A'..='F').contains(&c) {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// TXT-записи `_esphomelib._tcp`.
pub fn txt_records(id: &DeviceIdentity, key_supported: bool, has_key: bool) -> Vec<String> {
    let mut txt = vec![
        format!("friendly_name={}", id.friendly_name),
        format!("version={ESPHOME_VERSION}"),
        format!("mac={}", mac_plain(&id.mac)),
        "platform=host".to_owned(),
        format!("board={}", id.board),
        "network=wifi".to_owned(),
        format!("project_name={PROJECT_NAME}"),
        format!("project_version={}", id.project_version),
    ];
    // HA читает их, чтобы попросить ключ / предложить провижининг до подключения.
    if key_supported {
        let field = if has_key {
            "api_encryption"
        } else {
            "api_encryption_supported"
        };
        txt.push(format!("{field}=Noise_NNpsk0_25519_ChaChaPoly_SHA256"));
    }
    txt
}

/// Наши уровни <-> ESPHome LogLevel (DEBUG 5, INFO 3, WARN 2, ERROR 1).
fn esphome_level(level: Level) -> u32 {
    match level {
        Level::ERROR => 1,
        Level::WARN => 2,
        Level::INFO => 3,
        Level::DEBUG | Level::TRACE => 5,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Timer {
    Tick,
    KeyReload,
    AcceptResume,
    Diag,
}

pub struct Client {
    id: u64,
    peer: String,
    out: UnboundedSender<Vec<u8>>,
    backlog: Rc<Cell<usize>>,
    session: Session,
    /// Неполный кадр, оставшийся от прошлого чтения: TCP режет кадры произвольно.
    rbuf: Vec<u8>,
    hello: bool,
    /// Подписан на состояния сущностей.
    states: bool,
    /// Порядок его SubscribeVoiceAssistant(true), 0 = не подписан.
    voice_wanted: u64,
    connected_at: Instant,
    last_rx: Instant,
    ping_outstanding: bool,
    last_time_req: Instant,
    /// Уровень SubscribeLogsRequest (ESPHome LogLevel), 0 = нет.
    log_level: u32,
}

impl Client {
    fn standing(&self, owner: Option<u64>) -> Standing {
        Standing {
            id: self.id,
            hello: self.hello,
            voice: owner == Some(self.id),
            voice_wanted: self.voice_wanted,
            last_rx: self.last_rx,
        }
    }
}

enum Wake {
    Timer(Timer),
    Cmd(Cmd),
    Closed,
    Logs,
    Accept(io::Result<(Box<dyn Transport>, String)>),
}

struct Server {
    id: DeviceIdentity,
    cfg: ApiConfig,
    mac_plain: String,
    shared: Rc<Shared>,
    events: UnboundedSender<ApiEvent>,
    cmds: UnboundedReceiver<Cmd>,
    cmd_handle: Handle<Cmd>,
    listener: Box<dyn Listener>,
    timers: Timers<Timer>,
    /// Строки логов приходят из любого потока; sink только будит актор через это.
    log_notify: Arc<Notify>,
    /// Все соединения в порядке accept. Соединение считается в MAX_CLIENTS только после
    /// валидного HelloRequest (после Noise handshake при шифровании), и только тогда может
    /// вытеснить установленного клиента: сканеры портов и мусор не сбивают HA.
    clients: Vec<Client>,
    next_client: u64,
    key: Key,
    /// Упорядочивает голосовые подписки (`Client::voice_wanted`).
    voice_seq: u64,
    /// Run id последнего voice_request_start: к какому run относятся голосовые события.
    run: u64,
    /// Микрофонное аудио остановлено для этого run (backlog).
    audio_overflowed: bool,
    /// Открытый текст Noise-кадра при шифровании.
    tx_scratch: Vec<u8>,
    /// Ветка accept выключена до Timer::AcceptResume.
    accept_paused: bool,
    /// Ошибка accept уже залогирована; сбрасывается следующим принятым соединением.
    accept_failing: bool,
    voice: VoiceOwnership,
    /// Единственный источник истины состояний сущностей; replay обходит таблицу.
    states: std::collections::BTreeMap<EntityKey, EntityState>,
    diagnostics: Diagnostics,
    started: Instant,
    /// Пришёл `Cmd::Shutdown`: цикл завершается, клиенты получают DisconnectRequest.
    stop: bool,
}

impl Server {
    async fn run(mut self) {
        loop {
            // Таймеры первыми (контракт порядка pulsar_core::Timers); команды раньше сокетов.
            let wake = tokio::select! {
                biased;
                t = self.timers.next() => Wake::Timer(t),
                cmd = self.cmds.recv() => cmd.map_or(Wake::Closed, Wake::Cmd),
                () = self.log_notify.notified() => Wake::Logs,
                r = poll_fn(|cx| self.listener.poll_accept(cx)), if !self.accept_paused => {
                    Wake::Accept(r)
                }
            };
            match wake {
                Wake::Timer(Timer::Tick) => self.tick(),
                Wake::Timer(Timer::KeyReload) => self.reload_key(),
                Wake::Timer(Timer::AcceptResume) => self.accept_paused = false,
                Wake::Timer(Timer::Diag) => self.spawn_diagnostics(true),
                Wake::Cmd(cmd) => {
                    self.command(cmd);
                    if self.stop {
                        break;
                    }
                }
                Wake::Closed => break,
                Wake::Logs => self.drain_logs(),
                Wake::Accept(r) => self.accept(r),
            }
        }
        self.teardown();
    }

    fn client(&mut self, id: u64) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    fn ready(&self) -> bool {
        self.clients.iter().any(|c| c.hello)
    }

    fn voice_client(&self) -> Option<u64> {
        self.voice.current()
    }

    fn for_each_client(&mut self, mut f: impl FnMut(&mut Self, u64)) {
        let mut i = 0;
        while let Some(id) = self.clients.get(i).map(|c| c.id) {
            f(self, id);
            if self.clients.get(i).is_some_and(|c| c.id == id) {
                i += 1;
            }
        }
    }

    /// Обновляет то, что читают геттеры handle, затем отправляет событие.
    fn emit(&self, ev: ApiEvent) {
        self.shared.connected.set(self.ready());
        self.shared
            .voice_subscribed
            .set(self.voice.current().is_some());
        let _ = self.events.send(ev);
    }

    fn accept(&mut self, r: io::Result<(Box<dyn Transport>, String)>) {
        match r {
            Ok((transport, peer)) => {
                self.accept_failing = false;
                self.on_accept(transport, peer);
            }
            Err(e) if !accept_exhausted(&e) => warn!("api: accept: {}", os_error_text(&e)),
            Err(e) => {
                if !self.accept_failing {
                    warn!("api: accept: {}", os_error_text(&e));
                }
                self.accept_failing = true;
                self.accept_paused = true;
                self.timers.set(Timer::AcceptResume, ACCEPT_RETRY_MS);
            }
        }
    }

    fn on_accept(&mut self, transport: Box<dyn Transport>, peer: String) {
        if self.clients.iter().filter(|c| !c.hello).count() >= MAX_PENDING
            && let Some(oldest) = self.clients.iter().find(|c| !c.hello).map(|c| c.id)
        {
            self.close_client(oldest, "superseded by newer connections");
        }
        let now = Instant::now();
        info!("api: connection from {peer}");
        self.next_client += 1;
        let id = self.next_client;
        let (out, out_rx) = unbounded_channel();
        let backlog = Rc::new(Cell::new(0));
        let mut session_cfg = SessionConfig::new(
            &self.id.name,
            &self.mac_plain,
            self.key.psk.clone(),
            !self.key.has_key,
        );
        session_cfg.reject_all = self.key.reject_all;
        let session = Session::new(session_cfg);
        self.clients.push(Client {
            id,
            peer,
            out,
            backlog: backlog.clone(),
            session,
            rbuf: Vec::new(),
            hello: false,
            states: false,
            voice_wanted: 0,
            connected_at: now,
            last_rx: now,
            ping_outstanding: false,
            last_time_req: now,
            log_level: 0,
        });
        let cmds = self.cmd_handle.clone();
        pulsar_core::supervisor().spawn_local(
            "api-client",
            run_client(transport, out_rx, cmds, id, backlog),
        );
    }

    fn close_client(&mut self, id: u64, why: &str) {
        let Some(i) = self.clients.iter().position(|c| c.id == id) else {
            return;
        };
        let c = self.clients.remove(i);
        let state = if c.hello { "disconnected" } else { "dropped" };
        info!("api: {} {state} ({why})", c.peer);
        if c.log_level != 0 {
            self.update_log_sink();
        }
        if !c.hello {
            return;
        }
        if self.voice.current() == Some(id) {
            self.voice.set_current(None);
            self.emit(ApiEvent::VoiceSubscribed(false));
        }
        self.update_voice_owner();
        if !self.ready() {
            self.emit(ApiEvent::ClientDisconnected);
        }
    }

    /// Передаёт пайплайн клиенту `voice_choice`; при каждом переходе события видят false
    /// перед true.
    fn update_voice_owner(&mut self) {
        let want = voice::voice_choice(
            self.clients
                .iter()
                .map(|c| c.standing(self.voice.current())),
        );
        let cur = self.voice.current();
        if cur == want {
            return;
        }
        if let Some(cur) = cur {
            let want_peer = want
                .and_then(|w| self.clients.iter().find(|c| c.id == w))
                .map(|c| c.peer.clone());
            let cur_peer = self
                .clients
                .iter()
                .find(|c| c.id == cur)
                .map(|c| c.peer.clone())
                .unwrap_or_default();
            if let Some(want_peer) = want_peer {
                info!("api: voice assistant moves from {cur_peer} to {want_peer}");
            }
            self.voice.set_current(None);
            self.emit(ApiEvent::VoiceSubscribed(false));
        }
        if let Some(want) = want {
            if cur.is_none()
                && let Some(c) = self.clients.iter().find(|c| c.id == want)
            {
                info!("api: voice assistant served by {}", c.peer);
            }
            self.voice.set_current(Some(want));
            self.emit(ApiEvent::VoiceSubscribed(true));
        }
    }

    fn make_room(&mut self) {
        let (victim, established) = voice::eviction_victim(
            self.clients
                .iter()
                .map(|c| c.standing(self.voice.current())),
        );
        if established >= MAX_CLIENTS
            && let Some(victim) = victim
        {
            self.close_client(victim, "replaced by a new connection");
        }
    }

    // ------------------------------------------------------------ input

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            // Обрабатывает цикл: здесь только флаг, чтобы teardown был в одном месте.
            Cmd::Shutdown => self.stop = true,
            Cmd::ClientData { id, bytes } => self.client_data(id, &bytes),
            Cmd::ClientEof { id } => self.close_client(id, "connection closed by peer"),
            Cmd::ClientError { id, kind } => {
                self.close_client(id, &format!("client io error: {kind:?}"));
            }
            Cmd::RefreshDiagnostics => self.spawn_diagnostics(false),
            Cmd::Diagnostics(d) => self.apply_diagnostics(d),
            Cmd::VoiceRequestStart { run, phrase } => self.voice_request_start(run, &phrase),
            // Устаревшие команды, адресованные run предыдущего владельца. Отчёт об анонсе
            // относится к анонсу, а не к run, и уходит владельцу всегда.
            Cmd::VoiceRequestStop
            | Cmd::VoiceSendAudio(_)
            | Cmd::VoiceAudioEnd
            | Cmd::VoiceAnswerFinished(_)
                if self.voice.is_stale() => {}
            Cmd::VoiceRequestStop => self.voice_request_stop(),
            Cmd::VoiceSendAudio(pcm) => self.voice_send_audio(&pcm),
            Cmd::VoiceAudioEnd => self.voice_send_audio_end(),
            Cmd::VoiceAnnounceFinished(success) | Cmd::VoiceAnswerFinished(success) => {
                self.voice_announce_finished(success);
            }
            Cmd::SetActiveWakeWords(ids) => self.cfg.active_wake_words = ids,
            Cmd::Publish(p) => self.publish(p),
        }
    }

    fn client_data(&mut self, id: u64, bytes: &[u8]) {
        let Some(c) = self.client(id) else { return };
        c.last_rx = Instant::now();
        c.ping_outstanding = false;
        c.rbuf.extend_from_slice(bytes);
        // Верхняя граница буфера — максимальный кадр плюс два чтения (кадр мог закончиться, а
        // следующее чтение уже принесло начало нового); больше в неполном кадре быть не может.
        if c.rbuf.len() > esphome_proto::frame::MAX_FRAME + 2 * READ_CHUNK {
            self.close_client(id, "frame too large");
            return;
        }
        let input = std::mem::take(&mut c.rbuf);
        let mut out = Vec::new();
        let feed = c.session.feed(&input, &mut out);
        // Протокольный вывод (server hello, ответ handshake, отказ) — до ответов на сообщения.
        if !out.is_empty() {
            self.send_raw(id, out);
        }
        // Неполный хвост возвращаем до обработки: она может закрыть клиента.
        if feed.consumed < input.len()
            && let Some(c) = self.client(id)
        {
            c.rbuf.extend_from_slice(&input[feed.consumed..]);
        }
        for m in feed.messages {
            if self.client(id).is_none() {
                return;
            }
            self.handle(id, m.msg_type, &m.payload);
        }
        if let Some(event) = feed.event {
            self.close_client(id, &format!("{event:?}"));
        }
    }

    // ------------------------------------------------------------ output

    fn send_raw(&mut self, id: u64, bytes: Vec<u8>) {
        let Some(c) = self.client(id) else { return };
        let pending = c.backlog.get() + bytes.len();
        if pending > MAX_BACKLOG {
            self.close_client(id, "client not reading");
            return;
        }
        // Счётчик уменьшает клиентская задача после записи в сокет: он и есть объём вывода,
        // который ещё не ушёл, включая то, что лежит в канале.
        c.backlog.set(pending);
        if c.out.send(bytes).is_err() {
            self.close_client(id, "connection gone");
        }
    }

    fn send_msg(&mut self, id: u64, msg_type: u32, payload: &[u8]) {
        let mut frame = Vec::new();
        let mut scratch = std::mem::take(&mut self.tx_scratch);
        let Some(c) = self.client(id) else {
            self.tx_scratch = scratch;
            return;
        };
        let r = c
            .session
            .encode(msg_type, payload, &mut frame, &mut scratch);
        self.tx_scratch = scratch;
        match r {
            Ok(()) => {}
            Err(esphome_proto::frame::EncryptError::NotReady) => return,
            Err(esphome_proto::frame::EncryptError::TooLarge) => {
                error!(
                    "api: message {msg_type} too large for a Noise frame ({} bytes)",
                    payload.len()
                );
                return;
            }
            Err(esphome_proto::frame::EncryptError::Failed) => {
                error!("api: noise encrypt failed");
                return;
            }
        }
        self.send_raw(id, frame);
    }

    // ------------------------------------------------------------ timers

    fn tick(&mut self) {
        let now = Instant::now();
        self.for_each_client(|s, id| {
            let Some(c) = s.client(id) else { return };
            if !c.hello {
                if now.saturating_duration_since(c.connected_at) > HELLO_TIMEOUT {
                    s.close_client(id, "no hello");
                }
                return;
            }
            let idle = now.saturating_duration_since(c.last_rx);
            if idle > IDLE_DROP {
                s.close_client(id, "keepalive timeout");
                return;
            }
            if idle > IDLE_PING && !c.ping_outstanding {
                c.ping_outstanding = true;
                s.send_msg(id, msg::PING_REQ, &[]);
            }
            if s.client(id)
                .is_some_and(|c| now.saturating_duration_since(c.last_time_req) >= TIME_RESYNC)
            {
                s.request_time(id);
            }
        });
    }

    fn request_time(&mut self, id: u64) {
        let Some(c) = self.client(id) else { return };
        c.last_time_req = Instant::now();
        self.send_msg(id, msg::GET_TIME_REQ, &[]);
    }

    fn spawn_diagnostics(&mut self, publish: bool) {
        let tx = self.cmd_handle.clone();
        pulsar_core::supervisor().spawn("api-diag", async move {
            let d = tokio::task::spawn_blocking(move || Diagnostics {
                rssi: std::fs::read_to_string("/proc/net/wireless")
                    .ok()
                    .and_then(|t| parse_wireless(&t)),
                ip: pulsar_core::net::best_interface_checked()
                    .ok()
                    .flatten()
                    .map(|i| i.addr),
                publish,
            })
            .await
            .unwrap_or(Diagnostics {
                publish,
                ..Diagnostics::default()
            });
            let _ = tx.send(Cmd::Diagnostics(d));
        });
    }

    fn apply_diagnostics(&mut self, d: Diagnostics) {
        let ip_changed = d.ip != self.diagnostics.ip;
        if let Some(rssi) = d.rssi {
            self.diagnostics.rssi = Some(rssi);
        }
        if d.ip.is_some() {
            self.diagnostics.ip = d.ip;
        }
        if !d.publish || !self.states_on() {
            return;
        }
        self.send_diagnostics(None, ip_changed);
    }

    /// NoiseEncryptionSetKeyRequest: пустой ключ очищает. Как в ESPHome, ответ уходит в старую
    /// сессию, затем все соединения закрываются, чтобы вернулись только клиенты с новым ключом
    /// (или plaintext-клиенты после очистки).
    fn set_key(&mut self, b64: &[u8]) -> bool {
        match key::set_key(self.cfg.key_path.as_deref(), b64, &self.key) {
            Err(e) => {
                warn!("{e}");
                false
            }
            Ok(SetKey::Rejected) => false,
            Ok(SetKey::Unchanged) => true,
            Ok(SetKey::Changed) => {
                self.timers.set(Timer::KeyReload, KEY_RELOAD_MS);
                true
            }
        }
    }

    fn reload_key(&mut self) {
        self.key = key::load_key(self.cfg.key_path.as_deref());
        let txt = txt_records(&self.id, self.cfg.key_path.is_some(), self.key.has_key);
        self.shared.mdns_txt.replace(txt.clone());
        self.emit(ApiEvent::MdnsTxtChanged(txt));
        warn!("api: encryption key changed, disconnecting all clients");
        self.for_each_client(|s, id| {
            let Some(c) = s.client(id) else { return };
            if c.hello {
                s.send_msg(id, msg::DISCONNECT_REQ, &[]);
            }
            s.close_client(id, "encryption key changed");
        });
    }

    // ------------------------------------------------------------ logs

    fn update_log_sink(&self) {
        let want = self
            .clients
            .iter()
            .filter(|c| c.hello)
            .map(|c| c.log_level)
            .max()
            .unwrap_or(0);
        let min = match want {
            0 => return pulsar_core::set_log_sink(None, None),
            5.. => Level::DEBUG,
            3 | 4 => Level::INFO,
            2 => Level::WARN,
            1 => Level::ERROR,
        };
        pulsar_core::set_log_sink(Some(min), Some(self.log_notify.clone()));
    }

    fn drain_logs(&mut self) {
        for line in pulsar_core::take_log_lines() {
            let level = esphome_level(line.level);
            let m = esphome_proto::pb::SubscribeLogsResponse {
                level: Some(level),
                message: Some(line.text.into_bytes()),
            };
            let payload = prost::Message::encode_to_vec(&m);
            self.for_each_client(|s, id| {
                if s.client(id)
                    .is_some_and(|c| c.hello && c.log_level >= level)
                {
                    s.send_msg(id, msg::SUBSCRIBE_LOGS_RESP, &payload);
                }
            });
        }
    }

    // ------------------------------------------------------------ voice (handle commands)

    fn voice_request_start(&mut self, run: u64, phrase: &str) {
        self.run = run;
        self.voice.start_run();
        let Some(vc) = self.voice_client() else {
            warn!("api: voice_request_start without a subscribed client");
            return;
        };
        let audio = esphome_proto::pb::VoiceAssistantAudioSettings {
            noise_suppression_level: None,
            auto_gain: None,
            volume_multiplier: Some(1.0),
        };
        let m = esphome_proto::pb::VoiceAssistantRequest {
            start: Some(true),
            flags: None,
            audio_settings: Some(audio),
            wake_word_phrase: (!phrase.is_empty()).then(|| phrase.to_owned()),
        };
        self.audio_overflowed = false;
        self.send_msg(vc, msg::VA_REQUEST, &prost::Message::encode_to_vec(&m));
    }

    fn voice_request_stop(&mut self) {
        let Some(vc) = self.voice_client() else {
            return;
        };
        let m = esphome_proto::pb::VoiceAssistantRequest::default();
        self.send_msg(vc, msg::VA_REQUEST, &prost::Message::encode_to_vec(&m));
    }

    /// 16 кГц моно S16LE, пока пайплайн слушает.
    fn voice_send_audio(&mut self, pcm: &[i16]) {
        let Some(vc) = self.voice_client() else {
            return;
        };
        if pcm.is_empty() || self.audio_overflowed {
            return;
        }
        // Дыра во фразе заставит HA транскрибировать мусор: лучше завершить run.
        if self
            .client(vc)
            .is_some_and(|c| c.backlog.get() > AUDIO_BACKLOG_LIMIT)
        {
            warn!("api: network backlog, ending the voice pipeline");
            self.audio_overflowed = true;
            self.voice_request_stop();
            self.emit(ApiEvent::VoiceAudioOverflow { run: self.run });
            return;
        }
        let data: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let m = esphome_proto::pb::VoiceAssistantAudio {
            data: Some(data),
            end: None,
        };
        self.send_msg(vc, msg::VA_AUDIO, &prost::Message::encode_to_vec(&m));
    }

    /// `VoiceAssistantAudio{end = true}` без данных: мягкий «говорящий закончил»
    /// (aioesphomeapi `handle_stop(False)`), в отличие от abort-запроса
    /// `VoiceAssistantRequest{start = false}` в [`Self::voice_request_stop`].
    fn voice_send_audio_end(&mut self) {
        let Some(vc) = self.voice_client() else {
            return;
        };
        let m = esphome_proto::pb::VoiceAssistantAudio {
            data: None,
            end: Some(true),
        };
        self.send_msg(vc, msg::VA_AUDIO, &prost::Message::encode_to_vec(&m));
    }

    fn voice_announce_finished(&mut self, success: bool) {
        let Some(vc) = self.voice_client() else {
            return;
        };
        let m = esphome_proto::pb::VoiceAssistantAnnounceFinished {
            success: success.then_some(true),
        };
        self.send_msg(
            vc,
            msg::VA_ANNOUNCE_FINISHED,
            &prost::Message::encode_to_vec(&m),
        );
    }

    // ------------------------------------------------------------ dispatch

    fn handle(&mut self, id: u64, msg_type: u32, p: &[u8]) {
        let Some(c) = self.client(id) else { return };
        if !c.hello
            && !matches!(
                msg_type,
                msg::HELLO_REQ | msg::DISCONNECT_REQ | msg::PING_REQ
            )
        {
            warn!("api: message {msg_type} before hello");
            self.close_client(id, "protocol violation");
            return;
        }
        match msg_type {
            msg::HELLO_REQ => self.hello(id, p),
            msg::DISCONNECT_REQ => {
                self.send_msg(id, msg::DISCONNECT_RESP, &[]);
                self.close_client(id, "disconnect requested");
            }
            msg::DISCONNECT_RESP => self.close_client(id, "disconnect acknowledged"),
            msg::PING_REQ => self.send_msg(id, msg::PING_RESP, &[]),
            msg::PING_RESP => {}
            msg::SUBSCRIBE_LOGS_REQ => {
                let m: esphome_proto::pb::SubscribeLogsRequest = decode(p);
                let level = m.level.unwrap_or(0);
                if let Some(c) = self.client(id) {
                    info!("api: {} log subscription, level {level}", c.peer);
                    c.log_level = level;
                }
                self.update_log_sink();
            }
            msg::GET_TIME_RESP => self.on_time(id, p),
            msg::SUBSCRIBE_VA_REQ => self.on_voice_subscribe(id, p),
            msg::VA_RESPONSE => self.on_voice_response(id, p),
            msg::NOISE_SET_KEY_REQ => self.on_set_key(id, p),
            _ => {
                self.handle_entities(id, msg_type, p);
            }
        }
    }

    fn hello(&mut self, id: u64, p: &[u8]) {
        let req: esphome_proto::pb::HelloRequest = decode(p);
        let (major, minor) = (
            req.api_version_major.unwrap_or(0),
            req.api_version_minor.unwrap_or(0),
        );
        let info = req.client_info.unwrap_or_default();
        let first = self.client(id).is_some_and(|c| !c.hello);
        let resp = esphome_proto::pb::HelloResponse {
            api_version_major: Some(esphome_proto::API_MAJOR),
            api_version_minor: Some(esphome_proto::API_MINOR),
            server_info: Some(format!("pulsar {}", self.id.project_version)),
            name: Some(self.id.name.clone()),
        };
        if first {
            self.make_room();
        }
        self.send_msg(id, msg::HELLO_RESP, &prost::Message::encode_to_vec(&resp));
        let Some(c) = self.client(id) else { return };
        info!(
            "api: hello from '{info}' (api {major}.{minor}) at {}",
            c.peer
        );
        if first {
            c.hello = true;
            self.emit(ApiEvent::ClientConnected { client_info: info });
            self.request_time(id);
        }
    }

    fn on_time(&mut self, id: u64, p: &[u8]) {
        let m: esphome_proto::pb::GetTimeResponse = decode(p);
        let epoch = m.epoch_seconds.unwrap_or(0);
        let mut tz = m.timezone.unwrap_or_default();
        if tz.is_empty()
            && let Some(parsed) = &m.parsed_timezone
        {
            tz = esphome_proto::timezone::posix_tz_from_parsed(&prost::Message::encode_to_vec(
                parsed,
            ));
        }
        info!("api: time from HA: epoch={epoch} tz='{tz}'");
        let _ = id;
        if epoch != 0 {
            self.emit(ApiEvent::Time {
                epoch,
                posix_tz: tz,
            });
        }
    }

    fn on_voice_subscribe(&mut self, id: u64, p: &[u8]) {
        let m: esphome_proto::pb::SubscribeVoiceAssistantRequest = decode(p);
        let sub = m.subscribe.unwrap_or(false);
        let flags = m.flags.unwrap_or(0);
        let Some(c) = self.client(id) else { return };
        let what = if sub { "subscribed" } else { "unsubscribed" };
        info!("api: {} voice assistant {what} (flags={flags})", c.peer);
        let seq = if sub {
            self.voice_seq += 1;
            self.voice_seq
        } else {
            0
        };
        if let Some(c) = self.client(id) {
            c.voice_wanted = seq;
        }
        self.update_voice_owner();
    }

    fn on_voice_response(&mut self, id: u64, p: &[u8]) {
        // Ответы на наши запросы приходят только от владельца пайплайна.
        if self.voice.current() != Some(id) {
            return;
        }
        let m: esphome_proto::pb::VoiceAssistantResponse = decode(p);
        if m.error.unwrap_or(false) {
            warn!("api: HA failed to start the voice pipeline");
            self.emit(ApiEvent::VoiceStartFailed { run: self.run });
        } else {
            let via = if m.port.unwrap_or(0) != 0 {
                "UDP"
            } else {
                "API"
            };
            info!("api: voice pipeline started ({via} audio)");
        }
    }

    fn on_set_key(&mut self, id: u64, p: &[u8]) {
        let m: esphome_proto::pb::NoiseEncryptionSetKeyRequest = decode(p);
        let b64 = m.key.unwrap_or_default();
        let ok = self.set_key(esphome_proto::api_key::trim(&b64));
        // Как C++-сервер: false не пишется на провод (поле отсутствует).
        let resp = esphome_proto::pb::NoiseEncryptionSetKeyResponse {
            success: ok.then_some(true),
        };
        self.send_msg(
            id,
            msg::NOISE_SET_KEY_RESP,
            &prost::Message::encode_to_vec(&resp),
        );
    }

    // ------------------------------------------------------------ teardown

    /// Handle'ы кончились (выход демона): событий отсюда больше нет.
    fn teardown(&mut self) {
        pulsar_core::set_log_sink(None, None);
        for id in self
            .clients
            .iter()
            .filter(|c| c.hello)
            .map(|c| c.id)
            .collect::<Vec<_>>()
        {
            // Best effort: с DisconnectRequest HA считает закрытие ожидаемым и тихо
            // переподключается, когда демон вернётся. Клиентская задача дочитает очередь.
            self.send_msg(id, msg::DISCONNECT_REQ, &[]);
        }
        self.clients.clear();
    }
}

fn decode<M: prost::Message + Default>(p: &[u8]) -> M {
    let mut m = M::default();
    let _ = m.merge(p);
    m
}

/// Клиентская задача: читатель и писатель сокета гоняются в одном `select!`. Чтение не
/// останавливается, пока писатель ждёт медленного клиента: иначе сервер не увидел бы, что
/// клиент не читает, и не закрыл его по лимиту backlog.
async fn run_client(
    transport: Box<dyn Transport>,
    mut out_rx: UnboundedReceiver<Vec<u8>>,
    cmds: Handle<Cmd>,
    id: u64,
    backlog: Rc<Cell<usize>>,
) {
    let (mut read, mut write) = transport.into_split();
    let mut buf = vec![0u8; READ_CHUNK];
    let mut writer = std::pin::pin!(async {
        while let Some(bytes) = out_rx.recv().await {
            if write.write_all(&bytes).await.is_err() {
                return false;
            }
            backlog.set(backlog.get().saturating_sub(bytes.len()));
        }
        let _ = write.shutdown().await;
        true
    });
    // После EOF/ошибки чтения писатель дочитывает очередь: ответы на уже полученные
    // сообщения актор положит раньше, чем закроет клиента.
    loop {
        tokio::select! {
            r = read.read(&mut buf) => match r {
                Ok(0) => {
                    let _ = cmds.send(Cmd::ClientEof { id });
                    writer.await;
                    break;
                }
                Ok(n) => {
                    if !cmds.send(Cmd::ClientData { id, bytes: buf[..n].to_vec() }) {
                        break;
                    }
                }
                Err(e) => {
                    let _ = cmds.send(Cmd::ClientError { id, kind: e.kind() });
                    writer.await;
                    break;
                }
            },
            clean = &mut writer => {
                // Сокет закрыт или актор ушёл: читать больше некуда.
                if !clean {
                    let _ = cmds.send(Cmd::ClientError {
                        id,
                        kind: io::ErrorKind::BrokenPipe,
                    });
                }
                break;
            }
        }
    }
}

const DIAG_PERIOD_MS: u64 = 60_000;

#[cfg(test)]
mod tests;
