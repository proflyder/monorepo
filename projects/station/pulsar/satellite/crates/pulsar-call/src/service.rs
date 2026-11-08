//! Актор звонка: сигналинг + медиа + сборка/выдача аудио.
//!
//! Контроллер владеет микрофоном и кольцом, поэтому uplink приходит сюда готовыми чанками
//! VQE (16 кГц), а downlink уходит в [`Downlink`] (канал `Call` в pulsar-playback). Сам актор
//! не знает ни про HA, ни про модель станции.
//!
//! Порядок звонка: `Incoming` → (сигнал [`RING_MS`]) → `Answer` → offer/answer → медиа →
//! `Active`. Любая ошибка медиа или сигналинга завершает звонок событием `Ended`.
//! Аудио никогда не пишется и не логируется.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use home_actor::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};

use crate::codec::{Codec, OpusCodec, Upsampler};
use crate::jitter::JitterBuffer;
use crate::machine::{Action, CallPhase, Machine, SignalEvent};
use crate::rtc::{Peer, PeerOutput};
use crate::signaling::{SignalError, Signaling, WatchMessage};
use crate::{CallConfig, CallEvent, FRAME_MS, FRAME_SAMPLES, RING_MS};

/// Куда писать downlink. Реализация — pulsar-media: отдельный PCM-канал `Call` с малым
/// буфером, не Music/Announce.
pub trait Downlink: Send + Sync {
    /// Проиграть моно 48 кГц PCM.
    fn write(&self, pcm: &[i16]);
    /// Выбросить всё непригранное (звонок закончился).
    fn flush(&self);
}

enum Cmd {
    Mute(bool),
    Hangup,
    Uplink(Vec<i16>),
}

/// Команды контроллера. Все методы не блокируют; фаза и мут читаются синхронно.
#[derive(Clone)]
pub struct CallHandle {
    cmds: Handle<Cmd>,
    phase: Rc<Cell<CallPhase>>,
    muted: Rc<Cell<bool>>,
    enabled: bool,
}

impl CallHandle {
    /// Модуль включён (есть URL и токен) и готов принимать команды.
    #[must_use]
    pub fn available(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn phase(&self) -> CallPhase {
        self.phase.get()
    }

    #[must_use]
    pub fn muted(&self) -> bool {
        self.muted.get()
    }

    /// Мут микрофона: глушит только uplink, собеседник продолжает звучать.
    pub fn set_muted(&self, muted: bool) {
        self.muted.set(muted);
        let _ = self.cmds.send(Cmd::Mute(muted));
    }

    /// Сбросить звонок (кнопка действия или HA).
    pub fn hangup(&self) {
        let _ = self.cmds.send(Cmd::Hangup);
    }

    /// Чанк очищенного VQE-потока 16 кГц.
    pub fn push_uplink(&self, pcm: &[i16]) {
        if !self.muted.get() {
            let _ = self.cmds.send(Cmd::Uplink(pcm.to_vec()));
        }
    }
}

/// Запустить звонок. Без конфигурации или вывода модуль выключен: handle отвечает `Idle`.
pub fn spawn(
    config: Option<CallConfig>,
    downlink: Option<Arc<dyn Downlink>>,
) -> (CallHandle, UnboundedReceiver<CallEvent>) {
    let (commands, commands_rx) = home_actor::channel();
    let (events, events_rx) = unbounded_channel();
    let phase = Rc::new(Cell::new(CallPhase::Idle));
    let muted = Rc::new(Cell::new(false));
    let (Some(config), Some(downlink)) = (config, downlink) else {
        // Команды никто не читает: канал закрывается вместе с последним handle.
        return (
            CallHandle {
                cmds: commands,
                phase,
                muted,
                enabled: false,
            },
            events_rx,
        );
    };
    let handle = CallHandle {
        cmds: commands,
        phase: Rc::clone(&phase),
        muted: Rc::clone(&muted),
        enabled: true,
    };
    pulsar_core::supervisor().spawn_local(
        "call",
        run(config, downlink, commands_rx, events, Rc::clone(&phase)),
    );
    (handle, events_rx)
}

async fn run(
    config: CallConfig,
    downlink: Arc<dyn Downlink>,
    mut commands: UnboundedReceiver<Cmd>,
    events: UnboundedSender<CallEvent>,
    phase: Rc<Cell<CallPhase>>,
) {
    let signaling = match Signaling::new(&config) {
        Ok(signaling) => signaling,
        Err(error) => {
            warn!("call: disabled: {error}");
            while let Some(command) = commands.recv().await {
                if matches!(command, Cmd::Hangup) {
                    break;
                }
            }
            return;
        }
    };
    let ctx = Ctx {
        signaling: &signaling,
        events: &events,
        phase: &phase,
        downlink: &downlink,
    };
    let mut backoff = Duration::from_secs(1);
    loop {
        match session(&ctx, &mut commands).await {
            Ok(()) => {}
            Err(error) => warn!("call: signaling: {error}"),
        }
        // Обрыв: станция возвращается в обычное состояние, клиент переподключается.
        phase.set(CallPhase::Idle);
        downlink.flush();
        let _ = events.send(CallEvent::Ended);
        tokio::select! {
            () = tokio::time::sleep(backoff) => {}
            command = commands.recv() => {
                if command.is_none() {
                    return;
                }
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

/// Всё, что нужно для побочных эффектов: сигналинг, события и выход звука.
struct Ctx<'a> {
    signaling: &'a Signaling,
    events: &'a UnboundedSender<CallEvent>,
    phase: &'a Rc<Cell<CallPhase>>,
    downlink: &'a Arc<dyn Downlink>,
}

impl Ctx<'_> {
    async fn apply(&self, actions: Vec<Action>, machine: &mut Machine, media: &mut Option<Media>) {
        for action in actions {
            match action {
                Action::Answer { call_id } => {
                    if let Err(error) = self.signaling.answer(&call_id).await {
                        warn!("call: answer failed: {error}");
                        let _ = machine.media_failed();
                        self.teardown(media);
                        self.phase.set(CallPhase::Idle);
                        let _ = self.events.send(CallEvent::Ended);
                    }
                }
                Action::Hangup { call_id, reason } => {
                    if let Err(error) = self.signaling.hangup(&call_id, reason).await {
                        debug!("call: hangup failed: {error}");
                    }
                    self.teardown(media);
                }
                Action::Emit(event) => {
                    self.phase.set(machine.phase());
                    if event == CallEvent::Ended {
                        self.teardown(media);
                    }
                    if event == CallEvent::Active {
                        info!("call: active");
                    }
                    let _ = self.events.send(event);
                }
            }
        }
    }

    fn teardown(&self, media: &mut Option<Media>) {
        if let Some(mut m) = media.take() {
            m.peer.close();
        }
        self.downlink.flush();
    }
}

struct Media {
    peer: Peer,
    codec: OpusCodec,
    jitter: JitterBuffer,
    upsampler: Upsampler,
}

impl Media {
    /// Разобрать накопленное движком: потери/соединение/входящие кадры.
    /// Возвращает `false`, если медиа потеряно.
    fn drive(&mut self, downlink: &Arc<dyn Downlink>, active: bool) -> bool {
        // Сначала время: только `handle_timeout` пакетирует записанные Opus-кадры и
        // отправляет ICE/RTCP. Затем дренаж, который отдаёт `Transmit` в сокет.
        self.peer.handle_timeout(Instant::now());
        let Ok((outputs, _deadline)) = self.peer.drain() else {
            return false;
        };
        for output in outputs {
            match output {
                PeerOutput::Connected => debug!("call: media connected"),
                PeerOutput::Disconnected => {
                    debug!("call: media disconnected");
                    downlink.flush();
                    return false;
                }
                PeerOutput::Audio { data, time } => {
                    let now = pulsar_core::now_ms();
                    if let Some(pcm) = self.codec.decode(&data) {
                        let seq = time / FRAME_SAMPLES as u64;
                        let _ = self.jitter.push(seq, pcm, now);
                    }
                }
            }
        }
        if active {
            let now = pulsar_core::now_ms();
            let codec = &mut self.codec;
            if let Some(pcm) = self.jitter.pop(now, &mut || codec.conceal()) {
                downlink.write(&pcm);
            }
        }
        true
    }

    fn uplink(&mut self, pcm: &[i16]) {
        if !self.peer.is_connected() {
            return;
        }
        self.upsampler.push(pcm);
        while let Some(frame) = self.upsampler.take_frame() {
            if let Some(packet) = self.codec.encode(&frame) {
                let _ = self.peer.write_audio(&packet, Instant::now());
            }
        }
        // Не ждать тика: кадр должен уйти сразу, задержка звонка важнее экономии пакетов.
        self.peer.handle_timeout(Instant::now());
        let _ = self.peer.drain();
    }
}

async fn session(ctx: &Ctx<'_>, commands: &mut UnboundedReceiver<Cmd>) -> Result<(), SignalError> {
    let _ = ctx.signaling.register("Колонка").await;
    let mut watch = ctx.signaling.watch().await?;
    debug!("call: signaling stream open");
    // STUN/TURN внутри LAN/VPN не нужны, но конфигурацию читаем один раз: если сервер её
    // расширит, клиенту не понадобится правка.
    if let Ok(ice) = ctx.signaling.ice_config().await {
        debug!("call: {} ICE servers configured", ice.ice_servers.len());
    }
    let mut machine = Machine::new();
    let mut media: Option<Media> = None;
    let mut ring_deadline: Option<Instant> = None;
    let mut ticker = tokio::time::interval(Duration::from_millis(u64::from(FRAME_MS)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let ring = async {
            match ring_deadline {
                Some(deadline) => {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                }
                None => std::future::pending().await,
            }
        };
        let receiver = media.as_ref().map(|m| m.peer.receiver());
        let incoming = async {
            match &receiver {
                Some(receiver) => receiver.recv().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            event = watch.next() => {
                let Some(message) = event? else {
                    return Err(SignalError::Closed);
                };
                match message {
                    WatchMessage::Call(signal) => {
                        if let SignalEvent::Ringing { .. } = &signal {
                            ring_deadline = Some(Instant::now() + Duration::from_millis(RING_MS));
                        }
                        let actions = machine.on_signal(signal);
                        ctx.apply(actions, &mut machine, &mut media).await;
                        if machine.phase() == CallPhase::Idle {
                            ring_deadline = None;
                        }
                    }
                    WatchMessage::Offer { call_id, sdp } => {
                        match open_media(sdp, call_id).await {
                            Ok((peer, answer, call_id)) => {
                                ctx.signaling.send_sdp(&call_id, answer).await?;
                                let _ = ctx.signaling.send_ice_end(&call_id).await;
                                if let Some(m) = new_media(peer, call_id) {
                                    media = Some(m);
                                } else {
                                    warn!("call: opus codec unavailable");
                                    let actions = machine.media_failed();
                                    ctx.apply(actions, &mut machine, &mut media).await;
                                }
                            }
                            Err(error) => {
                                debug!("call: media negotiation failed: {error}");
                                let actions = machine.media_failed();
                                ctx.apply(actions, &mut machine, &mut media).await;
                            }
                        }
                    }
                    WatchMessage::Ice { candidate, .. } => {
                        if let Some(m) = &mut media
                            && m.peer.add_remote_candidate(&candidate).is_err()
                        {
                            debug!("call: rejected remote ICE candidate");
                        }
                    }
                    WatchMessage::IceEnd { .. } | WatchMessage::Other => {}
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(()) };
                match command {
                    Cmd::Mute(m) => machine.set_muted(m),
                    Cmd::Uplink(pcm) => {
                        if machine.phase() == CallPhase::Active
                            && !machine.muted()
                            && let Some(m) = &mut media
                        {
                            m.uplink(&pcm);
                        }
                    }
                    Cmd::Hangup => {
                        let actions = machine.local_hangup();
                        ctx.apply(actions, &mut machine, &mut media).await;
                        ring_deadline = None;
                    }
                }
            }
            () = ring, if ring_deadline.is_some() => {
                ring_deadline = None;
                let actions = machine.ring_elapsed();
                ctx.apply(actions, &mut machine, &mut media).await;
            }
            packet = incoming, if media.is_some() => {
                if let (Ok((packet, source)), Some(m)) = (packet, media.as_mut()) {
                    m.peer.handle_input(&packet, source);
                    let active = machine.phase() == CallPhase::Active;
                    if !m.drive(ctx.downlink, active) {
                        let actions = machine.media_failed();
                        ctx.apply(actions, &mut machine, &mut media).await;
                    }
                }
            }
            _ = ticker.tick() => {
                let active = machine.phase() == CallPhase::Active;
                let lost = media
                    .as_mut()
                    .is_some_and(|m| !m.drive(ctx.downlink, active));
                if lost {
                    let actions = machine.media_failed();
                    ctx.apply(actions, &mut machine, &mut media).await;
                }
            }
        }
    }
}

async fn open_media(
    sdp: String,
    call_id: String,
) -> Result<(Peer, String, String), crate::rtc::PeerError> {
    let mut peer = Peer::bind().await?;
    let answer = peer.accept_offer(&sdp)?;
    Ok((peer, answer, call_id))
}

fn new_media(peer: Peer, _call_id: String) -> Option<Media> {
    debug!(
        "call: media opened (local port {}, jitter target {} ms)",
        peer.local_addr().port(),
        JitterBuffer::new(FRAME_MS).target_ms()
    );
    Some(Media {
        peer,
        codec: OpusCodec::new()?,
        jitter: JitterBuffer::new(FRAME_MS),
        upsampler: Upsampler::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDownlink {
        written: Mutex<Vec<i16>>,
        flushed: Mutex<u32>,
    }

    impl Downlink for FakeDownlink {
        fn write(&self, pcm: &[i16]) {
            self.written.lock().unwrap().extend_from_slice(pcm);
        }

        fn flush(&self) {
            *self.flushed.lock().unwrap() += 1;
        }
    }

    #[test]
    fn a_disabled_handle_never_reports_a_call() {
        let (handle, _events) = spawn(None, None);
        assert!(!handle.available());
        assert_eq!(handle.phase(), CallPhase::Idle);
        // Команды принимаются и не паникуют.
        handle.set_muted(true);
        handle.hangup();
        handle.push_uplink(&[0; 32]);
        assert_eq!(handle.phase(), CallPhase::Idle);
    }

    #[test]
    fn the_handle_tracks_mute_and_uplink_immediately() {
        let (handle, _events) = spawn(None, None);
        assert!(!handle.muted());
        handle.set_muted(true);
        assert!(handle.muted());
        handle.set_muted(false);
        assert!(!handle.muted());
    }

    #[test]
    fn the_fake_downlink_records_pcm_and_flushes() {
        let sink = Arc::new(FakeDownlink::default());
        sink.write(&[1, 2, 3]);
        sink.flush();
        assert_eq!(*sink.written.lock().unwrap(), vec![1, 2, 3]);
        assert_eq!(*sink.flushed.lock().unwrap(), 1);
    }
}
