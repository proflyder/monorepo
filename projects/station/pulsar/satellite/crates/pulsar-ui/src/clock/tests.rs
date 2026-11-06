use super::*;

use pulsar_core::MusicFrame;
use pulsar_core::{MUSIC_STALE_MS, SENDSPIN_FRESH_MS};

use crate::ClockFace;
use crate::MusicSource;

const NOW: i64 = 1_000_000;

fn wall() -> Wall {
    Wall {
        hour: 12,
        min: 34,
        first_half: true,
    }
}

fn time_frame() -> Frame {
    glyph_frame(font::time_glyphs(12, 34), true)
}

fn text_frame(s: &str) -> Frame {
    let (g, colon) = font::text_glyphs(&font::layout_text(s.as_bytes()), 0, 400);
    glyph_frame(g, colon)
}

fn bands(x: f32) -> MusicFrame {
    MusicFrame {
        bands: [x; 8],
        ..MusicFrame::default()
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn layer_priority() {
    let mut l = ClockLayers::new();
    assert_eq!(l.compose(NOW, wall), (time_frame(), 0));

    // Music levels: style on, media playing, a fresh frame.
    l.levels_on = true;
    l.media = true;
    l.music_frame(&bands(0.5), MusicSource::Speaker, NOW);
    let (levels, next) = l.compose(NOW, wall);
    assert_eq!(levels, clock_fx::levels(&l.mus_in));
    assert_ne!(levels, Frame::default());
    assert_eq!(next, MUSIC_FRAME_MS);

    l.set_text("Abc", NOW);
    assert_eq!(l.compose(NOW, wall), (text_frame("Abc"), 0));

    l.show_temporary_face(ClockFace::Static(0), 10, NOW);
    let face = glyph_frame(font::face_glyphs(0, 0), false);
    assert_eq!(l.compose(NOW, wall), (face, 10_000));

    l.set_timer(65_000, true, NOW);
    let timer = glyph_frame(font::timer_glyphs(65), true);
    assert_eq!(l.compose(NOW, wall), (timer, 0));

    l.play(ClockShot::Pause, NOW);
    assert_eq!(
        l.compose(NOW, wall),
        (clock_fx::render(Anim::Pause, 0), FX_FRAME_MS)
    );

    l.updating = true;
    assert_eq!(l.compose(NOW, wall), (Frame::default(), 0));

    // LISTENING only with voice indication, TIMER_RING always.
    l.set_voice_indication(false, NOW);
    l.set_ui_state(UiState::Listening, NOW);
    assert_eq!(l.compose(NOW, wall), (Frame::default(), 0));
    l.set_voice_indication(true, NOW);
    assert_eq!(
        l.compose(NOW, wall),
        (clock_fx::render(Anim::Listening, 0), FX_FRAME_MS)
    );
    l.set_voice_indication(false, NOW);
    l.set_ui_state(UiState::TimerRing, NOW);
    assert_eq!(
        l.compose(NOW, wall),
        (clock_fx::render(Anim::Ringing, 0), FX_FRAME_MS)
    );

    l.flash_number(42, NOW);
    let flash = glyph_frame(font::number_glyphs(42), false);
    assert_eq!(l.compose(NOW, wall), (flash, FLASH_MS));

    l.enabled = false;
    assert_eq!(l.compose(NOW, wall), (Frame::default(), 0));

    // The pairing code shows even when disabled.
    l.show_code("1234".into(), NOW);
    let code = glyph_frame(font::code_glyphs(b"1234", 0, CODE_PAGE_MS), false);
    assert_eq!(l.compose(NOW, wall), (code, 0));

    // Peel the layers off again.
    l.clear_code();
    assert_eq!(l.compose(NOW, wall).0, Frame::default());
    l.enabled = true;
    assert_eq!(l.compose(NOW, wall).0, flash);
    l.flash_until = 0;
    assert_eq!(l.compose(NOW, wall).0, clock_fx::render(Anim::Ringing, 0));
    l.set_ui_state(UiState::Idle, NOW);
    assert_eq!(l.compose(NOW, wall).0, Frame::default());
    l.updating = false;
    assert_eq!(l.compose(NOW, wall).0, clock_fx::render(Anim::Pause, 0));
    l.shot = None;
    assert_eq!(l.compose(NOW, wall).0, timer);
    l.set_timer(0, true, NOW);
    assert_eq!(l.compose(NOW, wall).0, face);
    l.show_temporary_face(ClockFace::None, 0, NOW);
    assert_eq!(l.compose(NOW, wall).0, text_frame("Abc"));
    l.set_text("", NOW);
    l.music_frame(&bands(0.5), MusicSource::Speaker, NOW);
    assert_eq!(l.compose(NOW, wall).0, clock_fx::levels(&l.mus_in));
    l.media = false;
    assert_eq!(l.compose(NOW, wall).0, time_frame());
}

#[test]
fn error_state_shows_nothing_of_its_own() {
    let mut l = ClockLayers::new();
    l.set_ui_state(UiState::Error, NOW);
    assert_eq!(l.compose(NOW, wall).0, time_frame());
    // Changing the state to the same animation keeps its phase.
    l.set_ui_state(UiState::Thinking, NOW);
    l.set_ui_state(UiState::Thinking, NOW + 500);
    assert_eq!(l.voice_t0, NOW);
}

#[test]
fn one_shot_ends_and_frame_steps() {
    let mut l = ClockLayers::new();
    l.play(ClockShot::Next, NOW);
    assert_eq!(l.compose(NOW + 7, wall).1, 13);
    // The last frame step is cut at the end of the animation.
    assert_eq!(l.compose(NOW + 1495, wall).1, 5);
    assert_eq!(l.compose(NOW + 1499, wall).1, 1);
    assert_eq!(l.compose(NOW + 1500, wall), (time_frame(), 0));
    assert_eq!(l.shot, None);
}

#[test]
fn timer_countdown() {
    let mut l = ClockLayers::new();
    l.set_timer(65_000, true, NOW);
    let shows = |l: &mut ClockLayers, at: i64| l.compose(at, wall).0;
    assert_eq!(
        shows(&mut l, NOW),
        glyph_frame(font::timer_glyphs(65), true)
    );
    // Whole seconds are rounded up.
    assert_eq!(
        shows(&mut l, NOW + 1),
        glyph_frame(font::timer_glyphs(65), true)
    );
    assert_eq!(
        shows(&mut l, NOW + 1000),
        glyph_frame(font::timer_glyphs(64), true)
    );
    assert_eq!(
        shows(&mut l, NOW + 64_001),
        glyph_frame(font::timer_glyphs(1), true)
    );
    // At 00:00 the digits and the colon blink every 500 ms (NOW is a multiple of 1000).
    let zero = glyph_frame(font::timer_glyphs(0), true);
    assert_eq!(shows(&mut l, NOW + 65_000), zero);
    assert_eq!(shows(&mut l, NOW + 65_499), zero);
    assert_eq!(shows(&mut l, NOW + 65_500), Frame::default());
    assert_eq!(shows(&mut l, NOW + 66_000), zero);
    assert_eq!(shows(&mut l, NOW + 99_700), Frame::default());

    // Paused: the time left stays.
    l.set_timer(3_600_000, false, NOW);
    assert_eq!(
        shows(&mut l, NOW + 50_000),
        glyph_frame(font::timer_glyphs(3600), true)
    );
    l.set_timer(-5, true, NOW);
    assert_eq!(shows(&mut l, NOW), time_frame());
}

#[test]
fn flash_number_clamps_and_expires() {
    let mut l = ClockLayers::new();
    l.flash_number(123_456, NOW);
    assert_eq!(l.flash_value, 9999);
    l.flash_number(-5000, NOW);
    assert_eq!(l.flash_value, -999);
    assert_eq!(l.compose(NOW + 1000, wall).1, 500);
    assert_eq!(l.compose(NOW + 1500, wall), (time_frame(), 0));
}

#[test]
fn text_and_face_replace_each_other() {
    let mut l = ClockLayers::new();
    l.set_face(ClockFace::Static(3), NOW);
    // Clearing the text keeps the face; text of only ':' replaces it.
    l.set_text("", NOW);
    assert!(matches!(l.base, Some(Content::Face { face: 3, .. })));
    l.set_text(":", NOW);
    assert_eq!(l.base, None);
    l.set_text("hi", NOW);
    l.set_face(ClockFace::None, NOW);
    assert!(matches!(l.base, Some(Content::Text { .. })));
    l.set_face(ClockFace::All, NOW);
    assert!(matches!(
        l.base,
        Some(Content::Face {
            face: font::FACE_ALL,
            ..
        })
    ));
    l.set_text("", NOW);
    assert!(l.base.is_some());
}

#[test]
fn moving_content_frame_steps() {
    let mut l = ClockLayers::new();
    l.set_scroll_ms(50);
    assert_eq!(l.scroll_ms, 100);
    l.set_scroll_ms(5000);
    assert_eq!(l.scroll_ms, 2000);
    l.set_scroll_ms(300);
    l.set_text("12345", NOW);
    assert_eq!(l.compose(NOW + 50, wall).1, 250);
    l.set_text("1234", NOW);
    assert_eq!(l.compose(NOW + 50, wall).1, 0);
    l.set_face(ClockFace::Blink, NOW);
    assert_eq!(l.compose(NOW + 30, wall).1, 70);
    l.set_face(ClockFace::All, NOW);
    assert_eq!(l.compose(NOW + 30, wall).1, 1970);
}

#[test]
fn temporary_layer_durations() {
    let mut l = ClockLayers::new();
    l.set_scroll_ms(300);
    // One pass of a scrolling text: (n + 4) steps.
    l.show_temporary_text("123456", 0, NOW);
    assert_eq!(l.overlay_until, NOW + 10 * 300);
    // The frame timer also fires when the layer ends.
    assert_eq!(l.compose(NOW + 2950, wall).1, 50);
    assert_eq!(l.compose(NOW + 3000, wall).0, time_frame());
    assert_eq!(l.overlay, None);
    l.show_temporary_text("12", 0, NOW);
    assert_eq!(l.overlay_until, NOW + TEMPORARY_MS);
    assert_eq!(l.compose(NOW + 4000, wall).1, 1000);
    l.show_temporary_text("12", 7, NOW);
    assert_eq!(l.overlay_until, NOW + 7000);
    l.show_temporary_text("12", 100_000, NOW);
    assert_eq!(l.overlay_until, NOW + 3_600_000);
    l.show_temporary_face(ClockFace::Static(2), -1, NOW);
    assert_eq!(l.overlay_until, NOW + TEMPORARY_MS);
    l.show_temporary_text("", 5, NOW);
    assert_eq!(l.overlay, None);
    // A layer that ends right now still gets a 1 ms frame timer.
    l.show_temporary_face(ClockFace::Static(2), 1, NOW);
    assert_eq!(l.compose(NOW + 999, wall).1, 1);
}

#[test]
fn level_ranges_per_source() {
    let mut l = ClockLayers::new();
    // The first frame of a source starts its range: the minimum span around the value.
    l.music_frame(&bands(0.5), MusicSource::Speaker, NOW);
    assert!(l.mus_in.iter().all(|&v| close(v, 0.5)));
    // Silence stays dark instead of showing the middle of the meter.
    let mut s = ClockLayers::new();
    s.music_frame(&bands(0.0), MusicSource::Speaker, NOW);
    assert_eq!(s.mus_in, [0.0; 4]);

    // Peaks widen the range at once.
    l.music_frame(&bands(0.9), MusicSource::Speaker, NOW + 33);
    assert!(l.mus_in.iter().all(|&v| close(v, 1.0)));
    let r = l.ranges[0];
    assert!(close(r.hi[0], 0.9));
    let k = (-33.0f32 / RANGE_TAU_MS).exp();
    assert!(close(r.lo[0], 0.9 + (0.5 - 0.9) * k));
    // The range closes in with a step of at most 100 ms across a gap, and survives a pause.
    l.music_frame(&bands(0.7), MusicSource::Speaker, NOW + 33 + 9_999);
    let k = (-100.0f32 / RANGE_TAU_MS).exp();
    let r2 = l.ranges[0];
    assert!(close(r2.hi[0], 0.7 + (0.9 - 0.7) * k));
    assert!(close(r2.lo[0], 0.7 + (r.lo[0] - 0.7) * k));
    let span = r2.hi[0] - r2.lo[0];
    assert!(close(l.mus_in[0], (0.7 - r2.lo[0]) / span));
    // 10 s without the source: it starts over.
    l.music_frame(
        &bands(0.2),
        MusicSource::Speaker,
        NOW + 33 + 9_999 + RANGE_KEEP_MS,
    );
    assert!(close(l.ranges[0].lo[0], 0.2) && close(l.ranges[0].hi[0], 0.2));

    // Bands are averaged pairwise per digit.
    let mut l = ClockLayers::new();
    let f = MusicFrame {
        bands: [0.0, 0.2, 0.4, 0.4, 0.6, 0.6, 1.0, 1.0],
        ..MusicFrame::default()
    };
    l.music_frame(&f, MusicSource::Sendspin, NOW);
    let r = l.ranges[1];
    assert!(close(r.lo[0], 0.1) && close(r.lo[1], 0.4) && close(r.lo[3], 1.0));
    // A full-scale value keeps the span inside 0..1.
    assert!(close(l.mus_in[3], 1.0));
    assert_eq!(l.ranges[0].last_ms, i64::MIN / 2);
}

#[test]
fn sendspin_frames_win_while_fresh() {
    let mut l = ClockLayers::new();
    l.music_frame(&bands(0.5), MusicSource::Sendspin, NOW);
    let shown = l.mus_in;
    l.music_frame(
        &bands(0.0),
        MusicSource::Speaker,
        NOW + SENDSPIN_FRESH_MS - 1,
    );
    assert_eq!(l.mus_in, shown);
    assert_eq!(l.music.frame().bands, [0.5; 8]);
    assert_eq!(l.ranges[0].last_ms, i64::MIN / 2);
    l.music_frame(&bands(0.0), MusicSource::Speaker, NOW + SENDSPIN_FRESH_MS);
    assert_eq!(l.mus_in, [0.0; 4]);
    assert!(l.music.fresh(NOW + SENDSPIN_FRESH_MS));
}

#[test]
fn meters_cycle_with_the_time_and_fall() {
    let mut l = ClockLayers::new();
    l.levels_on = true;
    l.media = true;
    let feed = |l: &mut ClockLayers, at: i64, x: f32| {
        l.ranges = [Range::default(); 2];
        l.music_frame(&bands(x), MusicSource::Speaker, at);
        l.mus_in = [x; 4];
    };
    feed(&mut l, NOW, 0.8);
    assert_eq!(l.compose(NOW, wall).0, clock_fx::levels(&[0.8; 4]));
    assert_eq!(l.viz_t0, NOW);
    // The meters fall with a 250 ms time constant.
    feed(&mut l, NOW + 250, 0.0);
    let k = (-1.0f32).exp();
    assert_eq!(
        l.compose(NOW + 250, wall).0,
        clock_fx::levels(&[0.8 * k; 4])
    );
    // Rises show at once.
    feed(&mut l, NOW + 290, 0.9);
    assert_eq!(l.compose(NOW + 290, wall).0, clock_fx::levels(&[0.9; 4]));
    // The time takes over for the last 3 s of every 30 s.
    feed(&mut l, NOW + 26_999, 0.9);
    assert_ne!(l.compose(NOW + 26_999, wall).0, time_frame());
    feed(&mut l, NOW + 27_000, 0.9);
    assert_eq!(l.compose(NOW + 27_000, wall), (time_frame(), 0));
    feed(&mut l, NOW + 30_000, 0.9);
    assert_eq!(l.compose(NOW + 30_000, wall).1, MUSIC_FRAME_MS);
    // Stale frames end the display; the next start resets the meters and the cycle.
    assert_eq!(
        l.compose(NOW + 30_000 + MUSIC_STALE_MS, wall).0,
        time_frame()
    );
    assert!(!l.viz);
    feed(&mut l, NOW + 40_000, 0.3);
    assert_eq!(l.compose(NOW + 40_000, wall).0, clock_fx::levels(&[0.3; 4]));
    assert_eq!(l.viz_t0, NOW + 40_000);
    // Media stopped.
    l.media = false;
    assert_eq!(l.compose(NOW + 40_010, wall).0, time_frame());
}
