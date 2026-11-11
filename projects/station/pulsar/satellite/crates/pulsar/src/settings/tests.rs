// Фикстуры в tests/golden/settings/ сняты с эталона на C++ (rust-port-base): Settings::load и
// Settings::save из controller.cpp, перенесённые без изменений во временный хост-харнесс, собранный
// `zig c++ -std=c++20 -O2`. save_*.conf — `save` значений ниже; load_*.out — `load` из load_*.in,
// затем `save`. С тех пор правились руками: `wake_preroll` (после `clock_music`; 0 в load_odd.in);
// `clock_auto_brightness` (после `wake_preroll`); `voice_color` (после `led_case_color`);
// `followup_duration_s` (10); ключи форка `wake_event_recording` / `wake_dry_run` /
// `announce_volume` / `mic_gain_db` — последними; и
// умолчания форка (`wake_word=ricardo`, `nearest_answers=0`).
//
// `Settings` больше не хранит чужие доменные типы: опции HA и роль Zigbee — строки `state.conf`,
// которые контроллер отображает на свои типы на границе.

use super::*;

// Под Bazel тест исполняется из runfiles-дерева, где абсолютный
// CARGO_MANIFEST_DIR сборочного экшена не существует; PULSAR_MANIFEST_DIR
// задаёт путь относительно текущего каталога теста. Cargo его не выставляет,
// поэтому там работает прежний CARGO_MANIFEST_DIR.
fn dir() -> PathBuf {
    std::env::var_os("PULSAR_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
        .join("tests/golden/settings")
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap()
}

/// As the harness read its arguments (strtof).
fn f(s: &str) -> f32 {
    parse_f32_prefix(s)
}

fn values() -> Settings {
    Settings {
        volume: f("0.30000001"),
        volume_legacy: f("0.123456789"),
        clock_brightness: 0.0001,
        clock_scroll_ms: 1500,
        light: Light {
            on: true,
            brightness: f("1234567"),
            r: f("0.00001"),
            g: f("123456.7"),
            b: f("3.14159265"),
            effect: "Rainbow".into(),
        },
        led_speed: 0.25,
        zigbee_flash_failed: 0x5001_abcd,
        zigbee_role: "pulsar-coordinator".into(),
        tz: "EST5EDT,M3.2.0,M11.1.0".into(),
        update_attempt: "v1.2.3".into(),
        led_case_color: "white".into(),
        muted: true,
        mic_muted: false,
        clock_on: true,
        ..Settings::default()
    }
}

#[test]
fn save_matches_golden_byte_for_byte() {
    assert_eq!(
        Settings::default().serialize().as_bytes(),
        golden("save_default.conf")
    );
    assert_eq!(values().serialize().as_bytes(), golden("save_values.conf"));
    let extremes = Settings {
        volume: -1.0,
        volume_legacy: -1.0,
        clock_brightness: 1e-30,
        clock_scroll_ms: 100,
        light: Light {
            on: false,
            brightness: 1e30,
            r: 0.0,
            g: 1.0,
            b: 0.5,
            effect: String::new(),
        },
        led_speed: 4.0,
        zigbee_flash_failed: 0xffff_ffff,
        zigbee_role: "router".into(),
        tz: String::new(),
        mic_muted: true,
        clock_on: false,
        ..Settings::default()
    };
    assert_eq!(
        extremes.serialize().as_bytes(),
        golden("save_extremes.conf")
    );
}

#[test]
fn parses_a_file_written_by_the_fork() {
    let mut s = Settings::default();
    s.parse(&golden("save_values.conf"));
    // Written with 6 significant digits: the values read back are those of the text.
    let want = Settings {
        volume: 0.3,
        has_volume: true,
        volume_legacy: 0.123_457,
        light: Light {
            brightness: 1_234_570.0,
            g: 123_457.0,
            b: f("3.14159"),
            ..values().light
        },
        ..values()
    };
    assert_eq!(s, want);
    assert_eq!(s.serialize().as_bytes(), golden("save_values.conf"));
}

#[test]
fn load_then_save_matches_cxx() {
    for (name, has_volume, legacy_on_stock) in
        [("load_odd", false, true), ("load_edge", true, false)]
    {
        let mut s = Settings::default();
        s.parse(&golden(&format!("{name}.in")));
        assert_eq!(
            s.serialize(),
            String::from_utf8(golden(&format!("{name}.out"))).unwrap(),
            "{name}"
        );
        assert_eq!(
            (s.has_volume, s.legacy_on_stock),
            (has_volume, legacy_on_stock),
            "{name}"
        );
    }
}

/// A fresh directory under the system temp dir, removed on drop.
struct TmpDir(PathBuf);

impl TmpDir {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "pulsar-settings-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn save_is_atomic_and_load_reads_it_back() {
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    let mut missing = Settings::default();
    assert!(!missing.load(&path));
    assert!(values().save(&path));
    assert!(!t.0.join("state.conf.tmp").exists());
    assert_eq!(std::fs::read(&path).unwrap(), golden("save_values.conf"));
    let mut s = Settings::default();
    assert!(s.load(&path));
    assert_eq!(s.update_attempt, "v1.2.3");
    // Missing directory: the temporary file cannot be created (logged), nothing written.
    assert!(!values().save(&t.0.join("no/state.conf")));
    assert!(!t.0.join("no").exists());
}

#[test]
fn stale_tmp_is_replaced() {
    // A power cut mid-write leaves `state.conf.tmp`; the next write truncates it.
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    std::fs::write(t.0.join("state.conf.tmp"), vec![b'x'; 4096]).unwrap();
    assert!(values().save(&path));
    assert_eq!(std::fs::read(&path).unwrap(), golden("save_values.conf"));
    assert!(!t.0.join("state.conf.tmp").exists());
}

// A write that fails half-way (here ENOSPC from /dev/full behind the temporary name) must not
// be renamed over the good file.
#[cfg(target_os = "linux")]
#[test]
fn failed_write_keeps_the_old_file() {
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    let tmp = t.0.join("state.conf.tmp");
    assert!(values().save(&path));
    std::os::unix::fs::symlink("/dev/full", &tmp).unwrap();
    let mut s = values();
    s.led_speed = 2.0;
    assert!(!s.save(&path));
    assert_eq!(std::fs::read(&path).unwrap(), golden("save_values.conf"));
    assert!(std::fs::symlink_metadata(&tmp).is_err());
}

/// The file's bytes, None when absent.
fn on_disk(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

const NEVER: std::time::Duration = std::time::Duration::from_secs(3600);

#[tokio::test(start_paused = true)]
async fn changes_coalesce_into_one_write_after_the_last() {
    use tokio::time::{Duration, Instant, advance, timeout};
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    let mut sf = Store::with_values(path.clone(), values());
    let start = Instant::now();
    for i in 0..20 {
        sf.led_speed = 0.5 + 0.1 * i as f32;
        sf.changed();
        if i < 19 {
            advance(Duration::from_millis(50)).await;
        }
    }
    // Nothing written before SAVE_DELAY_MS after the last change.
    assert!(
        timeout(Duration::from_millis(SAVE_DELAY_MS - 1), sf.due())
            .await
            .is_err()
    );
    assert_eq!(on_disk(&path), None);
    sf.due().await;
    assert_eq!((Instant::now() - start).as_millis(), 19 * 50 + 1000);
    sf.flush();
    assert_eq!(on_disk(&path).unwrap(), sf.serialize().as_bytes());
    assert!(sf.serialize().contains("\nled_speed=2.4\n"));
    // One write: nothing else pending.
    assert!(timeout(NEVER, sf.due()).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn write_now_bypasses_and_cancels_the_debounce() {
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    let mut sf = Store::with_values(path.clone(), values());
    sf.led_speed = 3.0;
    sf.changed();
    sf.zigbee_flash_failed = 0x5101_0203;
    assert!(sf.write_now());
    // At once, with the change pending before it.
    assert_eq!(on_disk(&path).unwrap(), sf.serialize().as_bytes());
    // The delayed write is no longer due, and flush has nothing to write.
    assert!(tokio::time::timeout(NEVER, sf.due()).await.is_err());
    std::fs::remove_file(&path).unwrap();
    sf.flush();
    assert_eq!(on_disk(&path), None);
}

#[tokio::test(start_paused = true)]
async fn flush_writes_the_latest_value_once() {
    let t = TmpDir::new();
    let path = t.0.join("state.conf");
    let mut sf = Store::with_values(path.clone(), values());
    // Nothing changed: nothing written.
    sf.flush();
    assert_eq!(on_disk(&path), None);
    sf.led_speed = 1.5;
    sf.changed();
    sf.led_speed = 2.5;
    sf.changed();
    // The exit flush, before the delay ran out.
    sf.flush();
    assert_eq!(on_disk(&path).unwrap(), sf.serialize().as_bytes());
    assert!(tokio::time::timeout(NEVER, sf.due()).await.is_err());
    std::fs::remove_file(&path).unwrap();
    sf.flush();
    assert_eq!(on_disk(&path), None);
}

#[tokio::test(start_paused = true)]
async fn failed_flush_is_retried() {
    let t = TmpDir::new();
    let dir = t.0.join("later");
    let path = dir.join("state.conf");
    let mut sf = Store::with_values(path.clone(), values());
    sf.changed();
    sf.due().await;
    sf.flush(); // the directory is missing: logged, still pending
    assert_eq!(on_disk(&path), None);
    std::fs::create_dir(&dir).unwrap();
    sf.flush();
    assert_eq!(on_disk(&path).unwrap(), golden("save_values.conf"));
}

#[tokio::test(start_paused = true)]
async fn failed_write_now_reports_false_and_stays_pending() {
    // The install path refuses to replace the binary when update_attempt is not on disk.
    let t = TmpDir::new();
    let dir = t.0.join("later");
    let path = dir.join("state.conf");
    let mut sf = Store::with_values(path.clone(), values());
    assert!(!sf.write_now()); // the directory is missing: nothing written
    assert_eq!(on_disk(&path), None);
    // No delayed write is armed; the change stays pending for the next flush.
    assert!(tokio::time::timeout(NEVER, sf.due()).await.is_err());
    std::fs::create_dir(&dir).unwrap();
    sf.flush();
    assert_eq!(on_disk(&path).unwrap(), golden("save_values.conf"));
    assert!(sf.write_now());
}

// glibc strtoul(s, nullptr, 16) with a 64-bit unsigned long (the Midi; the host).
#[cfg(target_pointer_width = "64")]
#[test]
fn strtoul_hex_matches_libc() {
    for (s, want) in [
        ("", 0),
        ("0", 0),
        ("5001abcd", 0x5001_abcd),
        ("0x5001ABCD", 0x5001_abcd),
        ("0X1f", 0x1f),
        ("0x", 0),
        ("0xg", 0),
        ("  -1", u64::MAX),
        ("+ff", 0xff),
        ("ffffffff", 0xffff_ffff),
        ("1ffffffff", 0x1_ffff_ffff),
        ("fffffffffffffffffff", u64::MAX),
        ("12zz", 0x12),
        ("-0x10", 0u64.wrapping_sub(16)),
        ("\t7", 7),
    ] {
        assert_eq!(strtoul_hex(s), want, "{s:?}");
    }
}

#[test]
fn clock_auto_brightness_is_always_written() {
    let mut s = Settings::default();
    assert!(s.clock_auto_brightness);
    assert!(
        s.serialize()
            .lines()
            .any(|line| line == "clock_auto_brightness=1")
    );
    s.parse(b"clock_auto_brightness=0\n");
    assert!(!s.clock_auto_brightness);
    assert!(
        s.serialize()
            .lines()
            .any(|line| line == "clock_auto_brightness=0")
    );
    s.parse(b"clock_auto_brightness=1\n");
    assert!(s.clock_auto_brightness);
}

#[test]
fn voice_color_defaults_to_violet_and_round_trips() {
    // The stock look; a `.prev` binary skips the unknown key, and an old file has none.
    assert_eq!(Settings::default().voice_color, "Violet");
    assert!(
        Settings::default()
            .serialize()
            .contains("\nvoice_color=Violet\n")
    );
    let mut s = Settings::default();
    s.parse(b"voice_color=Blue\n");
    assert_eq!(s.voice_color, "Blue");
    assert!(s.serialize().contains("\nvoice_color=Blue\n"));
    // Anything else is the stock violet, not an error.
    s.parse(b"voice_color=blue\n");
    assert_eq!(s.voice_color, "Violet");
    s.parse(b"voice_color=\n");
    assert_eq!(s.voice_color, "Violet");
}

/// The persisted option texts are the domain crates' option lists, in their order; a domain
/// name change must fail here, not silently orphan a `state.conf` value.
#[test]
fn option_tables_match_the_domain_crates() {
    assert_eq!(
        MUSIC_STYLES.as_slice(),
        pulsar_hw::MusicStyle::ALL.map(pulsar_hw::MusicStyle::as_str)
    );
    assert_eq!(
        CLOCK_MUSIC_STYLES.as_slice(),
        pulsar_clock::ClockMusicStyle::ALL.map(pulsar_clock::ClockMusicStyle::as_str)
    );
    assert_eq!(
        VOICE_COLORS.as_slice(),
        pulsar_hw::VoiceColor::ALL.map(pulsar_hw::VoiceColor::as_str)
    );
    let roles: Vec<&str> = pulsar_zigbee::ROLES.iter().map(|r| r.name).collect();
    assert_eq!(ZIGBEE_ROLES.as_slice(), roles);
    // The fallbacks are real options, not typos.
    assert!(MUSIC_STYLES.contains(&MUSIC_STYLE_DEFAULT));
    assert!(CLOCK_MUSIC_STYLES.contains(&"Off"));
    assert!(VOICE_COLORS.contains(&"Violet"));
    assert!(ZIGBEE_ROLES.contains(&"off"));
}

#[test]
fn wake_event_switches_round_trip() {
    // Recording is on and dry run off by default; the `.prev` binary skips the unknown keys.
    let s = Settings::default();
    assert!(s.wake_recording);
    assert!(!s.wake_dry_run);
    let mut parsed = Settings::default();
    parsed.parse(b"wake_event_recording=0\nwake_dry_run=1\n");
    assert!(!parsed.wake_recording);
    assert!(parsed.wake_dry_run);
    assert!(parsed.serialize().contains("wake_dry_run=1\n"));
    // The newest fork keys go last, so `wake_dry_run` is no longer the tail.
    assert!(parsed.serialize().ends_with("mic_gain_db=0\n"));
}

#[test]
fn microphone_gain_defaults_roundtrips_and_clamps() {
    // 0 dB is the stock channel as is: an older `state.conf` has no key.
    let mut st = Settings::default();
    assert_eq!(st.mic_gain_db, 0);
    assert!(st.serialize().contains("mic_gain_db=0\n"));
    for db in [0, 12, 34, 40] {
        st.mic_gain_db = db;
        let mut loaded = Settings::default();
        loaded.parse(st.serialize().as_bytes());
        assert_eq!(loaded.mic_gain_db, db);
    }
    for (value, expected) in [("-5", 0), ("99", 40), ("33.6", 34), ("NaN", 0), ("inf", 0)] {
        let mut loaded = Settings::default();
        loaded.parse(format!("mic_gain_db={value}\n").as_bytes());
        assert_eq!(loaded.mic_gain_db, expected, "{value}");
    }
}

#[test]
fn announce_volume_defaults_roundtrips_and_clamps() {
    // 100 % is the old behaviour: an older `state.conf` has no key and reads as 1.0.
    let mut st = Settings::default();
    st.parse(b"unknown_old_setting=1\n");
    assert_eq!(st.announce_volume, 1.0);
    assert!(st.serialize().ends_with("mic_gain_db=0\n"));
    for volume in [0.0, 0.25, 0.55, 1.0] {
        st.announce_volume = volume;
        let mut loaded = Settings::default();
        loaded.parse(st.serialize().as_bytes());
        assert_eq!(loaded.announce_volume, volume);
    }
    for (value, expected) in [
        ("-0.5", 0.0),
        ("1.5", 1.0),
        ("NaN", 1.0),
        ("inf", 1.0),
        ("0.4junk", 0.4),
        ("garbage", 0.0),
    ] {
        st.parse(format!("announce_volume={value}\n").as_bytes());
        assert_eq!(st.announce_volume, expected, "{value}");
    }
}

#[test]
fn followup_duration_defaults_roundtrips_and_clamps() {
    let mut st = Settings::default();
    st.parse(b"unknown_old_setting=1\n");
    assert_eq!(st.followup_duration_s, 10);
    for seconds in [0, 7, 10] {
        st.followup_duration_s = seconds;
        let mut loaded = Settings::default();
        loaded.parse(st.serialize().as_bytes());
        assert_eq!(loaded.followup_duration_s, seconds);
    }
    st.followup_duration_s = 7;
    for value in ["NaN", "inf", "-inf"] {
        st.parse(format!("followup_duration_s={value}\n").as_bytes());
        assert_eq!(st.followup_duration_s, 7, "{value}");
    }
    // Разбор как у остальных чисел — strtof: мусор даёт 0, число в начале строки читается.
    for (value, expected) in [
        ("garbage", 0),
        ("", 0),
        ("7junk", 7),
        ("-1", 0),
        ("7.6", 8),
        ("999999", 10),
    ] {
        st.parse(format!("followup_duration_s={value}\n").as_bytes());
        assert_eq!(st.followup_duration_s, expected, "{value}");
    }
}
