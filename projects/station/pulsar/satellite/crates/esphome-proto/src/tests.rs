//! Golden-оракулы и тесты протокола. `proto.txt` и `selftest_raw.txt` — захват C++-референса;
//! ожидаемые байты никогда не подгоняются под реализацию.

use std::fmt::Write as _;

use prost::Message;

use crate::api_key::{decode_psk, trim};
use crate::entity::*;
use crate::frame::tests::{Initiator, MAC, NAME, noise_bodies};
use crate::frame::{NoiseFrame, Psk, append_plain, parse_noise};
use crate::pb::msg;
use crate::pb::*;
use crate::session::{Decoded, EncodeError, Session, SessionConfig, SessionEvent};
use crate::timezone::posix_tz_from_parsed;

const PROTO: &str = include_str!("../tests/golden/proto.txt");
const SELFTEST: &str = include_str!("../tests/golden/selftest_raw.txt");

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

fn cases(kind: &str) -> Vec<Vec<&'static str>> {
    let v: Vec<Vec<&str>> = PROTO
        .lines()
        .filter_map(|l| {
            let mut w = l.split(' ');
            (w.next() == Some(kind)).then(|| w.collect())
        })
        .collect();
    assert!(!v.is_empty(), "нет golden-векторов для {kind}");
    v
}

// Примитивы провода из `proto.txt`. Это не production-энкодер (там prost): независимый
// оракул тех же правил protobuf, чтобы golden-векторы продолжали доказывать байты derives.

fn put_varint(out: &mut Vec<u8>, v: u64) {
    prost::encoding::encode_varint(v, out);
}

fn get_varint(p: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    while shift < 70 && *pos < p.len() {
        let b = p[*pos];
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
    None
}

fn zigzag32(v: u64) -> i32 {
    ((v >> 1) as u32 ^ (v as u32 & 1).wrapping_neg()) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WireType {
    Varint = 0,
    Fixed64 = 1,
    Len = 2,
    Fixed32 = 5,
}

#[derive(Clone, Debug, Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self::default()
    }
    fn data(&self) -> &[u8] {
        &self.buf
    }
    fn tag(&mut self, field: u32, wt: WireType) {
        put_varint(&mut self.buf, (u64::from(field) << 3) | wt as u64);
    }
    fn u32(&mut self, field: u32, v: u32, force: bool) {
        if v == 0 && !force {
            return;
        }
        self.tag(field, WireType::Varint);
        put_varint(&mut self.buf, u64::from(v));
    }
    fn i32(&mut self, field: u32, v: i32, force: bool) {
        if v == 0 && !force {
            return;
        }
        self.tag(field, WireType::Varint);
        put_varint(&mut self.buf, i64::from(v) as u64);
    }
    fn boolean(&mut self, field: u32, v: bool, force: bool) {
        if !v && !force {
            return;
        }
        self.tag(field, WireType::Varint);
        self.buf.push(u8::from(v));
    }
    fn fixed32(&mut self, field: u32, v: u32, force: bool) {
        if v == 0 && !force {
            return;
        }
        self.tag(field, WireType::Fixed32);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn flt(&mut self, field: u32, v: f32, force: bool) {
        if v == 0.0 && !force {
            return;
        }
        self.fixed32(field, v.to_bits(), true);
    }
    fn bytes(&mut self, field: u32, p: &[u8], force: bool) {
        if p.is_empty() && !force {
            return;
        }
        self.tag(field, WireType::Len);
        put_varint(&mut self.buf, p.len() as u64);
        self.buf.extend_from_slice(p);
    }
    fn msg(&mut self, field: u32, sub: &Self) {
        self.bytes(field, &sub.buf, true);
    }
    fn packed_u32(&mut self, field: u32, vals: &[u32]) {
        if vals.is_empty() {
            return;
        }
        let mut tmp = Vec::new();
        for &v in vals {
            put_varint(&mut tmp, u64::from(v));
        }
        self.bytes(field, &tmp, true);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Field<'a> {
    num: u32,
    wt: WireType,
    v: u64,
    data: &'a [u8],
}

struct Reader<'a> {
    p: &'a [u8],
    pos: usize,
    err: bool,
}

impl<'a> Reader<'a> {
    fn new(p: &'a [u8]) -> Self {
        Self {
            p,
            pos: 0,
            err: false,
        }
    }
    fn next(&mut self) -> Option<Field<'a>> {
        if self.pos >= self.p.len() {
            return None;
        }
        let Some(key) = get_varint(self.p, &mut self.pos) else {
            return self.fail();
        };
        let num = (key >> 3) as u32;
        let rest = self.p.len() - self.pos;
        let mut f = Field {
            num,
            wt: WireType::Varint,
            v: 0,
            data: &[],
        };
        match key & 7 {
            0 => match get_varint(self.p, &mut self.pos) {
                Some(v) => f.v = v,
                None => return self.fail(),
            },
            1 => {
                if rest < 8 {
                    return self.fail();
                }
                f.wt = WireType::Fixed64;
                f.v = u64::from_le_bytes(self.take(8).try_into().unwrap_or_default());
            }
            5 => {
                if rest < 4 {
                    return self.fail();
                }
                f.wt = WireType::Fixed32;
                f.v = u64::from(u32::from_le_bytes(
                    self.take(4).try_into().unwrap_or_default(),
                ));
            }
            2 => {
                let Some(len) = get_varint(self.p, &mut self.pos) else {
                    return self.fail();
                };
                let Ok(len) = usize::try_from(len) else {
                    return self.fail();
                };
                if len > self.p.len() - self.pos {
                    return self.fail();
                }
                f.wt = WireType::Len;
                f.data = self.take(len);
            }
            _ => return self.fail(),
        }
        Some(f)
    }
    fn error(&self) -> bool {
        self.err
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.p[self.pos..self.pos + n];
        self.pos += n;
        s
    }
    fn fail(&mut self) -> Option<Field<'a>> {
        self.err = true;
        self.pos = self.p.len();
        None
    }
}

fn flag(s: &str) -> bool {
    s == "1"
}

#[test]
fn varint_golden() {
    for c in cases("varint") {
        let mut out = Vec::new();
        put_varint(&mut out, c[0].parse().unwrap());
        assert_eq!(hex(&out), c[1], "put_varint {}", c[0]);
    }
    for c in cases("get_varint") {
        let input = unhex(c[0]);
        let mut pos = 0;
        let got = get_varint(&input, &mut pos)
            .map_or_else(|| format!("fail {pos}"), |v| format!("ok {v} {pos}"));
        assert_eq!(got, c[1..].join(" "), "get_varint {}", c[0]);
    }
}

#[test]
fn writer_golden() {
    let check = |kind: &str, f: &dyn Fn(&mut Writer, &[&str])| {
        for c in cases(kind) {
            let mut w = Writer::new();
            f(&mut w, &c);
            assert_eq!(hex(w.data()), *c.last().unwrap(), "{kind} {c:?}");
        }
    };
    check("u32", &|w, c| {
        w.u32(c[0].parse().unwrap(), c[1].parse().unwrap(), flag(c[2]));
    });
    check("i32", &|w, c| {
        w.i32(c[0].parse().unwrap(), c[1].parse().unwrap(), flag(c[2]));
    });
    check("bool", &|w, c| {
        w.boolean(c[0].parse().unwrap(), flag(c[1]), flag(c[2]));
    });
    check("fixed32", &|w, c| {
        w.fixed32(c[0].parse().unwrap(), c[1].parse().unwrap(), flag(c[2]));
    });
    check("flt", &|w, c| {
        let bits = u32::from_str_radix(c[1], 16).unwrap();
        w.flt(c[0].parse().unwrap(), f32::from_bits(bits), flag(c[2]));
    });
    check("bytes", &|w, c| {
        w.bytes(c[0].parse().unwrap(), &unhex(c[1]), flag(c[2]));
    });
    check("msg", &|w, c| {
        let mut sub = Writer::new();
        sub.buf = unhex(c[1]);
        w.msg(c[0].parse().unwrap(), &sub);
    });
    check("packed", &|w, c| {
        let vals: Vec<u32> = if c[1] == "-" {
            Vec::new()
        } else {
            c[1].split(',').map(|v| v.parse().unwrap()).collect()
        };
        w.packed_u32(c[0].parse().unwrap(), &vals);
    });
}

#[test]
fn reader_golden_including_broken_input() {
    for c in cases("reader") {
        let input = unhex(c[0]);
        let mut r = Reader::new(&input);
        let mut out = String::new();
        while let Some(f) = r.next() {
            let data = if f.wt == WireType::Len {
                hex(f.data)
            } else {
                "-".into()
            };
            let _ = write!(out, "{}:{}:{}:{data} ", f.num, f.wt as u8, f.v);
        }
        let again = r.next().is_some();
        let _ = write!(out, "err={} again={}", u8::from(r.error()), u8::from(again));
        assert_eq!(out, c[1..].join(" "), "reader {}", c[0]);
    }
}

#[test]
fn zigzag_golden() {
    for c in cases("zigzag") {
        assert_eq!(
            zigzag32(c[0].parse().unwrap()),
            c[1].parse::<i32>().unwrap(),
            "zigzag {}",
            c[0]
        );
    }
}

#[test]
fn psk_golden() {
    for c in cases("psk") {
        let got = decode_psk(&unhex(c[0])).map_or("fail".into(), |k| hex(k.as_bytes()));
        assert_eq!(got, c[1], "psk {}", c[0]);
    }
}

#[test]
fn trim_golden() {
    for c in cases("trim") {
        assert_eq!(hex(trim(&unhex(c[0]))), c[1], "trim {}", c[0]);
    }
}

#[test]
fn posix_tz_golden() {
    for c in cases("posix_tz") {
        let tz = posix_tz_from_parsed(&unhex(c[0]));
        let want = if c[1] == "-" { "" } else { c[1] };
        assert_eq!(tz, want, "posix_tz {}", c[0]);
    }
}

fn selftest_lists() -> (Capabilities, Vec<String>) {
    let s = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    (
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
        },
        s(&[
            "Rainbow",
            "Pulse",
            "None",
            "Smiley",
            "Off",
            "All styles",
            "Spectrum",
            "None",
            "blue",
            "Off",
            "Levels",
            "Violet",
            "Blue",
            "Coordinator",
            "Router",
        ]),
    )
}

fn selftest_options(lists: &[String]) -> EntityOptions<'_> {
    EntityOptions {
        light_effects: &lists[0..2],
        clock_faces: &lists[2..4],
        music_styles: &lists[4..7],
        case_colors: &lists[7..9],
        clock_music_styles: &lists[9..11],
        voice_colors: &lists[11..13],
        zigbee_modes: &lists[13..15],
    }
}

fn entity_key_of(msg_type: u32, payload: &[u8]) -> Option<u32> {
    match msg_type {
        msg::LIST_BINARY_SENSOR => ListEntitiesBinarySensorResponse::decode(payload).ok()?.key,
        msg::LIST_LIGHT => ListEntitiesLightResponse::decode(payload).ok()?.key,
        msg::LIST_SENSOR => ListEntitiesSensorResponse::decode(payload).ok()?.key,
        msg::LIST_SWITCH => ListEntitiesSwitchResponse::decode(payload).ok()?.key,
        msg::LIST_TEXT_SENSOR => ListEntitiesTextSensorResponse::decode(payload).ok()?.key,
        msg::LIST_NUMBER => ListEntitiesNumberResponse::decode(payload).ok()?.key,
        msg::LIST_SELECT => ListEntitiesSelectResponse::decode(payload).ok()?.key,
        msg::LIST_BUTTON => ListEntitiesButtonResponse::decode(payload).ok()?.key,
        msg::LIST_MEDIA_PLAYER => ListEntitiesMediaPlayerResponse::decode(payload).ok()?.key,
        msg::LIST_TEXT => ListEntitiesTextResponse::decode(payload).ok()?.key,
        msg::LIST_EVENT => ListEntitiesEventResponse::decode(payload).ok()?.key,
        msg::LIST_UPDATE => ListEntitiesUpdateResponse::decode(payload).ok()?.key,
        msg::LIST_SERVICES => ListEntitiesServicesResponse::decode(payload).ok()?.key,
        _ => None,
    }
}

/// Сущности, которые форк добавил к захвату C++-референса.
fn fork_only(msg_type: u32, payload: &[u8]) -> bool {
    const KEYS: [u32; 5] = [
        key::SW_WAKE_EVENT_RECORDING.get(),
        key::SW_WAKE_DRY_RUN.get(),
        key::BUTTON_WAKE_FALSE_ALARM.get(),
        key::BUTTON_WAKE_MISSED.get(),
        key::SELECT_VOICE_COLOR.get(),
    ];
    matches!(
        msg_type,
        msg::LIST_SWITCH | msg::LIST_BUTTON | msg::LIST_SELECT
    ) && entity_key_of(msg_type, payload).is_some_and(|k| KEYS.contains(&k))
}

#[test]
fn list_entities_matches_cxx_golden() {
    let golden: Vec<(u32, Vec<u8>)> = SELFTEST
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let (t, hex) = l.split_once(' ')?;
            let t: u32 = t.parse().ok()?;
            matches!(
                t,
                msg::LIST_BINARY_SENSOR
                    | msg::LIST_LIGHT
                    | msg::LIST_SENSOR
                    | msg::LIST_SWITCH
                    | msg::LIST_TEXT_SENSOR
                    | msg::LIST_NUMBER
                    | msg::LIST_SELECT
                    | msg::LIST_BUTTON
                    | msg::LIST_MEDIA_PLAYER
                    | msg::LIST_TEXT
                    | msg::LIST_EVENT
                    | msg::LIST_UPDATE
                    | msg::LIST_SERVICES
                    | msg::LIST_DONE
            )
            .then(|| (t, unhex(hex)))
        })
        .collect();
    let (caps, lists) = selftest_lists();
    let got: Vec<(u32, Vec<u8>)> = list_entities(&caps, &selftest_options(&lists))
        .into_iter()
        .filter(|m| !fork_only(m.msg_type, &m.payload))
        .map(|m| (m.msg_type, m.payload))
        .collect();
    assert_eq!(got.len(), golden.len());
    for (i, (g, r)) in golden.iter().zip(&got).enumerate() {
        assert_eq!(g.0, r.0, "сообщение {i}");
        assert_eq!(hex(&g.1), hex(&r.1), "сообщение {i}, тип {}", g.0);
    }
}

#[test]
fn entity_table_snapshot() {
    let empty: Vec<String> = Vec::new();
    let all = EntityOptions {
        light_effects: &empty,
        clock_faces: &empty,
        music_styles: &empty,
        case_colors: &empty,
        clock_music_styles: &empty,
        voice_colors: &empty,
        zigbee_modes: &empty,
    };
    let full = Capabilities {
        clock: true,
        lux: true,
        auto_brightness: true,
        wake_preroll: true,
        hw_mic_mute: true,
        buttons: true,
        case_colors: true,
        voice_colors: true,
        zigbee: true,
    };
    let keys = |caps: &Capabilities| -> Vec<u32> {
        list_entities(caps, &all)
            .into_iter()
            .filter_map(|m| entity_key_of(m.msg_type, &m.payload))
            .collect()
    };
    assert_eq!(
        keys(&full),
        [
            1, 2, 3, 33, 20, 25, 28, 32, 36, 37, 27, 39, 26, 29, 38, 31, 19, 4, 5, 6, 7, 8, 9, 34,
            35, 10, 11, 12, 13, 16, 14, 15, 21, 22, 23, 24, 17, 18, 30
        ]
    );
    let minimal = Capabilities::default();
    assert_eq!(
        keys(&minimal),
        [
            1, 2, 20, 25, 28, 36, 37, 27, 39, 26, 19, 7, 9, 34, 35, 10, 11, 12, 13, 30
        ]
    );
}

#[test]
fn table_keys_are_unique_and_cover_1_to_39() {
    let mut keys: Vec<u32> = ENTITIES.iter().map(|d| d.key.get()).collect();
    keys.sort_unstable();
    assert_eq!(keys, (1..=39).collect::<Vec<_>>());
}

#[test]
fn typed_ids_map_to_table_entries() {
    fn check<T: Copy>(ids: &[T], key: impl Fn(T) -> EntityKey, tag: EntityKindTag) {
        for &id in ids {
            let def = find(key(id)).unwrap_or_else(|| panic!("нет записи для {:?}", key(id)));
            assert_eq!(def.kind.tag(), tag, "ключ {:?}", key(id));
        }
    }
    check(
        &[
            SwitchId::Microphones,
            SwitchId::Clock,
            SwitchId::AutoUpdate,
            SwitchId::NearestAnswers,
            SwitchId::VoiceIndication,
            SwitchId::WakePreroll,
            SwitchId::ClockAutoBrightness,
            SwitchId::WakeEventRecording,
            SwitchId::WakeDryRun,
        ],
        SwitchId::key,
        EntityKindTag::Switch,
    );
    check(
        &[SensorId::Illuminance],
        SensorId::key,
        EntityKindTag::Sensor,
    );
    check(
        &[BinarySensorId::HwMicMute],
        BinarySensorId::key,
        EntityKindTag::BinarySensor,
    );
    check(
        &[
            NumberId::ClockBrightness,
            NumberId::ClockScroll,
            NumberId::LedSpeed,
            NumberId::FollowupDuration,
        ],
        NumberId::key,
        EntityKindTag::Number,
    );
    check(&[TextId::ClockText], TextId::key, EntityKindTag::Text);
    check(
        &[
            SelectId::ClockFace,
            SelectId::ZigbeeMode,
            SelectId::MusicViz,
            SelectId::CaseColor,
            SelectId::ClockMusic,
            SelectId::VoiceColor,
        ],
        SelectId::key,
        EntityKindTag::Select,
    );
    check(
        &[
            ButtonId::ZigbeeJoin,
            ButtonId::ZigbeeLeave,
            ButtonId::WakeFalseAlarm,
            ButtonId::WakeMissed,
        ],
        ButtonId::key,
        EntityKindTag::Button,
    );
    check(&[EventId::Button], EventId::key, EntityKindTag::Event);
    check(
        &[TextSensorId::Zigbee],
        TextSensorId::key,
        EntityKindTag::TextSensor,
    );
    check(
        &[
            ActionId::ClockShowText,
            ActionId::ClockShowFace,
            ActionId::LedShow,
        ],
        ActionId::key,
        EntityKindTag::Action,
    );
    // Один ключ не описан двумя разными id.
    let mut keys = vec![
        SwitchId::Microphones.key(),
        SwitchId::Clock.key(),
        SwitchId::AutoUpdate.key(),
        SwitchId::NearestAnswers.key(),
        SwitchId::VoiceIndication.key(),
        SwitchId::WakePreroll.key(),
        SwitchId::ClockAutoBrightness.key(),
        SwitchId::WakeEventRecording.key(),
        SwitchId::WakeDryRun.key(),
        SensorId::Illuminance.key(),
        BinarySensorId::HwMicMute.key(),
        NumberId::ClockBrightness.key(),
        NumberId::ClockScroll.key(),
        NumberId::LedSpeed.key(),
        NumberId::FollowupDuration.key(),
        TextId::ClockText.key(),
        SelectId::ClockFace.key(),
        SelectId::ZigbeeMode.key(),
        SelectId::MusicViz.key(),
        SelectId::CaseColor.key(),
        SelectId::ClockMusic.key(),
        SelectId::VoiceColor.key(),
        ButtonId::ZigbeeJoin.key(),
        ButtonId::ZigbeeLeave.key(),
        ButtonId::WakeFalseAlarm.key(),
        ButtonId::WakeMissed.key(),
        EventId::Button.key(),
        TextSensorId::Zigbee.key(),
        ActionId::ClockShowText.key(),
        ActionId::ClockShowFace.key(),
        ActionId::LedShow.key(),
    ];
    keys.sort_unstable();
    let before = keys.len();
    keys.dedup();
    assert_eq!(before, keys.len(), "id повторяют ключи");
}

#[test]
fn replay_order_covers_every_replayable_entity() {
    let mut replay: Vec<u32> = REPLAY_ORDER.iter().map(|k| k.get()).collect();
    let before = replay.len();
    replay.sort_unstable();
    replay.dedup();
    assert_eq!(before, replay.len(), "REPLAY_ORDER повторяет ключи");
    for def in ENTITIES {
        if !matches!(
            def.kind.tag(),
            EntityKindTag::Event | EntityKindTag::Button | EntityKindTag::Action
        ) {
            assert!(
                REPLAY_ORDER.contains(&def.key),
                "{} отсутствует в REPLAY_ORDER",
                def.object_id
            );
        }
    }
}

#[test]
fn plaintext_feed_and_encode() {
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), true));
    let mut input = Vec::new();
    append_plain(&mut input, 1, b"one");
    append_plain(&mut input, 300, b"two");
    let complete = input.len();
    // Неполный третий кадр остаётся в буфере вызывающего.
    let mut tail = Vec::new();
    append_plain(&mut tail, 7, b"three");
    tail.truncate(4);
    input.extend_from_slice(&tail);
    let mut out = Vec::new();
    let feed = s.feed(&input, &mut out);
    assert_eq!(feed.consumed, complete);
    assert_eq!(feed.messages.len(), 2);
    assert_eq!(
        feed.messages[0],
        Decoded {
            msg_type: 1,
            payload: b"one".to_vec()
        }
    );
    assert_eq!(
        feed.messages[1],
        Decoded {
            msg_type: 300,
            payload: b"two".to_vec()
        }
    );
    assert!(feed.event.is_none());
    assert!(out.is_empty());

    let mut scratch = Vec::new();
    s.encode(8, b"pong", &mut out, &mut scratch).unwrap();
    assert_eq!(out, b"\x00\x04\x08pong");

    // Остаток третьего кадра доходит отдельным feed.
    let mut rest = Vec::new();
    append_plain(&mut rest, 7, b"three");
    let feed = s.feed(&rest, &mut out);
    assert_eq!(feed.consumed, rest.len());
    assert_eq!(feed.messages[0].payload, b"three");
}

#[test]
fn plaintext_is_refused_with_a_key() {
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([1u8; 32]), false));
    let mut out = Vec::new();
    let feed = s.feed(b"\x00\x00\x01", &mut out);
    assert_eq!(feed.event, Some(SessionEvent::Reject("Bad indicator byte")));
    assert_eq!(out, b"\x01\x00\x13\x01Bad indicator byte");
}

#[test]
fn bad_framing_closes() {
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), true));
    let mut out = Vec::new();
    assert_eq!(
        s.feed(b"\x07", &mut out).event,
        Some(SessionEvent::Close("bad frame preamble"))
    );
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), true));
    let huge = vec![0u8, 0xff, 0xff, 0xff, 0xff, 0x0f, 1];
    let feed = s.feed(&huge, &mut out);
    assert_eq!(feed.event, Some(SessionEvent::Close("frame too large")));
}

#[test]
fn noise_handshake_and_data() {
    let psk = Psk::new(std::array::from_fn(|i| i as u8 * 3));
    let (mut client, hello) = Initiator::new(&psk);
    let mut s = Session::new(SessionConfig::new(NAME, MAC, psk, false));
    let mut out = Vec::new();
    let feed = s.feed(&hello, &mut out);
    assert_eq!(feed.consumed, hello.len());
    assert!(feed.messages.is_empty());
    let bodies = noise_bodies(&out);
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], format!("\x01{NAME}\0{MAC}\0").as_bytes());

    assert!(s.handshake_done());
    client.finish(&bodies[1]).unwrap();

    // Сервер → клиент.
    let mut scratch = Vec::new();
    let mut frame = Vec::new();
    s.encode(90, b"\x08\x01", &mut frame, &mut scratch).unwrap();
    let body = match parse_noise(&frame, false) {
        NoiseFrame::Frame { body, .. } => body,
        other => panic!("{other:?}"),
    };
    assert_eq!(client.decrypt(body), (90, b"\x08\x01".to_vec()));

    // Клиент → сервер.
    let mut client = client;
    let frame = client.encrypt(92, b"\x0a\x03abc");
    let mut out3 = Vec::new();
    let feed = s.feed(&frame, &mut out3);
    assert_eq!(feed.consumed, frame.len());
    assert_eq!(feed.messages[0].msg_type, 92);
    assert_eq!(feed.messages[0].payload, b"\x0a\x03abc");
}

#[test]
fn noise_wrong_key_is_a_mac_failure() {
    let (_, hello) = Initiator::new(&Psk::new([1u8; 32]));
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([2u8; 32]), false));
    let mut out = Vec::new();
    let feed = s.feed(&hello, &mut out);
    assert_eq!(
        feed.event,
        Some(SessionEvent::Reject("Handshake MAC failure"))
    );
    let bodies = noise_bodies(&out);
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[1], b"\x01Handshake MAC failure");
}

#[test]
fn noise_incomplete_and_oversized_frames() {
    let (_, hello) = Initiator::new(&Psk::new([0u8; 32]));
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), false));
    let mut out = Vec::new();
    let feed = s.feed(&hello[..2], &mut out);
    assert_eq!(feed.consumed, 0);
    assert!(feed.messages.is_empty());
    let feed = s.feed(&hello, &mut out);
    assert_eq!(feed.consumed, hello.len());
    assert_eq!(noise_bodies(&out).len(), 2);

    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), false));
    let mut out = Vec::new();
    // Во время handshake кадр длиннее 128 байт отвергается.
    let mut big = vec![0x01, 0x00, 0x81];
    big.resize(3 + 0x81, 0);
    let feed = s.feed(&big, &mut out);
    assert_eq!(
        feed.event,
        Some(SessionEvent::Reject("Bad handshake packet len"))
    );
}

#[test]
fn reject_all_refuses_every_handshake() {
    // Fail-closed: случайный ключ получить не удалось — не пускаем никого, даже с psk из
    // конфигурации (он нулевой и известен).
    let psk = Psk::new([0u8; 32]);
    let mut cfg = SessionConfig::new(NAME, MAC, psk.clone(), false);
    cfg.reject_all = true;
    let mut s = Session::new(cfg);
    let (_, hello) = Initiator::new(&psk);
    let mut out = Vec::new();
    let feed = s.feed(&hello, &mut out);
    assert_eq!(
        feed.event,
        Some(SessionEvent::Reject("Handshake MAC failure"))
    );
    let bodies = noise_bodies(&out);
    assert_eq!(bodies[0], format!("\x01{NAME}\0{MAC}\0").as_bytes());
    assert_eq!(bodies[1], b"\x01Handshake MAC failure");
}

#[test]
fn encode_before_handshake_writes_nothing() {
    let mut s = Session::new(SessionConfig::new(NAME, MAC, Psk::new([0u8; 32]), false));
    let mut out = Vec::new();
    let mut scratch = Vec::new();
    assert_eq!(
        s.encode(1, b"x", &mut out, &mut scratch),
        Err(EncodeError::NotReady)
    );
    assert!(out.is_empty());
}
