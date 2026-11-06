use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use pulsar_ui::led_fx;
use pulsar_ui::leds::to_px;
use tokio::task::{JoinHandle, LocalSet};

use super::*;

#[test]
fn matrix_parser_takes_the_key_and_nine_numbers() {
    let stock =
        r#"{"type": "matrix", "matrix": [[0.95, 0.05, 0], [0.0, .9, -0.1], [1e-1, 0.2, 7.5E-1]]}"#;
    assert_eq!(
        parse_matrix(stock),
        Some([[0.95, 0.05, 0.0], [0.0, 0.9, -0.1], [0.1, 0.2, 0.75]])
    );
    // Whitespace before the colon; a "matrix" value is not the key.
    let spaced = "{\"name\": \"matrix\", \"matrix\"\n\t : [1,2,3,4,5,6,7,8,9,10]}";
    assert_eq!(
        parse_matrix(spaced),
        Some([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]])
    );
    // Fewer than nine numbers before the closing brace.
    assert_eq!(
        parse_matrix(r#"{"matrix": [[1,2,3],[4,5,6]]} [7,8,9]"#),
        None
    );
    // A lone '-' or '.' is no number; NaN / infinity are rejected.
    assert_eq!(parse_matrix(r#"{"matrix": [- 1,2,3,4,5,6,7,8,9]}"#), None);
    assert_eq!(parse_matrix(r#"{"matrix": [-nan,2,3,4,5,6,7,8,9]}"#), None);
    assert_eq!(parse_matrix(r#"{"matrix": [1e999,2,3,4,5,6,7,8,9]}"#), None);
    assert_eq!(parse_matrix(r#"{"type": "matrix"}"#), None);
    assert_eq!(parse_matrix(r#"{"matrix": 5}"#), None);
}

/// The fallback path for the stock file: broken JSON but the first nine numbers after the
/// "matrix" key are still read.
#[test]
fn matrix_fallback_reads_the_broken_stock_file() {
    let broken = r#"{"matrix": [[0.95, 0.05, 0], [0.0, .9, -0.1], [1e-1, 0.2, 7.5E-1"#;
    assert_eq!(
        parse_matrix(broken),
        Some([[0.95, 0.05, 0.0], [0.0, 0.9, -0.1], [0.1, 0.2, 0.75]])
    );
    // Valid JSON with another shape still uses the same nine numbers.
    assert_eq!(
        parse_matrix(r#"{"matrix": {"rows": [1,2,3,4,5,6,7,8,9]}}"#),
        Some([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]])
    );
    assert_eq!(parse_matrix(""), None);
}

#[test]
fn case_colors_are_sorted_json_stems() {
    let dir = std::env::temp_dir().join(format!("pulsar-hw-case-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in [
        "white.json",
        "black.json",
        ".json",
        "notes.txt",
        "beige.json",
    ] {
        std::fs::write(dir.join(f), "{}").unwrap();
    }
    let got = case_colors_in(&format!("{}/", dir.display()));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(got, ["beige", "black", "white"]);
    assert!(case_colors_in("/nonexistent/").is_empty());
}

/// (time, pixels) of the writes a test sink saw.
type Captured = Rc<RefCell<Vec<(i64, Vec<u8>)>>>;

/// Тестовый приёмник: записывает каждый кадр (RGB) и время от своего старта.
struct Recording {
    frames: Captured,
    epoch: Instant,
}

impl LedSink for Recording {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError> {
        let now = i64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(i64::MAX);
        let bytes = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        self.frames.borrow_mut().push((now, bytes));
        Ok(())
    }
}

/// Приёмник с переключаемым отказом; `attempts` считает все попытки.
struct Flaky {
    inner: Recording,
    fail: Rc<Cell<bool>>,
    attempts: Rc<Cell<u32>>,
}

impl LedSink for Flaky {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError> {
        self.attempts.set(self.attempts.get() + 1);
        if self.fail.get() {
            return Err(LedError::Io(io::Error::from(io::ErrorKind::Other)));
        }
        self.inner.write(px)
    }
}

fn spawn_with_sink(n: usize, nlogo: usize, sink: Box<dyn LedSink>) -> (LedsHandle, JoinHandle<()>) {
    let out = Output::new(sink, n, nlogo);
    let (tx, rx) = unbounded_channel();
    let task = tokio::task::spawn_local(run(Compositor::new(n, nlogo), out, rx, true));
    (LedsHandle { tx }, task)
}

/// The actor on a recording sink.
fn spawn_capture(n: usize, nlogo: usize) -> (LedsHandle, Captured, JoinHandle<()>) {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let sink = Recording {
        frames: Rc::clone(&frames),
        epoch: Instant::now(),
    };
    let (leds, task) = spawn_with_sink(n, nlogo, Box::new(sink));
    (leds, frames, task)
}

async fn sleep_ms(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// A failed static write is retried after 1 s even though nothing changes.
#[tokio::test(start_paused = true)]
async fn failed_static_frame_is_retried_after_a_second() {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let fail = Rc::new(Cell::new(true));
    let attempts = Rc::new(Cell::new(0));
    let sink = Flaky {
        inner: Recording {
            frames: Rc::clone(&frames),
            epoch: Instant::now(),
        },
        fail: Rc::clone(&fail),
        attempts: Rc::clone(&attempts),
    };
    let mut out = Output::new(Box::new(sink), 3, 0);
    let mut comp = Compositor::new(3, 0);
    comp.start_at(0);
    comp.apply(
        LedCommand::UserLight {
            on: true,
            color: Rgb { r: 255, g: 0, b: 0 },
            brightness: 1.0,
            effect: Effect::Solid,
            transition_ms: 0,
        },
        0,
    );
    let mut timers = Timers::new();
    frame(&mut comp, &mut out, &mut timers, 0);
    assert_eq!(approvals(&frames), 0, "the write failed");
    assert!(timers.is_set(TimerKey::Retry), "retry armed");
    assert_eq!(attempts.get(), 1);
    fail.set(false);
    assert_eq!(timers.next().await, TimerKey::Retry);
    frame(&mut comp, &mut out, &mut timers, 1000);
    assert_eq!(approvals(&frames), 1, "the retry was written");
}

fn approvals(frames: &Captured) -> usize {
    frames.borrow().len()
}

#[tokio::test(start_paused = true)]
async fn error_runs_out_then_the_ring_goes_dark_and_frames_stop() {
    LocalSet::new()
        .run_until(async {
            let (leds, frames, task) = spawn_capture(24, 6);
            leds.set_ui_state(UiState::Error);
            sleep_ms(1000).await;
            leds.set_ui_state(UiState::Idle);
            sleep_ms(1500).await;
            let lit = |px: &[u8]| px.iter().any(|v| *v != 0);
            assert!(
                frames.borrow().iter().any(|(t, px)| *t > 2000 && lit(px)),
                "still red after the ignored Idle"
            );
            sleep_ms(1500).await;
            let count = frames.borrow().len();
            let (t, px) = frames.borrow().last().cloned().unwrap();
            assert!(!lit(&px));
            assert!(t >= 3540 && t < 3540 + pulsar_ui::leds::FADE_MS + 2 * FRAME_MS as i64);
            sleep_ms(5000).await;
            assert_eq!(frames.borrow().len(), count, "no frames while static");
            drop(leds);
            task.await.unwrap();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn volume_overlay_cuts_in_and_its_expiry_restores_the_light() {
    LocalSet::new()
        .run_until(async {
            let (leds, frames, task) = spawn_capture(3, 0);
            let dim = Compositor::new(3, 0).dim();
            let red = to_px(led_fx::rgb(255, 0, 0), dim, None).repeat(3);
            leds.set_user_light(true, Rgb { r: 255, g: 0, b: 0 }, 1.0, Effect::Solid, 0);
            sleep_ms(500).await;
            assert_eq!(frames.borrow().last().unwrap().1, red, "transition 0: cut");
            leds.show_volume(0.5);
            sleep_ms(10).await;
            let mut vol = [led_fx::Col::default(); 3];
            led_fx::volume(0.5, &mut vol);
            let (t0, px) = frames.borrow().last().cloned().unwrap();
            assert_eq!(px, to_px(vol[0], dim, None).repeat(3));
            sleep_ms(2000).await;
            let f = frames.borrow().clone();
            let (t, px) = f.last().unwrap();
            assert_eq!(*px, red);
            // The expiry timer woke the static overlay up after 1.5 s (the fade's first frame
            // still equals the overlay), then the 250 ms fade.
            let first_after = f.iter().find(|(t, _)| *t > t0).unwrap().0;
            assert_eq!(
                first_after,
                t0 + pulsar_ui::leds::VOLUME_SHOW_MS + FRAME_MS as i64
            );
            assert!(
                *t <= t0
                    + pulsar_ui::leds::VOLUME_SHOW_MS
                    + pulsar_ui::leds::FADE_MS
                    + FRAME_MS as i64
            );
            drop(leds);
            task.await.unwrap();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn exit_clears_and_case_matrix_rewrites() {
    LocalSet::new()
        .run_until(async {
            let (leds, frames, task) = spawn_capture(3, 0);
            leds.set_user_light(true, Rgb { r: 255, g: 0, b: 0 }, 1.0, Effect::Solid, 0);
            sleep_ms(500).await;
            let before = frames.borrow().len();
            // Same pixels under the identity matrix: written again anyway.
            leds.tx
                .send(Msg::Ui(LedCommand::CaseMatrix(Some([
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                ]))))
                .unwrap();
            sleep_ms(10).await;
            assert_eq!(frames.borrow().len(), before + 1);
            let clone = leds.clone();
            drop(leds);
            sleep_ms(10).await;
            assert!(!task.is_finished(), "a clone keeps it running");
            drop(clone);
            task.await.unwrap();
            assert_eq!(frames.borrow().last().unwrap().1, [0; 9]);
        })
        .await;
}

/// The case colour is read by the actor; a missing file leaves the correction off and does not
/// block the handle.
#[tokio::test(start_paused = true)]
async fn case_color_file_is_read_by_the_actor() {
    LocalSet::new()
        .run_until(async {
            let (leds, frames, task) = spawn_capture(3, 0);
            leds.set_case_color(CaseColor::new("missing"));
            sleep_ms(10).await;
            // The start frame and the repaint after the command.
            assert_eq!(frames.borrow().len(), 2, "one repaint, no correction");
            drop(leds);
            task.await.unwrap();
        })
        .await;
}
