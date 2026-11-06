use super::*;

use pulsar_core::UiState;

use crate::{Effect, MusicStyle, VoiceColor};

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

#[test]
fn to_px_gamma_dim_cap() {
    let white = Col {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    assert_eq!(to_px(white, 1.0, None), [204, 204, 204]);
    // Gamma 2 after dimming; out-of-range inputs clamp first.
    let c = Col {
        r: 0.5,
        g: 2.0,
        b: -1.0,
    };
    assert_eq!(to_px(c, 1.0, None), [51, 204, 0]);
    assert_eq!(to_px(white, 0.5, None), [51, 51, 51]);
    // Ambient floor: 0.3 in the dark.
    let mut comp = Compositor::new(3, 0);
    assert!((comp.dim() - 1.0).abs() < 1e-6);
    comp.dimmer.update(0.0);
    assert!((comp.dim() - 0.3).abs() < 1e-6);
}

#[test]
fn to_px_case_matrix_keeps_hue_when_clipping() {
    let white = Col {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    let id: Matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    assert_eq!(to_px(white, 1.0, Some(&id)), [204, 204, 204]);
    // Rows are output R, G, B; a row above 1 scales the whole pixel down.
    let warm: Matrix = [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    assert_eq!(to_px(white, 1.0, Some(&warm)), [204, 102, 102]);
    // Negative results clamp to 0.
    let neg: Matrix = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 0.5]];
    assert_eq!(to_px(white, 1.0, Some(&neg)), [204, 0, 102]);
}

fn ui(c: &mut Compositor, s: UiState, now: i64) -> bool {
    c.apply(LedCommand::UiState(s), now)
}

fn user(on: bool) -> LedCommand {
    LedCommand::UserLight {
        on,
        color: RED,
        brightness: 1.0,
        effect: Effect::Solid,
        transition_ms: 0,
    }
}

#[test]
fn layer_priority() {
    let mut c = Compositor::new(3, 0);
    assert_eq!(c.active_layer(0), Layer::Off);
    c.apply(LedCommand::MediaPlaying(true), 0);
    assert_eq!(c.active_layer(0), Layer::Off, "music style Off");
    c.apply(LedCommand::MusicStyle(MusicStyle::Disco), 0);
    assert_eq!(c.active_layer(0), Layer::Music);
    c.apply(user(true), 0);
    assert_eq!(c.active_layer(0), Layer::User);
    // The first mute call only sets the state; on the Lite 2 the steady mute beats the light.
    c.apply(LedCommand::Muted(true), 0);
    assert_eq!(c.active_layer(0), Layer::Mute);
    c.apply(
        LedCommand::ShowTemporary {
            effect: Effect::Comet,
            color: RED,
            seconds: 3,
        },
        0,
    );
    assert_eq!(c.active_layer(0), Layer::Show);
    c.apply(LedCommand::TimerStarted, 0);
    assert_eq!(c.active_layer(0), Layer::OneShot);
    // Listening shows only with voice indication; Error and TimerRing always.
    c.apply(LedCommand::VoiceIndication(false), 0);
    ui(&mut c, UiState::Listening, 0);
    assert_eq!(c.active_layer(0), Layer::OneShot);
    c.apply(LedCommand::VoiceIndication(true), 0);
    assert_eq!(c.active_layer(0), Layer::Voice);
    c.apply(LedCommand::VoiceIndication(false), 0);
    ui(&mut c, UiState::TimerRing, 0);
    assert_eq!(c.active_layer(0), Layer::Voice);
    c.apply(LedCommand::Volume(0.4), 0);
    assert_eq!(c.active_layer(0), Layer::Volume);
    assert_eq!(c.active_layer(VOLUME_SHOW_MS), Layer::Voice);
    // Zero brightness hides the light.
    let mut c = Compositor::new(24, 6);
    c.apply(
        LedCommand::UserLight {
            on: true,
            color: RED,
            brightness: 0.0,
            effect: Effect::Solid,
            transition_ms: 0,
        },
        0,
    );
    assert_eq!(c.active_layer(0), Layer::Off);
    // The Midi has no steady mute layer (its logo shows it).
    c.apply(LedCommand::Muted(true), 0);
    assert_eq!(c.active_layer(0), Layer::Off);
}

#[test]
fn timed_content_expires() {
    let mut c = Compositor::new(24, 6);
    assert!(ui(&mut c, UiState::Error, 1000));
    // Idle during the error is ignored; it runs its 3.54 s to the end.
    assert!(!ui(&mut c, UiState::Idle, 4539));
    assert_eq!(c.next_expiry(4539), Some(4540));
    c.expire(4540);
    assert_eq!(c.state, UiState::Idle);
    // Another active state replaces it at once.
    ui(&mut c, UiState::Error, 5000);
    assert!(ui(&mut c, UiState::Listening, 5001));

    let mut c = Compositor::new(24, 6);
    c.apply(LedCommand::Muted(false), 0);
    assert_eq!(c.sweep, Sweep::None, "first call: no one-shot");
    c.apply(LedCommand::TimerStarted, 100);
    c.apply(LedCommand::Muted(true), 1000);
    assert_eq!(c.sweep, Sweep::MuteOn);
    c.apply(LedCommand::Volume(0.5), 1000);
    c.apply(
        LedCommand::ShowTemporary {
            effect: Effect::Solid,
            color: RED,
            seconds: 0,
        },
        1000,
    );
    assert_eq!(c.next_expiry(1000), Some(1000 + fx::MUTE_SWEEP_MS));
    c.expire(1600);
    assert_eq!(c.sweep, Sweep::None);
    // The countdown went on under the sweep.
    assert_eq!(c.active_layer(2600), Layer::OneShot);
    assert_eq!(c.next_expiry(2600), Some(6000), "seconds <= 0: 5 s");
    c.expire(6000);
    assert!(!c.show_on);
    assert_eq!(c.next_expiry(6000), Some(15100));
    c.expire(15100);
    assert!(!c.timer_shot);
    assert_eq!(c.next_expiry(15100), None);
    c.apply(
        LedCommand::ShowTemporary {
            effect: Effect::Solid,
            color: RED,
            seconds: 99_999,
        },
        0,
    );
    assert_eq!(c.show_until, 3_600_000);

    // Lite 2 unmute: no sweep, and a running mute flash ends with it.
    let mut c = Compositor::new(3, 0);
    c.apply(LedCommand::Muted(false), 0);
    c.apply(LedCommand::Muted(true), 10);
    assert_eq!(c.sweep, Sweep::MuteOn);
    c.apply(LedCommand::Muted(false), 20);
    assert_eq!(c.sweep, Sweep::None);
}

#[test]
fn midi_logo_follows_voice_mute_or_ring_average() {
    let mut c = Compositor::new(24, 6);
    c.apply(user(true), 0);
    c.frame(0);
    let red = to_px(fx::rgb(255, 0, 0), c.dim(), None);
    assert_eq!(&c.pixels()[24 * 3..24 * 3 + 3], &red, "logo = ring average");
    c.apply(LedCommand::Muted(false), 1);
    c.apply(LedCommand::Muted(true), 2);
    c.frame(1000);
    assert_eq!(
        &c.pixels()[24 * 3..24 * 3 + 3],
        &to_px(fx::mute_logo(), c.dim(), None)
    );
    ui(&mut c, UiState::Listening, 1000);
    // The voice layer fades in over 250 ms.
    c.frame(2000);
    c.frame(2300);
    let mut logo = [Col::default(); 6];
    fx::voice_logo(fx::Voice::Listening, 1300, VoiceColor::Violet, &mut logo);
    assert_eq!(
        &c.pixels()[24 * 3..24 * 3 + 3],
        &to_px(logo[0], c.dim(), None)
    );
}

#[test]
fn voice_color_command_repaints_the_voice_layer_and_logo() {
    let mut c = Compositor::new(24, 6);
    ui(&mut c, UiState::Listening, 0);
    c.frame(1000);
    // Violet is the initial value: the command changes nothing.
    assert!(!c.apply(LedCommand::VoiceColor(VoiceColor::Violet), 1000));
    assert!(c.apply(LedCommand::VoiceColor(VoiceColor::Blue), 1000));
    // The same option again is not a change.
    assert!(!c.apply(LedCommand::VoiceColor(VoiceColor::Blue), 1000));
    c.frame(1300);
    let mut ring = [Col::default(); 24];
    let mut logo = [Col::default(); 6];
    fx::voice(fx::Voice::Listening, 1300, VoiceColor::Blue, &mut ring);
    fx::voice_logo(fx::Voice::Listening, 1300, VoiceColor::Blue, &mut logo);
    for (i, w) in ring.iter().chain(&logo).enumerate() {
        assert_eq!(
            &c.pixels()[i * 3..i * 3 + 3],
            &to_px(*w, c.dim(), None),
            "LED {i}"
        );
    }
}

#[test]
fn layer_changes_fade_but_volume_cuts_in() {
    let mut c = Compositor::new(3, 0);
    c.frame(0);
    c.apply(
        LedCommand::ShowTemporary {
            effect: Effect::Solid,
            color: RED,
            seconds: 10,
        },
        0,
    );
    assert!(c.frame(0), "fading");
    assert_eq!(&c.pixels()[..3], &[0, 0, 0]);
    c.frame(125);
    let half = to_px(
        fx::lerp(Col::default(), fx::rgb(255, 0, 0), 0.5),
        c.dim(),
        None,
    );
    assert_eq!(&c.pixels()[..3], &half);
    assert!(!c.frame(250), "fade done, solid colour does not move");
    c.apply(LedCommand::Volume(1.0), 300);
    c.frame(300);
    let mut vol = [Col::default(); 3];
    fx::volume(1.0, &mut vol);
    assert_eq!(&c.pixels()[..3], &to_px(vol[0], c.dim(), None));
}

#[test]
fn music_source_priority_and_freshness() {
    let mut c = Compositor::new(24, 0);
    c.apply(LedCommand::MusicStyle(MusicStyle::Pulse), 0);
    c.apply(LedCommand::MediaPlaying(true), 0);
    let frame = |loudness: f32| MusicFrame {
        loudness,
        ..MusicFrame::default()
    };
    // Sendspin wins while it keeps coming; the speaker reference is ignored.
    assert!(!c.apply(LedCommand::Music(frame(0.8), MusicSource::Sendspin), 0));
    assert_eq!(c.take_source_change(), Some(MusicSource::Sendspin));
    assert!(!c.apply(LedCommand::Music(frame(0.1), MusicSource::Speaker), 100));
    assert!((c.music.frame().loudness - 0.8).abs() < 1e-6);
    assert!(c.music.fresh(100));
    // Once the Sendspin stream goes quiet, the speaker reference takes over.
    assert!(!c.apply(LedCommand::Music(frame(0.1), MusicSource::Speaker), 600));
    assert_eq!(c.take_source_change(), Some(MusicSource::Speaker));
    assert!((c.music.frame().loudness - 0.1).abs() < 1e-6);
    // Frames older than MUSIC_STALE_MS count as none.
    assert!(!c.music.fresh(600 + pulsar_core::MUSIC_STALE_MS));
}
