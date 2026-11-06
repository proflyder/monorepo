use super::*;

use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use pulsar_model::{DeviceProfile, profile_from_compatible};
use pulsar_ui::clock::glyph_frame;
use pulsar_ui::font;

#[test]
fn level_math() {
    assert_eq!(level(0.0, 1.0), 0);
    assert_eq!(level(-1.0, 1.0), 0);
    assert_eq!(level(1.0, 1.0), 255);
    assert_eq!(level(0.5, 1.0), 128); // 127.5 rounds away from zero
    assert_eq!(level(1.0, 0.02), 5);
    // Never below the lowest level that still lights while the user wants it on.
    assert_eq!(level(0.001, 0.02), MIN_LEVEL);
    assert_eq!(level(1.0, 2.0), 255);
}

// --- Output on a fake sysfs tree ---

/// A per-test directory under the system temp dir, removed on drop.
struct Root(PathBuf);

impl Root {
    fn led(&self, prefix: &str, name: &str) -> PathBuf {
        let d = self.0.join(format!("{prefix}{name}"));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("brightness");
        std::fs::write(&f, "").unwrap();
        f
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("pulsar-clock-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn base_model(compatible: &[u8]) -> &'static DeviceProfile {
    profile_from_compatible(compatible).unwrap()
}

fn base_panel(compatible: &[u8]) -> &'static PanelLayout {
    base_model(compatible).clock.as_ref().unwrap()
}

/// A stock model whose clock LEDs live under a fresh temp dir ("<root>/<prefix><led>"); one
/// static pair per call site, since `spawn` takes a `&'static PanelLayout`.
macro_rules! temp_model {
    ($name:literal, $compatible:literal, $prefix:literal) => {{
        static DIR: LazyLock<String> =
            LazyLock::new(|| format!("{}/{}", temp_root($name).display(), $prefix));
        static PANEL: LazyLock<PanelLayout> = LazyLock::new(|| PanelLayout {
            dir: DIR.as_str(),
            ..*base_panel($compatible)
        });
        let model: &'static PanelLayout = &PANEL;
        let root = Root(Path::new(DIR.as_str()).parent().unwrap().to_owned());
        (model, root)
    }};
}

/// Contents written since the last call, which empties the file again; "" = no write.
fn take(path: &Path) -> String {
    let s = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, "").unwrap();
    s
}

/// Every clock LED of `m` (Lite 2 layout) created; the segment files per digit.
fn create_lite2(m: &PanelLayout, root: &Root) -> Vec<Vec<PathBuf>> {
    let segs = (0..DIGITS)
        .map(|d| {
            (0..SEGS)
                .map(|s| root.led("clock", &Leds::seg_name(m, d, s)))
                .collect()
        })
        .collect();
    for name in m.colon.iter().chain(m.aux).chain(m.center.iter()) {
        root.led("clock", name);
    }
    segs
}

#[test]
fn writes_only_changes_and_rescales_on_level() {
    let (m, root) = temp_model!("writes", b"yandex,plum\0", "clock");
    let segs = create_lite2(m, &root);
    let mut leds = Leds::open(m).unwrap();
    let led = |name: &str| root.0.join(format!("clock{name}/brightness"));
    // Aux LEDs are switched off once.
    assert_eq!(take(&led("DEGREE")), "0");
    assert_eq!(take(&led("H2")), "0");

    let one = glyph_frame([0, 0, 0, 0x06], true);
    leds.draw(&one);
    assert_eq!(take(&segs[3][1]), "255");
    assert_eq!(take(&segs[3][0]), "0");
    assert_eq!(take(&led("DH")), "255");
    assert_eq!(take(&led("DL")), "255");
    assert_eq!(take(&led("DM")), "0");
    // Unchanged: nothing written.
    leds.draw(&one);
    assert_eq!(take(&segs[3][1]), "");
    assert_eq!(take(&led("DH")), "");
    // The full rewrite.
    leds.forget_written();
    leds.draw(&one);
    assert_eq!(take(&segs[3][1]), "255");
    assert_eq!(take(&segs[0][0]), "0");

    // No chip current on the Lite 2: the level is the value of a lit segment.
    assert!(leds.set_level(level(1.0, 0.02)));
    assert!(!leds.set_level(5));
    leds.draw(&one);
    assert_eq!(take(&segs[3][1]), "5");
    assert_eq!(take(&segs[3][0]), "0");
    let half = Frame {
        center: 128,
        ..Frame::default()
    };
    leds.draw(&half);
    assert_eq!(take(&led("DM")), "3"); // (128 * 5 + 127) / 255
}

#[test]
fn missing_leds_and_chip_current_dedup() {
    let (m, root) = temp_model!("dedup", b"yandex,cucumber\0", "clock-");
    // No segment at all: unavailable.
    assert!(Leds::open(m).is_none());
    assert!(!panel_present(m));

    for d in 0..DIGITS {
        for s in 0..SEGS {
            root.led("clock-", &Leds::seg_name(m, d, s));
        }
    }
    assert!(panel_present(m));
    let chip_a = root.0.join("chip-a");
    let chip_b = root.0.join("chip-b");
    for c in [&chip_a, &chip_b] {
        std::fs::create_dir_all(c).unwrap();
        std::fs::write(c.join("current"), "").unwrap();
    }
    let link = |seg: &str, chip: &Path| {
        std::os::unix::fs::symlink(chip, root.0.join(format!("clock-{seg}/device"))).unwrap();
    };
    // Both outer digits on one chip: its current once.
    link("a1", &chip_a);
    link("a4", &chip_a);
    let leds = Leds::open(m).unwrap();
    assert_eq!(leds.current.len(), 1);
    assert!(leds.colon.is_empty());

    std::fs::remove_file(root.0.join("clock-a4/device")).unwrap();
    link("a4", &chip_b);
    let mut leds = Leds::open(m).unwrap();
    assert_eq!(leds.current.len(), 2);
    assert!(leds.set_level(77));
    assert_eq!(take(&chip_a.join("current")), "77");
    assert_eq!(take(&chip_b.join("current")), "77");
    // Segments stay at full value; the chips dim.
    leds.draw(&glyph_frame([0x01, 0, 0, 0], false));
    assert_eq!(take(&root.0.join("clock-a1/brightness")), "255");
}

#[tokio::test]
async fn actor_draws_commands_and_stops_with_the_handles() {
    let (m, root) = temp_model!("actor", b"yandex,plum\0", "clock");
    let segs = create_lite2(m, &root);
    let local = LocalSet::new();
    local
        .run_until(async {
            let clock = spawn(Some(m));
            let other = clock.clone();
            clock.set_text("1234".into());
            other.show_code("8".into());
            other.clear_code();
            tokio::time::sleep(Duration::from_millis(30)).await;
            let lit = |d: usize| -> u8 {
                (0..SEGS)
                    .map(|s| {
                        let v = std::fs::read_to_string(&segs[d][s]).unwrap();
                        u8::from(v.starts_with("255")) << s
                    })
                    .sum()
            };
            let want = font::text_glyphs(&font::layout_text(b"1234"), 0, 400).0;
            assert_eq!([lit(0), lit(1), lit(2), lit(3)], want);
            drop(clock);
            drop(other);
            assert!(
                pulsar_core::supervisor()
                    .shutdown(Duration::from_secs(1))
                    .await
                    .is_empty()
            );
        })
        .await;
}

#[tokio::test]
async fn auto_brightness_off_ignores_the_light() {
    // Midi layout: the level is the chips' current, written only when it changes (the periodic
    // full rewrite covers the segments alone).
    let (m, root) = temp_model!("auto_bri", b"yandex,cucumber\0", "clock-");
    for d in 0..DIGITS {
        for s in 0..SEGS {
            root.led("clock-", &Leds::seg_name(m, d, s));
        }
    }
    let chip = root.0.join("chip");
    std::fs::create_dir_all(&chip).unwrap();
    let current = chip.join("current");
    std::fs::write(&current, "").unwrap();
    std::os::unix::fs::symlink(&chip, root.0.join("clock-a1/device")).unwrap();
    // The next level the actor writes while this task yields.
    let next = || async {
        for _ in 0..200 {
            let v = std::fs::read_to_string(&current).unwrap();
            if !v.is_empty() {
                std::fs::write(&current, "").unwrap();
                return v;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no level written");
    };
    let local = LocalSet::new();
    local
        .run_until(async {
            let clock = spawn(Some(m));
            assert_eq!(take(&current), "255");
            // Dark room, auto brightness on: 0.02 of full.
            clock.set_ambient_lux(0.0);
            assert_eq!(next().await, "5");
            clock.set_auto_brightness(false);
            assert_eq!(next().await, "255");
            // A reading while off writes nothing but still feeds the dimmer: switching back on
            // applies the new light.
            let mut dimmer = AmbientDimmer::new();
            dimmer.update(0.0);
            assert!(dimmer.update(1000.0));
            let (half, half_lit) = (level(0.5, 1.0), level(0.5, dimmer.factor()));
            assert!(![5, 255].contains(&half) && ![3, 5, 255, half].contains(&half_lit));
            clock.set_ambient_lux(1000.0);
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert_eq!(take(&current), "");
            clock.set_brightness(0.5);
            assert_eq!(next().await, half.to_string());
            clock.set_auto_brightness(true);
            assert_eq!(next().await, half_lit.to_string());
            drop(clock);
            assert!(
                pulsar_core::supervisor()
                    .shutdown(Duration::from_secs(1))
                    .await
                    .is_empty()
            );
        })
        .await;
}

#[tokio::test]
async fn unavailable_clock_is_a_no_op() {
    let (m, _root) = temp_model!("none", b"yandex,plum\0", "none-");
    let local = LocalSet::new();
    local
        .run_until(async {
            let clock = spawn(Some(m));
            clock.set_text("1234".into());
            clock.play(ClockShot::Boot);
            assert!(
                pulsar_core::supervisor()
                    .shutdown(Duration::from_secs(1))
                    .await
                    .is_empty()
            );
        })
        .await;
}

#[test]
fn names() {
    assert_eq!(music_styles(), ["Off", "Levels"]);
    assert_eq!(face_names().len(), 19);
}

use std::cell::RefCell;
use std::rc::Rc;

use tokio::task::LocalSet;

/// Панель-запись: кадры, уровни и число полных перезаписей.
#[derive(Clone, Default)]
struct RecordingPanel {
    frames: Rc<RefCell<Vec<(Frame, i32)>>>,
    invalidations: Rc<RefCell<u32>>,
}

impl ClockPanel for RecordingPanel {
    fn draw(&mut self, frame: &Frame, level: i32) -> Result<(), ClockError> {
        self.frames.borrow_mut().push((*frame, level));
        Ok(())
    }

    fn invalidate(&mut self) {
        *self.invalidations.borrow_mut() += 1;
    }
}

struct Fixed;

impl WallClock for Fixed {
    fn now(&self) -> Wall {
        Wall {
            hour: 12,
            min: 34,
            first_half: true,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn fixed_wall_draws_the_time_and_ticks_rewrite_every_20() {
    LocalSet::new()
        .run_until(async {
            let panel = RecordingPanel::default();
            let frames = Rc::clone(&panel.frames);
            let invalidations = Rc::clone(&panel.invalidations);
            let clock = spawn_with(Box::new(panel), Box::new(Fixed));
            assert_eq!(
                frames.borrow().first().unwrap().0,
                glyph_frame([0x06, 0x5b, 0x4f, 0x66], true)
            );
            assert_eq!(frames.borrow()[0].1, 255);
            // 19 ticks: no forced rewrite; the 20th forgets the cache.
            tokio::time::sleep(Duration::from_millis(19 * 250)).await;
            assert_eq!(*invalidations.borrow(), 0);
            tokio::time::sleep(Duration::from_millis(400)).await;
            assert_eq!(*invalidations.borrow(), 1);
            drop(clock);
            assert!(
                pulsar_core::supervisor()
                    .shutdown(Duration::from_secs(1))
                    .await
                    .is_empty()
            );
        })
        .await;
}
