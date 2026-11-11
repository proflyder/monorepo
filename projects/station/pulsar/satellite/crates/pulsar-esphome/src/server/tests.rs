//! Сервер на `LocalSet` с клиентами через `tokio::io::duplex`: golden-захват C++-референса,
//! plaintext и Noise, владение голосом и вытеснение, backlog, ключ, таймауты.

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use esphome_proto::frame::{
    NOISE_PROTOCOL, NoiseFrame, Psk, append_noise_raw, append_plain, parse_noise,
};
use esphome_proto::pb::msg;
use esphome_proto::pb::*;
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::LocalSet;
use tokio::time::{Instant, timeout};
use tracing::Level;

use super::*;
use crate::key::Key;
use crate::transport::{ChannelListener, Incoming, Listener};
use crate::types::*;
use crate::{ApiHandle, Capabilities};

const KEY_B64: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
const RAW: &str = include_str!("../../tests/golden/selftest_raw.txt");

fn identity() -> DeviceIdentity {
    DeviceIdentity {
        name: "yandex-midi-test".into(),
        friendly_name: "Станция".into(),
        mac: "AA:BB:CC:DD:EE:FF".into(),
        model: "Yandex Station Midi".into(),
        board: "cucumber".into(),
        manufacturer: "Yandex".into(),
        project_version: "v1.2.3".into(),
        compilation_time: "Jan  1 2026, 00:00:00".into(),
    }
}

fn all_caps() -> Capabilities {
    Capabilities {
        clock: true,
        lux: true,
        auto_brightness: true,
        wake_preroll: true,
        hw_mic_mute: true,
        buttons: true,
        case_colors: true,
        voice_colors: true,
        zigbee: true,
    }
}

fn selftest_config(key_path: Option<PathBuf>) -> ApiConfig {
    let s = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let ww = |id: &str, phrase: &str| WakeWordInfo {
        id: id.into(),
        phrase: phrase.into(),
        languages: s(&["ru"]),
    };
    ApiConfig {
        port: 0,
        capabilities: all_caps(),
        wake_words: vec![ww("alisa", "Алиса"), ww("yandex", "Яндекс")],
        active_wake_words: s(&["alisa"]),
        light_effects: s(&["Rainbow", "Pulse"]),
        clock_faces: s(&["None", "Smiley"]),
        music_styles: s(&["Off", "All styles", "Spectrum"]),
        case_colors: s(&["None", "blue"]),
        clock_music_styles: s(&["Off", "Levels"]),
        voice_colors: s(&["Violet", "Blue"]),
        zigbee_modes: s(&["Coordinator", "Router"]),
        key_path,
    }
}

fn config(key_path: Option<PathBuf>) -> ApiConfig {
    ApiConfig {
        port: 0,
        key_path,
        ..ApiConfig::default()
    }
}

/// Временный каталог, удаляется на drop.
struct TmpDir(PathBuf);

impl TmpDir {
    fn new(tag: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("pulsar-esphome-srv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct TestServer {
    api: ApiHandle,
    events: UnboundedReceiver<ApiEvent>,
    tx: UnboundedSender<Incoming>,
}

impl TestServer {
    fn start(cfg: ApiConfig) -> Self {
        let (listener, tx) = ChannelListener::new();
        let (api, events) = spawn_with(Box::new(listener), identity(), cfg).unwrap();
        Self { api, events, tx }
    }

    fn take_events(&mut self) -> UnboundedReceiver<ApiEvent> {
        let (_, rx) = unbounded_channel();
        std::mem::replace(&mut self.events, rx)
    }

    fn connect(&self) -> Peer {
        self.connect_buf(1 << 20)
    }

    /// Соединение с маленьким буфером: клиент, который не читает, упирается в него быстро.
    fn connect_buf(&self, capacity: usize) -> Peer {
        let (server, client) = tokio::io::duplex(capacity);
        self.tx
            .send((Box::new(server), "test".into()))
            .expect("listener gone");
        Peer::with(client)
    }

    /// Noise-клиент: server hello, обмен NNpsk0; Err — текст отказа сервера.
    async fn connect_noise(&self, psk: &Psk) -> Result<Peer, String> {
        let (server, client) = tokio::io::duplex(1 << 20);
        self.tx
            .send((Box::new(server), "noise".into()))
            .expect("listener gone");
        let mut p = Peer::with(client);
        let (init, hello) = Initiator::new(psk);
        p.write(&hello).await;
        let server_hello = p.raw_noise().await.expect("server hello");
        assert_eq!(
            server_hello,
            format!("\x01{}\0{}\0", identity().name, "aabbccddeeff").as_bytes()
        );
        p.noise = Some(init);
        let body = p.raw_noise().await.expect("handshake reply");
        p.noise.as_mut().unwrap().finish(&body)?;
        Ok(p)
    }
}

struct Peer {
    s: DuplexStream,
    rbuf: Vec<u8>,
    noise: Option<Initiator>,
    wait: Duration,
}

impl Peer {
    fn with(s: DuplexStream) -> Self {
        Self {
            s,
            rbuf: Vec::new(),
            noise: None,
            wait: Duration::from_secs(10),
        }
    }

    async fn write(&mut self, b: &[u8]) {
        self.s.write_all(b).await.unwrap();
    }

    async fn send(&mut self, msg_type: u32, payload: &[u8]) {
        let bytes = self.noise.as_mut().map_or_else(
            || {
                let mut v = Vec::new();
                append_plain(&mut v, msg_type, payload);
                v
            },
            |n| n.encrypt(msg_type as u16, payload),
        );
        self.write(&bytes).await;
    }

    async fn send_msg<M: Message>(&mut self, msg_type: u32, m: &M) {
        self.send(msg_type, &m.encode_to_vec()).await;
    }

    async fn fill(&mut self) -> bool {
        self.rbuf.reserve(65536);
        timeout(self.wait, self.s.read_buf(&mut self.rbuf))
            .await
            .expect("timed out waiting for the server")
            .unwrap_or(0)
            > 0
    }

    async fn raw_noise(&mut self) -> Option<Vec<u8>> {
        loop {
            if !self.rbuf.is_empty()
                && let NoiseFrame::Frame { body, used } = parse_noise(&self.rbuf, false)
            {
                let body = body.to_vec();
                self.rbuf.drain(..used);
                return Some(body);
            }
            if !self.fill().await {
                return None;
            }
        }
    }

    async fn recv(&mut self) -> Option<(u32, Vec<u8>)> {
        loop {
            if !self.rbuf.is_empty() {
                match &mut self.noise {
                    Some(n) => {
                        if let NoiseFrame::Frame { body, used } = parse_noise(&self.rbuf, false) {
                            let (t, m) = n.decrypt(body);
                            self.rbuf.drain(..used);
                            return Some((u32::from(t), m));
                        }
                    }
                    None => {
                        if let esphome_proto::frame::Plain::Frame {
                            msg_type,
                            msg,
                            used,
                        } = esphome_proto::frame::parse_plain(&self.rbuf)
                        {
                            let m = (msg_type, msg.to_vec());
                            self.rbuf.drain(..used);
                            return Some(m);
                        }
                    }
                }
            }
            if !self.fill().await {
                return None;
            }
        }
    }

    async fn expect(&mut self, msg_type: u32) -> Vec<u8> {
        let (t, m) = self.recv().await.expect("connection closed");
        assert_eq!(t, msg_type, "message {m:?}");
        m
    }

    async fn hello(&mut self, info: &str) -> Vec<u8> {
        let req = HelloRequest {
            client_info: Some(info.into()),
            api_version_major: Some(1),
            api_version_minor: Some(10),
        };
        self.send_msg(msg::HELLO_REQ, &req).await;
        let resp = self.expect(msg::HELLO_RESP).await;
        self.expect(msg::GET_TIME_REQ).await;
        resp
    }

    async fn subscribe_voice(&mut self, on: bool) {
        self.send_msg(
            msg::SUBSCRIBE_VA_REQ,
            &SubscribeVoiceAssistantRequest {
                subscribe: Some(on),
                flags: Some(0),
            },
        )
        .await;
    }

    async fn sync(&mut self) -> Vec<(u32, Vec<u8>)> {
        self.send(msg::PING_REQ, &[]).await;
        let mut before = Vec::new();
        loop {
            let m = self.recv().await.expect("connection closed");
            if m.0 == msg::PING_RESP {
                return before;
            }
            before.push(m);
        }
    }

    async fn closed(&mut self) -> Vec<u8> {
        while self.fill().await {}
        std::mem::take(&mut self.rbuf)
    }
}

struct Initiator {
    hs: Option<snow::HandshakeState>,
    ts: Option<snow::TransportState>,
}

impl Initiator {
    fn new(psk: &Psk) -> (Self, Vec<u8>) {
        let mut prologue = b"NoiseAPIInit".to_vec();
        prologue.extend_from_slice(&[0, 0]);
        let mut hs = snow::Builder::new(NOISE_PROTOCOL.parse().unwrap())
            .psk(0, psk.as_bytes())
            .unwrap()
            .prologue(&prologue)
            .unwrap()
            .build_initiator()
            .unwrap();
        let mut msg = [0u8; 128];
        msg[0] = 0x00;
        let n = hs.write_message(&[], &mut msg[1..]).unwrap();
        let mut out = Vec::new();
        append_noise_raw(&mut out, &[]);
        append_noise_raw(&mut out, &[&msg[..n + 1]]);
        (
            Self {
                hs: Some(hs),
                ts: None,
            },
            out,
        )
    }

    fn finish(&mut self, body: &[u8]) -> Result<(), String> {
        if body.first() != Some(&0x00) {
            return Err(String::from_utf8_lossy(&body[1..]).into_owned());
        }
        let mut hs = self.hs.take().unwrap();
        let mut payload = [0u8; 128];
        hs.read_message(&body[1..], &mut payload)
            .map_err(|e| e.to_string())?;
        self.ts = Some(hs.into_transport_mode().unwrap());
        Ok(())
    }

    fn encrypt(&mut self, msg_type: u16, msg: &[u8]) -> Vec<u8> {
        let mut plain = msg_type.to_be_bytes().to_vec();
        plain.extend_from_slice(&(msg.len() as u16).to_be_bytes());
        plain.extend_from_slice(msg);
        let mut ct = vec![0u8; plain.len() + 16];
        let n = self
            .ts
            .as_mut()
            .unwrap()
            .write_message(&plain, &mut ct)
            .unwrap();
        let mut out = Vec::new();
        append_noise_raw(&mut out, &[&ct[..n]]);
        out
    }

    fn decrypt(&mut self, body: &[u8]) -> (u16, Vec<u8>) {
        let mut plain = vec![0u8; body.len()];
        let n = self
            .ts
            .as_mut()
            .unwrap()
            .read_message(body, &mut plain)
            .unwrap();
        let t = u16::from_be_bytes([plain[0], plain[1]]);
        let len = usize::from(u16::from_be_bytes([plain[2], plain[3]]));
        assert_eq!(len, n - 4);
        (t, plain[4..n].to_vec())
    }
}

async fn event(rx: &mut UnboundedReceiver<ApiEvent>) -> ApiEvent {
    timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("no event")
        .expect("server gone")
}

fn local() -> LocalSet {
    LocalSet::new()
}

fn unhex(s: &str) -> Vec<u8> {
    if s == "-" {
        return Vec::new();
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    if b.is_empty() {
        return "-".into();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Clone, Copy, Debug)]
struct Field<'a> {
    v: u64,
    data: &'a [u8],
}

impl Field<'_> {
    fn u32(self) -> u32 {
        self.v as u32
    }
    fn flt(self) -> f32 {
        f32::from_bits(self.v as u32)
    }
    fn boolean(self) -> bool {
        self.v != 0
    }
    fn str(self) -> String {
        String::from_utf8_lossy(self.data).into_owned()
    }
}

fn field(p: &[u8], num: u32) -> Option<Field<'_>> {
    let mut pos = 0;
    while pos < p.len() {
        let key = read_varint(p, &mut pos)?;
        let f = (key >> 3) as u32;
        match key & 7 {
            0 => {
                let v = read_varint(p, &mut pos)?;
                if f == num {
                    return Some(Field { v, data: &[] });
                }
            }
            2 => {
                let len = read_varint(p, &mut pos)? as usize;
                let data = p.get(pos..pos + len)?;
                pos += len;
                if f == num {
                    return Some(Field { v: 0, data });
                }
            }
            5 => {
                let data = p.get(pos..pos + 4)?;
                pos += 4;
                if f == num {
                    return Some(Field {
                        v: u64::from(u32::from_le_bytes(data.try_into().ok()?)),
                        data,
                    });
                }
            }
            _ => return None,
        }
    }
    None
}

fn read_varint(p: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..70).step_by(7) {
        let b = *p.get(*pos)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn publish_selftest_states(h: &ApiHandle) {
    h.publish_media(MediaState::Idle, 0.5, false);
    h.publish_switch(SwitchId::Microphones, true);
    h.publish_switch(SwitchId::Clock, true);
    h.publish_switch(SwitchId::VoiceIndication, true);
    h.publish_number(NumberId::LedSpeed, 1.0);
    h.publish_select(SelectId::MusicViz, "All styles");
    h.publish_select(SelectId::CaseColor, "None");
    h.publish_select(SelectId::ClockMusic, "Off");
    h.publish_binary_sensor(BinarySensorId::HwMicMute, false);
    h.publish_number(NumberId::ClockBrightness, 50.0);
    h.publish_light(&LightState {
        on: true,
        brightness: 0.8,
        r: 0.0,
        g: 0.5,
        b: 1.0,
        effect: String::new(),
    });
    h.publish_number(NumberId::ClockScroll, 400.0);
    h.publish_text(TextId::ClockText, "");
    h.publish_select(SelectId::ClockFace, "None");
    h.publish_select(SelectId::ZigbeeMode, "Coordinator");
    h.publish_text_sensor(TextSensorId::Zigbee, "coordinator: PAN 0x1234, channel 15");
}

fn fork_only(t: u32, m: &[u8]) -> bool {
    const KEYS: [u32; 7] = [36, 37, 34, 35, 38, 40, 41];
    let key = match t {
        msg::LIST_SWITCH | msg::LIST_BUTTON | msg::LIST_SELECT | msg::LIST_NUMBER => {
            field(m, 2).map(Field::u32)
        }
        msg::SWITCH_STATE | msg::SELECT_STATE | msg::NUMBER_STATE => field(m, 1).map(Field::u32),
        _ => None,
    };
    matches!(
        t,
        msg::LIST_SWITCH
            | msg::LIST_BUTTON
            | msg::LIST_SELECT
            | msg::LIST_NUMBER
            | msg::SWITCH_STATE
            | msg::SELECT_STATE
            | msg::NUMBER_STATE
    ) && key.is_some_and(|k| KEYS.contains(&k))
}

#[tokio::test]
async fn selftest_matches_cxx_golden_bytes() {
    let golden: Vec<(u32, Vec<u8>)> = RAW
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let (t, h) = l.split_once(' ').unwrap();
            (t.parse().unwrap(), unhex(h))
        })
        .collect();
    let info = &golden
        .iter()
        .find(|(t, _)| *t == msg::DEVICE_INFO_RESP)
        .unwrap()
        .1;
    let built = field(info, 5).unwrap().str();
    local()
        .run_until(async {
            let cfg = selftest_config(None);
            let id = DeviceIdentity {
                name: "pulsar-selftest".into(),
                friendly_name: "Яндекс Станция Миди (selftest)".into(),
                mac: "02:00:00:C0:FF:EE".into(),
                model: "Yandex Station Midi".into(),
                board: "cucumber".into(),
                manufacturer: "Yandex".into(),
                project_version: "0.1.0".into(),
                compilation_time: built,
            };
            let (listener, _tx) = ChannelListener::new();
            let (h, _rx) = spawn_with(Box::new(listener), id, cfg).unwrap();
            publish_selftest_states(&h);
            let (server, client) = tokio::io::duplex(1 << 20);
            _tx.send((Box::new(server), "test".into())).unwrap();
            let mut p = Peer::with(client);
            p.hello("raw").await;
            let mut got = Vec::new();
            for req in [
                msg::DEVICE_INFO_REQ,
                msg::DEVICE_CAPS_REQ,
                msg::VA_CONFIG_REQ,
            ] {
                p.send(req, &[]).await;
                got.push(p.recv().await.unwrap());
            }
            p.send(msg::LIST_ENTITIES_REQ, &[]).await;
            loop {
                let m = p.recv().await.unwrap();
                let done = m.0 == msg::LIST_DONE;
                got.push(m);
                if done {
                    break;
                }
            }
            p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
            got.extend(p.sync().await);
            // Hello и GetTimeRequest уже сверены p.hello; golden начинается с них.
            got.retain(|(t, m)| !fork_only(*t, m));
            // Первые два golden — HelloResponse и GetTimeRequest; сравнение с ними отдельно.
            assert_eq!(golden[0].0, msg::HELLO_RESP);
            assert_eq!(golden[1].0, msg::GET_TIME_REQ);
            assert_eq!(got.len(), golden.len() - 2);
            for (i, (g, r)) in golden[2..].iter().zip(&got).enumerate() {
                assert_eq!(g.0, r.0, "сообщение {i}");
                let host_dependent = matches!(
                    (g.0, field(&g.1, 1).map(Field::u32)),
                    (msg::SENSOR_STATE, Some(10 | 11)) | (msg::TEXT_SENSOR_STATE, Some(12))
                );
                if host_dependent {
                    assert_eq!(
                        field(&g.1, 1).map(Field::u32),
                        field(&r.1, 1).map(Field::u32),
                        "ключ диагностики {i}"
                    );
                } else {
                    assert_eq!(hex(&g.1), hex(&r.1), "сообщение {i}, тип {}", g.0);
                }
            }
        })
        .await;
}

#[test]
fn txt_records_follow_the_key_state() {
    let id = identity();
    let base = [
        "friendly_name=Станция",
        "version=2026.9.0",
        "mac=aabbccddeeff",
        "platform=host",
        "board=cucumber",
        "network=wifi",
        "project_name=pulsar.satellite",
        "project_version=v1.2.3",
    ];
    assert_eq!(txt_records(&id, false, false), base);
    let mut with = base.map(String::from).to_vec();
    with.push("api_encryption_supported=Noise_NNpsk0_25519_ChaChaPoly_SHA256".into());
    assert_eq!(txt_records(&id, true, false), with);
    with[8] = "api_encryption=Noise_NNpsk0_25519_ChaChaPoly_SHA256".into();
    assert_eq!(txt_records(&id, true, true), with);
    assert_eq!(mac_plain("0A:1b:FF"), "0a1bff");
}

#[test]
fn log_levels_are_esphome() {
    assert_eq!(esphome_level(Level::ERROR), 1);
    assert_eq!(esphome_level(Level::WARN), 2);
    assert_eq!(esphome_level(Level::INFO), 3);
    assert_eq!(esphome_level(Level::DEBUG), 5);
}

#[test]
fn key_load_fails_closed() {
    let dir = TmpDir::new("load");
    let path = dir.path().join("api_key");
    assert_eq!(key::load_key(None), Key::default());
    assert_eq!(key::load_key(Some(&path)), Key::default());
    std::fs::write(&path, "junk").unwrap();
    let k = key::load_key(Some(&path));
    assert!(k.has_key && !k.reject_all);
    std::fs::write(&path, format!("{KEY_B64}\n")).unwrap();
    assert!(key::load_key(Some(&path)).has_key);
}

#[tokio::test]
async fn plaintext_session() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            assert!(!h.connected());
            assert_eq!(h.mdns_txt().len(), 8);
            let mut p = ts.connect();
            let resp = p.hello("Home Assistant 2026.10").await;
            assert_eq!(field(&resp, 1).unwrap().u32(), 1);
            assert_eq!(field(&resp, 2).unwrap().u32(), 15);
            assert_eq!(field(&resp, 3).unwrap().str(), "pulsar v1.2.3");
            assert_eq!(field(&resp, 4).unwrap().str(), identity().name);
            assert_eq!(
                event(&mut rx).await,
                ApiEvent::ClientConnected {
                    client_info: "Home Assistant 2026.10".into()
                }
            );
            assert!(h.connected());

            // GetTimeResponse: строка TZ главнее разобранной зоны, та — запасной вариант.
            let parsed = ParsedTimezone {
                std_offset_seconds: Some(-10_800),
                dst_start: None,
                dst_end: None,
            };
            p.send_msg(
                msg::GET_TIME_RESP,
                &GetTimeResponse {
                    epoch_seconds: Some(1_700_000_000),
                    timezone: None,
                    parsed_timezone: Some(parsed.clone()),
                },
            )
            .await;
            p.send_msg(
                msg::GET_TIME_RESP,
                &GetTimeResponse {
                    epoch_seconds: Some(1_700_000_001),
                    timezone: Some("MSK-3".into()),
                    parsed_timezone: Some(parsed),
                },
            )
            .await;
            // Epoch 0 — не время.
            p.send_msg(
                msg::GET_TIME_RESP,
                &GetTimeResponse {
                    epoch_seconds: None,
                    timezone: Some("UTC0".into()),
                    parsed_timezone: None,
                },
            )
            .await;
            // Неизвестные сообщения игнорируются, сессия продолжается.
            p.send(200, &[]).await;
            p.send(msg::PING_RESP, &[]).await;
            assert!(p.sync().await.is_empty());
            assert_eq!(
                event(&mut rx).await,
                ApiEvent::Time {
                    epoch: 1_700_000_000,
                    posix_tz: "<+0300>-3".into()
                }
            );
            assert_eq!(
                event(&mut rx).await,
                ApiEvent::Time {
                    epoch: 1_700_000_001,
                    posix_tz: "MSK-3".into()
                }
            );
            assert!(rx.try_recv().is_err());

            // Второй hello на том же соединении получает ответ, но никого не подключает.
            p.send_msg(
                msg::HELLO_REQ,
                &HelloRequest {
                    client_info: Some("again".into()),
                    api_version_major: Some(1),
                    api_version_minor: Some(10),
                },
            )
            .await;
            p.expect(msg::HELLO_RESP).await;
            assert!(p.sync().await.is_empty());
            assert!(rx.try_recv().is_err());

            p.send_msg(msg::DISCONNECT_REQ, &DisconnectRequest {}).await;
            p.expect(msg::DISCONNECT_RESP).await;
            assert!(p.closed().await.is_empty());
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
            assert!(!h.connected());

            // Собственный DisconnectRequest сервера клиент подтверждает.
            let mut p = ts.connect();
            p.hello("probe").await;
            event(&mut rx).await;
            p.send_msg(msg::DISCONNECT_RESP, &DisconnectResponse {})
                .await;
            assert!(p.closed().await.is_empty());
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
        })
        .await;
}

#[tokio::test]
async fn protocol_violations_drop_the_client() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let mut rx = ts.take_events();
            // Ping и disconnect до hello разрешены, остальное — нет.
            let mut p = ts.connect();
            assert!(p.sync().await.is_empty());
            p.subscribe_voice(true).await;
            assert!(p.closed().await.is_empty());

            for garbage in [&b"\x05hello"[..], b"\x00\x81\x80\x80\x01\x01"] {
                let mut p = ts.connect();
                p.write(garbage).await;
                assert!(p.closed().await.is_empty(), "{garbage:?}");
            }
            // Заголовок, который никогда не кончится.
            let mut p = ts.connect();
            p.write(&[0x00]).await;
            p.write(&[0xff; 21]).await;
            assert!(p.closed().await.is_empty());
            // EOF посреди кадра.
            let mut p = ts.connect();
            p.write(b"\x00\x05\x01ab").await;
            p.s.shutdown().await.unwrap();
            assert!(p.closed().await.is_empty());
            // Битый payload логируется, сообщение всё равно обработано.
            let mut p = ts.connect();
            p.write(b"\x00\x03\x01\x0a\x05x").await;
            p.expect(msg::HELLO_RESP).await;
            assert!(matches!(
                event(&mut rx).await,
                ApiEvent::ClientConnected { .. }
            ));
        })
        .await;
}

#[tokio::test]
async fn frames_split_across_reads_are_reassembled() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            let mut frame = Vec::new();
            append_plain(
                &mut frame,
                msg::HELLO_REQ,
                &HelloRequest {
                    client_info: Some("split".into()),
                    api_version_major: Some(1),
                    api_version_minor: Some(10),
                }
                .encode_to_vec(),
            );
            // TCP режет кадр произвольно: хвост первого чтения — начало второго.
            let (head, tail) = frame.split_at(4);
            p.write(head).await;
            tokio::task::yield_now().await;
            p.write(tail).await;
            p.expect(msg::HELLO_RESP).await;
            p.expect(msg::GET_TIME_REQ).await;
            assert_eq!(
                event(&mut rx).await,
                ApiEvent::ClientConnected {
                    client_info: "split".into()
                }
            );
        })
        .await;
}

#[tokio::test]
async fn pending_connections_are_capped() {
    local()
        .run_until(async {
            let ts = TestServer::start(config(None));
            let mut pending = Vec::new();
            for _ in 0..MAX_PENDING {
                pending.push(ts.connect());
            }
            let mut probe = pending.pop().unwrap();
            assert!(probe.sync().await.is_empty());
            let mut newest = ts.connect();
            assert!(newest.sync().await.is_empty());
            assert!(pending[0].closed().await.is_empty());
            for p in &mut pending[1..] {
                assert!(p.sync().await.is_empty());
            }
            assert!(probe.sync().await.is_empty());
        })
        .await;
}

#[tokio::test]
async fn voice_owner_hands_over() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut a = ts.connect();
            a.hello("HA").await;
            event(&mut rx).await;
            assert_eq!(h.voice_request_start("Алиса"), 1);
            a.subscribe_voice(true).await;
            a.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            assert!(h.voice_subscribed());

            // Более свежий подписчик забирает пайплайн: false перед true.
            let mut b = ts.connect();
            b.hello("probe").await;
            event(&mut rx).await;
            b.subscribe_voice(true).await;
            b.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            assert!(h.voice_subscribed());

            assert_eq!(h.voice_request_start("Алиса"), 2);
            let req = b.expect(msg::VA_REQUEST).await;
            assert!(field(&req, 1).unwrap().boolean());
            assert_eq!(field(&req, 5).unwrap().str(), "Алиса");
            let audio = field(&req, 4).unwrap();
            assert_eq!(field(audio.data, 3).unwrap().flt(), 1.0);
            h.voice_send_audio(vec![1, -2]);
            let m = b.expect(msg::VA_AUDIO).await;
            assert_eq!(field(&m, 1).unwrap().data, [1, 0, 0xfe, 0xff]);
            // Мягкий конец: VoiceAssistantAudio.end = true без данных, не stop-запрос.
            h.voice_send_audio_end();
            let m = b.expect(msg::VA_AUDIO).await;
            assert!(field(&m, 1).is_none());
            assert!(field(&m, 2).unwrap().boolean());
            h.voice_announce_finished(true);
            let m = b.expect(msg::VA_ANNOUNCE_FINISHED).await;
            assert!(field(&m, 1).unwrap().boolean());
            h.voice_request_stop();
            assert!(b.expect(msg::VA_REQUEST).await.is_empty());
            // Старый владелец не получил ничего.
            assert!(a.sync().await.is_empty());

            // HA отвечает за тот run, который её попросили начать; чужие ответы игнорируются.
            a.send_msg(
                msg::VA_RESPONSE,
                &VoiceAssistantResponse {
                    port: None,
                    error: Some(true),
                },
            )
            .await;
            a.sync().await;
            b.send_msg(
                msg::VA_RESPONSE,
                &VoiceAssistantResponse {
                    port: None,
                    error: Some(true),
                },
            )
            .await;
            b.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceStartFailed { run: 2 });
            assert!(rx.try_recv().is_err());

            // Новый клиент уходит: пайплайн возвращается предыдущему подписчику.
            drop(b);
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            h.voice_request_start("");
            let req = a.expect(msg::VA_REQUEST).await;
            assert!(field(&req, 5).is_none());

            // Отписка отдаёт его окончательно.
            a.subscribe_voice(false).await;
            a.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert!(!h.voice_subscribed() && h.connected());
            assert!(rx.try_recv().is_err());
            // Без владельца голосовые вызовы ничего не шлют.
            h.voice_request_start("x");
            h.voice_send_audio(vec![1]);
            h.voice_send_audio_end();
            h.voice_request_stop();
            assert!(a.sync().await.is_empty());

            // Владелец отключается: false, затем последний клиент ушёл.
            a.subscribe_voice(true).await;
            a.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            drop(a);
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
            assert!(!h.voice_subscribed() && !h.connected());
        })
        .await;
}

#[tokio::test]
async fn full_server_evicts_the_least_useful_client() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let mut rx = ts.take_events();
            let mut peers = Vec::new();
            for name in ["owner", "idle", "subscriber", "chatty"] {
                let mut p = ts.connect();
                p.hello(name).await;
                event(&mut rx).await;
                peers.push(p);
            }
            peers[0].subscribe_voice(true).await;
            peers[0].sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            // «subscriber» тоже хочет пайплайн, поэтому «owner» подписывается снова.
            peers[2].subscribe_voice(true).await;
            peers[2].sync().await;
            peers[0].subscribe_voice(true).await;
            peers[0].sync().await;
            for _ in 0..4 {
                event(&mut rx).await;
            }
            peers[3].sync().await;

            // Пятый клиент: уходит «idle» (без интереса к голосу, дольше всех молчит).
            let mut e = ts.connect();
            e.hello("new").await;
            assert!(matches!(
                event(&mut rx).await,
                ApiEvent::ClientConnected { .. }
            ));
            assert!(peers[1].closed().await.is_empty());
            for i in [0, 2, 3] {
                peers[i].sync().await;
            }

            // Дальше — подписчик, не владеющий пайплайном; владелец не уходит никогда.
            peers[3].subscribe_voice(true).await;
            peers[3].sync().await;
            peers[0].subscribe_voice(true).await;
            peers[0].sync().await;
            for _ in 0..4 {
                event(&mut rx).await;
            }
            e.subscribe_voice(true).await;
            e.sync().await;
            peers[0].subscribe_voice(true).await;
            peers[0].sync().await;
            for _ in 0..4 {
                event(&mut rx).await;
            }
            let mut f = ts.connect();
            f.hello("newer").await;
            event(&mut rx).await;
            // «subscriber» (ранг 1) слышали раньше «chatty» и «new».
            assert!(peers[2].closed().await.is_empty());
            peers[0].sync().await;
            assert!(rx.try_recv().is_err());
        })
        .await;
}

#[tokio::test]
async fn audio_backlog_ends_the_run() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            // Маленький буфер: ядро не поглощает поток, клиент его не читает.
            let mut p = ts.connect_buf(4096);
            p.hello("slow").await;
            event(&mut rx).await;
            p.subscribe_voice(true).await;
            p.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            let run = h.voice_request_start("Алиса");

            let chunk = vec![7i16; 16384];
            let mut sent = 0;
            let overflow = loop {
                h.voice_send_audio(chunk.clone());
                sent += 1;
                tokio::task::yield_now().await;
                if let Ok(ev) = rx.try_recv() {
                    break ev;
                }
                assert!(sent < 2000, "no overflow");
            };
            assert_eq!(overflow, ApiEvent::VoiceAudioOverflow { run });
            assert!(h.voice_subscribed());
            // Отброшено до следующего start.
            h.voice_send_audio(chunk.clone());

            let before = p.sync().await;
            let (stop, audio) = before.split_last().unwrap();
            assert_eq!(stop, &(msg::VA_REQUEST, Vec::new()));
            assert_eq!(audio[0].0, msg::VA_REQUEST);
            assert!(audio[1..].iter().all(|m| m.0 == msg::VA_AUDIO));
            assert!(audio.len() - 1 < sent);

            let run2 = h.voice_request_start("");
            assert_eq!(run2, run + 1);
            h.voice_send_audio(vec![1; 4]);
            let after = p.sync().await;
            assert_eq!(after.len(), 2);
            assert_eq!(after[1].0, msg::VA_AUDIO);
            assert!(rx.try_recv().is_err());
        })
        .await;
}

#[tokio::test]
async fn client_not_reading_is_dropped() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut p = ts.connect_buf(4096);
            p.hello("stuck").await;
            event(&mut rx).await;
            p.subscribe_voice(true).await;
            p.sync().await;
            event(&mut rx).await;
            h.voice_request_start("");
            // Одно сообщение больше 8 MiB backlog ещё до отправки: клиента отдают.
            h.voice_send_audio(vec![0; (MAX_BACKLOG / 2) + (1 << 20)]);
            // I/O делает актор: сразу после вызова геттеры не изменились.
            assert!(h.connected() && h.voice_subscribed());
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
            assert!(!h.connected() && !h.voice_subscribed());
            assert!(p.closed().await.len() < MAX_BACKLOG);
        })
        .await;
}

#[tokio::test]
async fn noise_provisioning_and_key_change() {
    let dir = TmpDir::new("noise");
    let key_path = dir.path().join("api_key");
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(Some(key_path.clone())));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            assert_eq!(
                h.mdns_txt()[8],
                "api_encryption_supported=Noise_NNpsk0_25519_ChaChaPoly_SHA256"
            );
            // Без ключа: plaintext и Noise на нулевом ключе (провижининг).
            let mut plain = ts.connect();
            plain.hello("plain").await;
            event(&mut rx).await;
            assert_eq!(
                ts.connect_noise(&Psk::new([1; 32])).await.err().unwrap(),
                "Handshake MAC failure"
            );
            let mut p = ts.connect_noise(&Psk::new([0; 32])).await.unwrap();
            p.hello("HA").await;
            event(&mut rx).await;
            assert!(p.sync().await.is_empty());

            // Отвергнутый ключ ничего не меняет.
            p.send_msg(
                msg::NOISE_SET_KEY_REQ,
                &NoiseEncryptionSetKeyRequest {
                    key: Some(b"not base64".to_vec()),
                },
            )
            .await;
            let resp = p.expect(msg::NOISE_SET_KEY_RESP).await;
            assert!(field(&resp, 1).is_none());

            p.send_msg(
                msg::NOISE_SET_KEY_REQ,
                &NoiseEncryptionSetKeyRequest {
                    key: Some(format!(" {KEY_B64}\n").into_bytes()),
                },
            )
            .await;
            let resp = p.expect(msg::NOISE_SET_KEY_RESP).await;
            assert!(field(&resp, 1).unwrap().boolean());
            assert_eq!(
                std::fs::read_to_string(&key_path).unwrap(),
                format!("{KEY_B64}\n")
            );
            // Через 100 мс: новые TXT, затем каждого клиента просят уйти.
            let txt = event(&mut rx).await;
            let ApiEvent::MdnsTxtChanged(txt) = txt else {
                panic!("{txt:?}")
            };
            assert_eq!(
                txt[8],
                "api_encryption=Noise_NNpsk0_25519_ChaChaPoly_SHA256"
            );
            assert_eq!(h.mdns_txt(), txt);
            assert!(p.expect(msg::DISCONNECT_REQ).await.is_empty());
            assert!(p.closed().await.is_empty());
            assert!(plain.expect(msg::DISCONNECT_REQ).await.is_empty());
            assert!(plain.closed().await.is_empty());
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);

            // Теперь пускает только ключ.
            let mut plain = ts.connect();
            plain
                .send_msg(
                    msg::HELLO_REQ,
                    &HelloRequest {
                        client_info: Some("plain".into()),
                        api_version_major: Some(1),
                        api_version_minor: Some(10),
                    },
                )
                .await;
            assert_eq!(plain.closed().await, b"\x01\x00\x13\x01Bad indicator byte");
            assert_eq!(
                ts.connect_noise(&Psk::new([0; 32])).await.err().unwrap(),
                "Handshake MAC failure"
            );
            let key = esphome_proto::api_key::decode_psk(KEY_B64.as_bytes()).unwrap();
            let mut p = ts.connect_noise(&key).await.unwrap();
            p.hello("HA").await;
            event(&mut rx).await;

            // Тот же ключ снова: успех, никого не отключают.
            p.send_msg(
                msg::NOISE_SET_KEY_REQ,
                &NoiseEncryptionSetKeyRequest {
                    key: Some(KEY_B64.as_bytes().to_vec()),
                },
            )
            .await;
            assert!(
                field(&p.expect(msg::NOISE_SET_KEY_RESP).await, 1)
                    .unwrap()
                    .boolean()
            );
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(p.sync().await.is_empty());

            // Очистка: назад в plaintext.
            p.send_msg(
                msg::NOISE_SET_KEY_REQ,
                &NoiseEncryptionSetKeyRequest { key: None },
            )
            .await;
            assert!(
                field(&p.expect(msg::NOISE_SET_KEY_RESP).await, 1)
                    .unwrap()
                    .boolean()
            );
            assert!(!key_path.exists());
            let ApiEvent::MdnsTxtChanged(txt) = event(&mut rx).await else {
                panic!()
            };
            assert_eq!(
                txt[8],
                "api_encryption_supported=Noise_NNpsk0_25519_ChaChaPoly_SHA256"
            );
            assert!(p.expect(msg::DISCONNECT_REQ).await.is_empty());
            assert!(p.closed().await.is_empty());
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
            let mut plain = ts.connect();
            plain.hello("plain").await;
        })
        .await;
}

#[tokio::test]
async fn noise_frame_errors() {
    let dir = TmpDir::new("noise-err");
    let key_path = dir.path().join("api_key");
    std::fs::write(&key_path, format!("{KEY_B64}\n")).unwrap();
    local()
        .run_until(async {
            let ts = TestServer::start(selftest_config(Some(key_path)));
            let h = ts.api.clone();
            assert_eq!(
                h.mdns_txt()[8],
                "api_encryption=Noise_NNpsk0_25519_ChaChaPoly_SHA256"
            );
            let raw = |payload: &[u8]| {
                let mut v = Vec::new();
                append_noise_raw(&mut v, &[payload]);
                v
            };
            let reject = |reason: &str| raw(&[b"\x01", reason.as_bytes()].concat());
            let hello = raw(format!("\x01{}\0{}\0", identity().name, "aabbccddeeff").as_bytes());
            let cases: [(&[u8], Vec<u8>); 4] = [
                (b"\x01\x00\x81", reject("Bad handshake packet len")),
                (
                    b"\x01\x00\x00\x01\x00\x00",
                    [&hello[..], &reject("Empty handshake message")].concat(),
                ),
                (
                    b"\x01\x00\x00\x01\x00\x02\x05x",
                    [&hello[..], &reject("Bad handshake error byte")].concat(),
                ),
                (
                    b"\x01\x00\x00\x02",
                    [&hello[..], &reject("Bad indicator byte")].concat(),
                ),
            ];
            for (input, want) in cases {
                let mut p = ts.connect();
                p.write(input).await;
                assert_eq!(p.closed().await, want, "{input:?}");
            }
            // Мусор после handshake: расшифровка не проходит, клиент уходит.
            let key = esphome_proto::api_key::decode_psk(KEY_B64.as_bytes()).unwrap();
            let mut p = ts.connect_noise(&key).await.unwrap();
            p.write(b"\x01\x00\x20").await;
            p.write(&[0x55; 32]).await;
            assert!(p.closed().await.is_empty());
        })
        .await;
}

#[tokio::test]
async fn corrupt_key_file_fails_closed() {
    let dir = TmpDir::new("corrupt");
    let key_path = dir.path().join("api_key");
    std::fs::write(&key_path, "garbage\n").unwrap();
    local()
        .run_until(async {
            let ts = TestServer::start(selftest_config(Some(key_path)));
            let h = ts.api.clone();
            assert_eq!(
                h.mdns_txt()[8],
                "api_encryption=Noise_NNpsk0_25519_ChaChaPoly_SHA256"
            );
            assert_eq!(
                ts.connect_noise(&Psk::new([0; 32])).await.err().unwrap(),
                "Handshake MAC failure"
            );
            let mut p = ts.connect();
            p.write(b"\x00\x00\x01").await;
            assert_eq!(p.closed().await, b"\x01\x00\x13\x01Bad indicator byte");
        })
        .await;
}

#[tokio::test]
async fn shutdown_asks_clients_to_disconnect() {
    let local = local();
    let (mut a, mut b) = local
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let mut a = ts.connect();
            a.hello("HA").await;
            event(&mut ts.events).await;
            let b = ts.connect();
            assert!(a.sync().await.is_empty());
            drop(ts);
            (a, b)
        })
        .await;
    local
        .run_until(async {
            assert!(a.expect(msg::DISCONNECT_REQ).await.is_empty());
            assert!(a.closed().await.is_empty());
            // Соединение до hello просто закрывают.
            assert!(b.closed().await.is_empty());
        })
        .await;
}

#[test]
fn bind_failure_is_an_error() {
    let taken = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let cfg = ApiConfig {
        port,
        ..ApiConfig::default()
    };
    let err = spawn(identity(), cfg).err().unwrap();
    let text = err.to_string();
    assert!(
        text.starts_with(&format!("api: bind/listen :{port}: ")),
        "{text}"
    );
}

/// В пределах нескольких тиков от `d`. С остановленными часами tokio двигает их к следующему
/// таймеру, когда runtime паркуется (в том числе между сетевыми round trip), поэтому часы
/// клиента и сервера расходятся на несколько 5-секундных тиков.
fn about(x: Duration, d: Duration) -> bool {
    let slack = Duration::from_secs(15);
    x + slack > d && x <= d + slack
}

#[tokio::test(start_paused = true)]
async fn hello_timeout_ping_and_keepalive_drop() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let mut rx = ts.take_events();
            let mut silent = ts.connect();
            silent.wait = Duration::from_secs(600);
            let t0 = Instant::now();
            assert!(silent.closed().await.is_empty());
            assert!(about(t0.elapsed(), HELLO_TIMEOUT), "{:?}", t0.elapsed());

            let mut p = ts.connect();
            p.wait = Duration::from_secs(600);
            p.hello("HA").await;
            event(&mut rx).await;
            let t0 = Instant::now();
            // Простой: сервер пингует один раз.
            p.expect(msg::PING_REQ).await;
            assert!(about(t0.elapsed(), IDLE_PING), "{:?}", t0.elapsed());
            // Без ответа: отключает.
            assert!(p.closed().await.is_empty());
            assert!(about(t0.elapsed(), IDLE_DROP), "{:?}", t0.elapsed());
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);

            // Клиент, который отвечает, остаётся и снова получает время каждые 6 ч.
            let mut p = ts.connect();
            p.wait = Duration::from_secs(600);
            p.hello("HA").await;
            event(&mut rx).await;
            let t0 = Instant::now();
            let mut time_requests = 0;
            while t0.elapsed() < TIME_RESYNC + Duration::from_secs(60) {
                match p.recv().await.unwrap().0 {
                    msg::PING_REQ => p.send(msg::PING_RESP, &[]).await,
                    msg::GET_TIME_REQ => {
                        time_requests += 1;
                        assert!(about(t0.elapsed(), TIME_RESYNC), "{:?}", t0.elapsed());
                    }
                    t => panic!("message {t}"),
                }
            }
            assert_eq!(time_requests, 1);
            assert!(ts.api.connected());
        })
        .await;
}

/// Слушатель, который умеет отвечать ошибкой accept (нехватка дескрипторов), как настоящий
/// `TcpListener`.
struct FlakyListener {
    rx: UnboundedReceiver<Incoming>,
    fail: Rc<Cell<bool>>,
    errors: Rc<Cell<usize>>,
}

impl Listener for FlakyListener {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Incoming>> {
        if self.fail.get() {
            self.errors.set(self.errors.get() + 1);
            return Poll::Ready(Err(io::Error::from_raw_os_error(libc::EMFILE)));
        }
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Ok(item)),
            Poll::Ready(None) => Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe))),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn accept_errors_pause_accepting_without_starving_clients() {
    local()
        .run_until(async {
            let (tx, rx) = unbounded_channel();
            let fail = Rc::new(Cell::new(false));
            let errors = Rc::new(Cell::new(0));
            let listener = FlakyListener {
                rx,
                fail: fail.clone(),
                errors: errors.clone(),
            };
            let (_h, mut rx_ev) = spawn_with(Box::new(listener), identity(), config(None)).unwrap();
            let connect = |tx: &UnboundedSender<Incoming>| {
                let (server, client) = tokio::io::duplex(1 << 20);
                tx.send((Box::new(server), "test".into())).unwrap();
                Peer::with(client)
            };
            let mut p = connect(&tx);
            p.wait = Duration::from_secs(600);
            p.hello("HA").await;
            event(&mut rx_ev).await;
            // Дескрипторы кончились: слушатель всегда готов, каждый accept падает. Без паузы
            // актор закрутился бы и остановленные часы не сдвинулись бы.
            fail.set(true);
            for _ in 0..5 {
                assert!(p.sync().await.is_empty());
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            let n = errors.get();
            // Одна попытка на 500 мс паузы (плюс до сна), а не спин.
            assert!((5..40).contains(&n), "{n}");
            assert!(p.sync().await.is_empty());
            fail.set(false);
            let mut q = connect(&tx);
            q.wait = Duration::from_secs(600);
            q.hello("back").await;
            assert!(matches!(
                event(&mut rx_ev).await,
                ApiEvent::ClientConnected { .. }
            ));
        })
        .await;
}

#[tokio::test]
async fn hand_over_drops_audio_stop_and_answer_report_of_the_old_run() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut a = ts.connect();
            a.hello("HA").await;
            event(&mut rx).await;
            a.subscribe_voice(true).await;
            a.sync().await;
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));
            h.voice_request_start("Алиса");
            a.expect(msg::VA_REQUEST).await;
            h.voice_send_audio(vec![1]);
            a.expect(msg::VA_AUDIO).await;

            let mut b = ts.connect();
            b.hello("HA again").await;
            event(&mut rx).await;
            b.subscribe_voice(true).await;
            b.sync().await;
            // Контроллер ещё не видел передачи: аудио, stop и отчёт ответа старого run.
            // Отчёт анонса всё равно идёт владельцу.
            h.voice_send_audio(vec![2]);
            h.voice_request_stop();
            h.voice_answer_finished(true);
            h.voice_announce_finished(false);
            assert!(b.expect(msg::VA_ANNOUNCE_FINISHED).await.is_empty());
            assert!(b.sync().await.is_empty());
            assert!(a.sync().await.is_empty());
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(false));
            assert_eq!(event(&mut rx).await, ApiEvent::VoiceSubscribed(true));

            // Новый run у нового владельца получает всё снова.
            h.voice_request_start("");
            b.expect(msg::VA_REQUEST).await;
            h.voice_send_audio(vec![3]);
            b.expect(msg::VA_AUDIO).await;
            h.voice_answer_finished(true);
            let m = b.expect(msg::VA_ANNOUNCE_FINISHED).await;
            assert!(field(&m, 1).unwrap().boolean());
            h.voice_request_stop();
            assert!(b.expect(msg::VA_REQUEST).await.is_empty());
        })
        .await;
}

#[tokio::test]
async fn amplified_replies_still_meet_the_backlog_limit() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut p = ts.connect_buf(4096);
            p.hello("flood").await;
            event(&mut rx).await;
            // 3-байтовые HelloRequest, каждый с ответом ~40 байт, без чтения: одна пачка
            // чтения даёт ~1 MiB ответов. Проверяется лимит и закрытие «клиент не читает».
            let chunk = b"\x00\x00\x01".repeat(20_000);
            for _ in 0..200 {
                if p.s.write_all(&chunk).await.is_err() {
                    break;
                }
                if !rx.is_empty() {
                    break;
                }
            }
            assert_eq!(event(&mut rx).await, ApiEvent::ClientDisconnected);
            assert!(!h.connected());
            assert!(p.closed().await.len() <= MAX_BACKLOG + READ_CHUNK);
        })
        .await;
}

mod entities;
