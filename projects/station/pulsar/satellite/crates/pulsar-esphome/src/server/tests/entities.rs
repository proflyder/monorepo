//! Сущности: проверки команд на границах (как C++-сервер), подписка и replay состояний,
//! гейтинг по `Capabilities`, голосовые события.

use esphome_proto::entity::{FOLLOWUP_DURATION_MAX_S, key};
use esphome_proto::pb::msg;
use tokio::sync::mpsc::UnboundedReceiver;

use super::*;

/// Ключ состояния (field 1, fixed32) и ключ сущности в ListEntities (field 2, fixed32).
fn state_key(m: &[u8]) -> Option<u32> {
    field(m, 1).map(Field::u32)
}

fn entity_keys(m: &[u8]) -> Option<u32> {
    field(m, 2).map(Field::u32)
}

/// Все события, накопившиеся после round trip.
async fn drain(p: &mut Peer, rx: &mut UnboundedReceiver<ApiEvent>) -> Vec<ApiEvent> {
    p.sync().await;
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

async fn list_keys(p: &mut Peer) -> Vec<u32> {
    p.send(msg::LIST_ENTITIES_REQ, &[]).await;
    let mut keys = Vec::new();
    loop {
        let (t, m) = p.recv().await.unwrap();
        if t == msg::LIST_DONE {
            return keys;
        }
        keys.push(entity_keys(&m).unwrap());
    }
}

fn action(k: u32, args: &[(Option<&str>, Option<i32>, Option<i32>)]) -> ExecuteServiceRequest {
    ExecuteServiceRequest {
        key: Some(k),
        args: args
            .iter()
            .map(|(s, sint, legacy)| ExecuteServiceArgument {
                bool_: None,
                legacy_int: *legacy,
                float_: None,
                string_: s.map(str::to_owned),
                int_: *sint,
            })
            .collect(),
    }
}

/// Строковый аргумент действия.
fn s(v: &str) -> (Option<&str>, Option<i32>, Option<i32>) {
    (Some(v), None, None)
}

#[tokio::test]
async fn commands_are_validated_like_cxx() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            p.hello("HA").await;
            event(&mut rx).await;

            // Числа: NaN отброшен, значения обрезаны диапазоном сущности.
            for (k, v) in [
                (key::NUMBER_CLOCK_BRI.get(), f32::NAN),
                (key::NUMBER_CLOCK_BRI.get(), 150.0),
                (key::NUMBER_CLOCK_BRI.get(), -5.0),
                (key::NUMBER_CLOCK_SCROLL.get(), 50.0),
                (key::NUMBER_CLOCK_SCROLL.get(), 5000.0),
                (key::NUMBER_LED_SPEED.get(), 0.1),
                (key::NUMBER_LED_SPEED.get(), 9.0),
                (999, 1.0),
            ] {
                p.send_msg(
                    msg::NUMBER_CMD,
                    &NumberCommandRequest {
                        key: Some(k),
                        state: Some(v),
                    },
                )
                .await;
            }
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::Number(NumberId::ClockBrightness, 100.0),
                    ApiEvent::Number(NumberId::ClockBrightness, 0.0),
                    ApiEvent::Number(NumberId::ClockScroll, 100.0),
                    ApiEvent::Number(NumberId::ClockScroll, 2000.0),
                    ApiEvent::Number(NumberId::LedSpeed, 0.25),
                    ApiEvent::Number(NumberId::LedSpeed, 4.0),
                ]
            );

            // Текст: не больше 64 символов, на декодированном тексте (как считал C++).
            let text = |b: Vec<u8>| TextCommandRequest {
                key: Some(key::TEXT_CLOCK.get()),
                state: Some(b),
            };
            p.send_msg(msg::TEXT_CMD, &text("я".repeat(64).into_bytes()))
                .await;
            p.send_msg(msg::TEXT_CMD, &text("я".repeat(65).into_bytes()))
                .await;
            let mut odd = b"a".to_vec();
            odd.extend_from_slice(&[0x80; 63]);
            p.send_msg(msg::TEXT_CMD, &text(odd.clone())).await;
            let mut too_odd = odd.clone();
            too_odd.push(0x80);
            p.send_msg(msg::TEXT_CMD, &text(too_odd)).await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::Text(TextId::ClockText, "я".repeat(64)),
                    ApiEvent::Text(
                        TextId::ClockText,
                        String::from_utf8_lossy(&odd).into_owned()
                    ),
                ]
            );

            // Select: только опции сущности; switch: только известные ключи.
            let select = |k: u32, s: &str| SelectCommandRequest {
                key: Some(k),
                state: Some(s.to_owned()),
            };
            for (k, s) in [
                (key::SELECT_CLOCK_FACE.get(), "Smiley"),
                (key::SELECT_CLOCK_FACE.get(), "Frown"),
                (key::SELECT_MUSIC_VIZ.get(), "Spectrum"),
                (key::SELECT_CASE_COLOR.get(), "Spectrum"),
                (key::SELECT_ZIGBEE_MODE.get(), "Router"),
                (key::SELECT_CLOCK_MUSIC.get(), "Levels"),
                (key::SELECT_VOICE_COLOR.get(), "Blue"),
                (key::SELECT_VOICE_COLOR.get(), "Green"),
            ] {
                p.send_msg(msg::SELECT_CMD, &select(k, s)).await;
            }
            let switch = |k: u32, v: Option<bool>| SwitchCommandRequest {
                key: Some(k),
                state: v,
            };
            p.send_msg(
                msg::SWITCH_CMD,
                &switch(key::SW_NEAREST_ANSWERS.get(), Some(true)),
            )
            .await;
            p.send_msg(
                msg::SWITCH_CMD,
                &switch(key::SW_VOICE_INDICATION.get(), None),
            )
            .await;
            p.send_msg(
                msg::SWITCH_CMD,
                &switch(key::SW_WAKE_PREROLL.get(), Some(true)),
            )
            .await;
            p.send_msg(
                msg::SWITCH_CMD,
                &switch(key::SW_CLOCK_AUTO_BRI.get(), Some(true)),
            )
            .await;
            p.send_msg(msg::SWITCH_CMD, &switch(key::MEDIA.get(), Some(true)))
                .await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::Select(SelectId::ClockFace, "Smiley".into()),
                    ApiEvent::Select(SelectId::MusicViz, "Spectrum".into()),
                    ApiEvent::Select(SelectId::ZigbeeMode, "Router".into()),
                    ApiEvent::Select(SelectId::ClockMusic, "Levels".into()),
                    ApiEvent::Select(SelectId::VoiceColor, "Blue".into()),
                    ApiEvent::Switch(SwitchId::NearestAnswers, true),
                    ApiEvent::Switch(SwitchId::VoiceIndication, false),
                    ApiEvent::Switch(SwitchId::WakePreroll, true),
                    ApiEvent::Switch(SwitchId::ClockAutoBrightness, true),
                ]
            );

            // Media: громкость обрезана, неизвестная команда отброшена, поля только заданные.
            p.send_msg(
                msg::MEDIA_PLAYER_CMD,
                &MediaPlayerCommandRequest {
                    key: Some(key::MEDIA.get()),
                    has_command: Some(true),
                    command: Some(99),
                    has_volume: Some(true),
                    volume: Some(1.5),
                    has_media_url: None,
                    media_url: None,
                    has_announcement: None,
                    announcement: None,
                },
            )
            .await;
            p.send_msg(
                msg::MEDIA_PLAYER_CMD,
                &MediaPlayerCommandRequest {
                    key: Some(key::MEDIA.get()),
                    has_command: Some(true),
                    command: Some(0),
                    has_volume: None,
                    volume: None,
                    has_media_url: Some(true),
                    media_url: Some("http://x/a.mp3".into()),
                    has_announcement: Some(true),
                    announcement: Some(true),
                },
            )
            .await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::MediaCommand(MediaCommandRequest {
                        command: None,
                        volume: Some(1.0),
                        media_url: None,
                        announcement: None,
                    }),
                    ApiEvent::MediaCommand(MediaCommandRequest {
                        command: Some(MediaCommand::Play),
                        volume: None,
                        media_url: Some("http://x/a.mp3".into()),
                        announcement: Some(true),
                    }),
                ]
            );

            // Update: ключ сущности и команды 0..2.
            for (k, cmd) in [
                (key::UPDATE.get(), 2),
                (key::UPDATE.get(), 3),
                (key::MEDIA.get(), 1),
            ] {
                p.send_msg(
                    msg::UPDATE_CMD,
                    &UpdateCommandRequest {
                        key: Some(k),
                        command: Some(cmd),
                    },
                )
                .await;
            }
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [ApiEvent::Update(UpdateCommand::Check)]
            );

            // Actions: число аргументов, sint32 и legacy int, проверки эффекта и лица.
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_CLOCK_TEXT.get(),
                    &[s("Hi"), (None, Some(-5), None)],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(key::ACTION_CLOCK_TEXT.get(), &[s("Hi")]),
            )
            .await;
            let long = "x".repeat(65);
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_CLOCK_TEXT.get(),
                    &[s(&long), (None, Some(1), None)],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_CLOCK_FACE.get(),
                    &[s("Smiley"), (None, None, Some(7))],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_CLOCK_FACE.get(),
                    &[s("Frown"), (None, None, Some(7))],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_CLOCK_FACE.get(),
                    &[s(""), (None, None, Some(0))],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_LED_SHOW.get(),
                    &[s("None"), s("#ff0000"), (None, Some(3), None)],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_LED_SHOW.get(),
                    &[s("Strobe"), s("#ff0000"), (None, Some(3), None)],
                ),
            )
            .await;
            p.send_msg(
                msg::EXECUTE_SERVICE_REQ,
                &action(
                    key::ACTION_LED_SHOW.get(),
                    &[s("Pulse"), s("red"), (None, Some(3), None)],
                ),
            )
            .await;
            p.send_msg(msg::EXECUTE_SERVICE_REQ, &action(999, &[s("a"), s("b")]))
                .await;
            let act = |id, v: &str, seconds| ApiEvent::Action {
                id,
                value: v.into(),
                seconds,
            };
            let show = |e: &str, color: &str| ApiEvent::LedShow {
                effect: e.into(),
                color: color.into(),
                seconds: 3,
            };
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    act(ActionId::ClockShowText, "Hi", -5),
                    act(ActionId::ClockShowFace, "Smiley", 7),
                    act(ActionId::ClockShowFace, "", 0),
                    show("", "#ff0000"),
                    show("Pulse", "red"),
                ]
            );

            // Свет: rgb масштабируется color_brightness, effect "None" -> "", переход по
            // умолчанию и с ограничением; сервер отдаёт нормализованное состояние.
            p.send_msg(
                msg::LIGHT_CMD,
                &LightCommandRequest {
                    key: Some(key::LIGHT_RING.get()),
                    has_state: Some(true),
                    state: Some(true),
                    has_brightness: None,
                    brightness: None,
                    has_color_brightness: Some(true),
                    color_brightness: Some(0.5),
                    has_rgb: Some(true),
                    red: Some(1.0),
                    green: Some(0.5),
                    blue: Some(2.0),
                    has_effect: Some(true),
                    effect: Some("None".into()),
                    has_transition_length: None,
                    transition_length: None,
                },
            )
            .await;
            p.send_msg(
                msg::LIGHT_CMD,
                &LightCommandRequest {
                    key: Some(key::LIGHT_RING.get()),
                    has_state: None,
                    state: None,
                    has_brightness: None,
                    brightness: None,
                    has_color_brightness: Some(true),
                    color_brightness: Some(1.0),
                    has_rgb: None,
                    red: None,
                    green: None,
                    blue: None,
                    has_effect: None,
                    effect: None,
                    has_transition_length: Some(true),
                    transition_length: Some(600_000),
                },
            )
            .await;
            // NaN до светодиодов не доходит: команда отброшена целиком.
            p.send_msg(
                msg::LIGHT_CMD,
                &LightCommandRequest {
                    key: Some(key::LIGHT_RING.get()),
                    has_state: Some(true),
                    state: Some(true),
                    has_brightness: Some(true),
                    brightness: Some(f32::NAN),
                    has_color_brightness: None,
                    color_brightness: None,
                    has_rgb: None,
                    red: None,
                    green: None,
                    blue: None,
                    has_effect: None,
                    effect: None,
                    has_transition_length: None,
                    transition_length: None,
                },
            )
            .await;
            let light = |r, g, b, transition_ms| ApiEvent::Light {
                state: LightState {
                    on: true,
                    brightness: 1.0,
                    r,
                    g,
                    b,
                    effect: String::new(),
                },
                transition_ms,
            };
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [light(0.5, 0.25, 0.5, 1000), light(1.0, 0.5, 1.0, 60_000)]
            );

            // Buttons: Zigbee (сущности есть) и Restart.
            for k in [
                key::BUTTON_ZIGBEE_LEAVE.get(),
                key::BUTTON_RESTART.get(),
                key::MEDIA.get(),
            ] {
                p.send_msg(msg::BUTTON_CMD, &ButtonCommandRequest { key: Some(k) })
                    .await;
            }
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [ApiEvent::Button(ButtonId::ZigbeeLeave), ApiEvent::Restart]
            );

            // Конфигурация голосового ассистента: одно известное wake word.
            p.send_msg(
                msg::VA_SET_CONFIG,
                &VoiceAssistantSetConfiguration {
                    active_wake_words: ["nope", "yandex", "alisa"]
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                },
            )
            .await;
            p.send_msg(
                msg::VA_SET_CONFIG,
                &VoiceAssistantSetConfiguration {
                    active_wake_words: vec!["nope".into()],
                },
            )
            .await;
            // Таймеры: неизвестные значения события отброшены.
            for ev in [3u32, 9] {
                p.send_msg(
                    msg::VA_TIMER_EVENT_RESP,
                    &VoiceAssistantTimerEventResponse {
                        event_type: Some(ev),
                        timer_id: Some("t1".into()),
                        name: None,
                        total_seconds: None,
                        seconds_left: Some(42),
                        is_active: Some(true),
                    },
                )
                .await;
            }
            // События пайплайна только от владельца голоса (здесь его нет).
            p.send_msg(
                msg::VA_EVENT_RESP,
                &VoiceAssistantEventResponse {
                    event_type: Some(1),
                    data: Vec::new(),
                },
            )
            .await;
            p.send_msg(
                msg::VA_ANNOUNCE_REQ,
                &VoiceAssistantAnnounceRequest {
                    media_id: Some("http://x/a.mp3".into()),
                    text: Some("Привет".into()),
                    preannounce_media_id: None,
                    start_conversation: Some(true),
                },
            )
            .await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::ActiveWakeWords(vec!["yandex".into()]),
                    ApiEvent::Timer(TimerInfo {
                        event: TimerEvent::Finished,
                        id: "t1".into(),
                        name: String::new(),
                        total_seconds: 0,
                        seconds_left: 42,
                        is_active: true,
                    }),
                    ApiEvent::Announce(AnnounceRequest {
                        media_id: "http://x/a.mp3".into(),
                        text: "Привет".into(),
                        preannounce_media_id: String::new(),
                        start_conversation: true,
                    }),
                ]
            );
        })
        .await;
}

#[tokio::test]
async fn voice_events_come_from_the_owner() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            p.hello("HA").await;
            event(&mut rx).await;
            p.subscribe_voice(true).await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [ApiEvent::VoiceSubscribed(true)]
            );
            let ev = |t: u32, data: &[(&str, &str)]| VoiceAssistantEventResponse {
                event_type: Some(t),
                data: data
                    .iter()
                    .map(|(k, v)| VoiceAssistantEventData {
                        name: Some((*k).to_owned()),
                        value: Some((*v).to_owned()),
                    })
                    .collect(),
            };
            p.send_msg(msg::VA_EVENT_RESP, &ev(4, &[("text", "a"), ("text", "b")]))
                .await;
            p.send_msg(msg::VA_EVENT_RESP, &ev(13, &[])).await;
            p.send_msg(msg::VA_EVENT_RESP, &ev(100, &[])).await;
            assert_eq!(
                drain(&mut p, &mut rx).await,
                [
                    ApiEvent::VoiceEvent {
                        event: VaEvent::SttEnd,
                        data: [("text".to_owned(), "b".to_owned())].into(),
                    },
                    // Тип новее этой сборки всё равно доходит до контроллера (watchdog).
                    ApiEvent::VoiceEvent {
                        event: VaEvent::Unknown(13),
                        data: Default::default(),
                    },
                    ApiEvent::VoiceEvent {
                        event: VaEvent::IntentProgress,
                        data: Default::default(),
                    },
                ]
            );
        })
        .await;
}

#[tokio::test]
async fn states_go_to_subscribers_and_zigbee_entities_need_modes() {
    local()
        .run_until(async {
            let mut cfg = selftest_config(None);
            cfg.capabilities.zigbee = false;
            cfg.zigbee_modes.clear();
            let mut ts = TestServer::start(cfg);
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            p.hello("HA").await;
            event(&mut rx).await;
            // Без подписки на состояния ничего не пушится, события отброшены.
            h.publish_switch(SwitchId::Microphones, true);
            h.fire_event(EventId::Button, "action");
            assert!(p.sync().await.is_empty());
            // Zigbee-сущностей, состояний и кнопок нет.
            let keys = list_keys(&mut p).await;
            assert!(!keys.contains(&key::SELECT_ZIGBEE_MODE.get()));
            assert!(!keys.contains(&key::TEXT_ZIGBEE.get()));
            assert!(!keys.contains(&key::BUTTON_ZIGBEE_JOIN.get()));
            p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
            let states = p.sync().await;
            // Кэшированное значение пришло, Zigbee-состояний нет.
            let mic = states
                .iter()
                .find(|(t, m)| {
                    *t == msg::SWITCH_STATE && state_key(m) == Some(key::SW_MICROPHONES.get())
                })
                .unwrap();
            assert!(field(&mic.1, 2).unwrap().boolean());
            assert!(
                !states
                    .iter()
                    .any(|(_, m)| state_key(m) == Some(key::TEXT_ZIGBEE.get()))
            );
            // После подписки публикации и события доходят; Zigbee text sensor — нет.
            h.publish_text_sensor(TextSensorId::Zigbee, "x");
            h.publish_sensor(SensorId::Illuminance, 12.5);
            h.fire_event(EventId::Button, "action_long");
            h.publish_update(&UpdateState {
                current_version: "v1".into(),
                latest_version: "v2".into(),
                ..UpdateState::default()
            });
            let got = p.sync().await;
            let types: Vec<u32> = got.iter().map(|(t, _)| *t).collect();
            assert_eq!(
                types,
                [msg::SENSOR_STATE, msg::EVENT_RESP, msg::UPDATE_STATE]
            );
            assert_eq!(field(&got[0].1, 2).unwrap().flt(), 12.5);
            assert_eq!(field(&got[1].1, 2).unwrap().str(), "action_long");
            assert_eq!(field(&got[2].1, 7).unwrap().str(), "v2");
            // Zigbee-кнопки игнорируются без сущностей.
            p.send_msg(
                msg::BUTTON_CMD,
                &ButtonCommandRequest {
                    key: Some(key::BUTTON_ZIGBEE_JOIN.get()),
                },
            )
            .await;
            assert!(drain(&mut p, &mut rx).await.is_empty());
        })
        .await;
}

#[tokio::test]
async fn an_unchanged_switch_state_is_published_every_time() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            p.hello("HA").await;
            event(&mut rx).await;
            p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
            let _ = p.sync().await;
            // HA asked for "on" under the hardware mute: the controller publishes the real
            // "off" after every command, so the server must not deduplicate identical states.
            h.publish_switch(SwitchId::Microphones, false);
            h.publish_switch(SwitchId::Microphones, false);
            let got = p.sync().await;
            let off = got
                .iter()
                .filter(|(t, m)| {
                    *t == msg::SWITCH_STATE
                        && state_key(m) == Some(key::SW_MICROPHONES.get())
                        // `state = false` is encoded as an absent field, as in ESPHome.
                        && field(m, 2).is_none()
                })
                .count();
            assert_eq!(off, 2, "{got:?}");
        })
        .await;
}

#[tokio::test]
async fn mini_has_no_clock_lux_mute_case_or_button_entities() {
    local()
        .run_until(async {
            let mut cfg = selftest_config(None);
            cfg.capabilities = Capabilities {
                clock: false,
                lux: false,
                auto_brightness: false,
                wake_preroll: false,
                hw_mic_mute: false,
                buttons: false,
                case_colors: false,
                voice_colors: true,
                zigbee: false,
            };
            cfg.case_colors = vec!["None".into()];
            cfg.zigbee_modes.clear();
            let mut ts = TestServer::start(cfg);
            let h = ts.api.clone();
            let mut rx = ts.take_events();
            let mut p = ts.connect();
            p.hello("HA").await;
            event(&mut rx).await;
            let keys = list_keys(&mut p).await;
            // 22 сущности плюс сервис led_show; без часов, Lux, wake pre-roll, аппаратного
            // MUTE, цвета корпуса и кнопок Button, но с follow-up duration и microphone gain.
            // Два switch и две кнопки wake-event и select голосового цвета есть всегда.
            assert_eq!(keys.len(), 23, "keys: {keys:?}");
            assert!(keys.contains(&key::NUMBER_FOLLOWUP_DURATION.get()));
            assert!(keys.contains(&key::NUMBER_MIC_GAIN.get()));
            assert!(keys.contains(&key::ACTION_LED_SHOW.get()));
            let absent = [
                key::SW_CLOCK,
                key::SW_CLOCK_AUTO_BRI,
                key::SW_WAKE_PREROLL,
                key::NUMBER_CLOCK_BRI,
                key::NUMBER_CLOCK_SCROLL,
                key::TEXT_CLOCK,
                key::SELECT_CLOCK_FACE,
                key::SELECT_CLOCK_MUSIC,
                key::ACTION_CLOCK_TEXT,
                key::ACTION_CLOCK_FACE,
                key::BS_HW_MIC_MUTE,
                key::SENSOR_LUX,
                key::SELECT_CASE_COLOR,
                key::EVENT_BUTTON,
            ];
            for k in absent {
                assert!(!keys.contains(&k.get()), "unexpected entity key {k:?}");
            }
            // Начальные состояния несут только перечисленные сущности.
            publish_selftest_states(&h);
            p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
            let initial: Vec<u32> = p
                .sync()
                .await
                .iter()
                .filter_map(|(_, m)| state_key(m))
                .collect();
            assert!(
                initial.contains(&key::NUMBER_LED_SPEED.get()),
                "{initial:?}"
            );
            for k in absent {
                assert!(
                    !initial.contains(&k.get()),
                    "unexpected state for key {k:?}"
                );
            }
            // Контроллер может публиковать эти значения; сервер обязан их отбросить.
            h.publish_switch(SwitchId::Clock, true);
            h.publish_switch(SwitchId::ClockAutoBrightness, true);
            h.publish_switch(SwitchId::WakePreroll, true);
            h.publish_number(NumberId::ClockBrightness, 50.0);
            h.publish_number(NumberId::ClockScroll, 400.0);
            h.publish_text(TextId::ClockText, "x");
            h.publish_select(SelectId::ClockFace, "None");
            h.publish_select(SelectId::ClockMusic, "Off");
            h.publish_select(SelectId::CaseColor, "None");
            h.publish_binary_sensor(BinarySensorId::HwMicMute, false);
            h.publish_sensor(SensorId::Illuminance, 12.5);
            h.fire_event(EventId::Button, "action");
            assert!(p.sync().await.is_empty());
            // Перечисленная сущность состояние получает.
            h.publish_number(NumberId::LedSpeed, 2.0);
            let got = p.sync().await;
            assert_eq!(got.len(), 1, "{got:?}");
            assert_eq!(got[0].0, msg::NUMBER_STATE);
            assert_eq!(state_key(&got[0].1), Some(key::NUMBER_LED_SPEED.get()));
        })
        .await;
}

#[tokio::test]
async fn clock_auto_brightness_needs_the_clock_and_the_light_sensor() {
    // Часы, чей датчик света не открылся; датчик света без панели часов (Миди 2, где чип часов
    // не завёлся). Illuminance перечислен в обоих случаях.
    for (has_clock, has_auto_brightness) in [(true, false), (false, true)] {
        local()
            .run_until(async {
                let mut cfg = selftest_config(None);
                cfg.capabilities.clock = has_clock;
                cfg.capabilities.auto_brightness = has_auto_brightness;
                let mut ts = TestServer::start(cfg);
                let h = ts.api.clone();
                let mut rx = ts.take_events();
                let mut p = ts.connect();
                p.hello("HA").await;
                event(&mut rx).await;
                publish_selftest_states(&h);
                let keys = list_keys(&mut p).await;
                assert_eq!(keys.contains(&key::SW_CLOCK.get()), has_clock);
                assert!(keys.contains(&key::SENSOR_LUX.get()));
                assert!(!keys.contains(&key::SW_CLOCK_AUTO_BRI.get()));
                p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
                let states = p.sync().await;
                assert!(
                    !states
                        .iter()
                        .any(|(_, m)| state_key(m) == Some(key::SW_CLOCK_AUTO_BRI.get()))
                );
                h.publish_switch(SwitchId::ClockAutoBrightness, false);
                h.publish_number(NumberId::LedSpeed, 2.0);
                let got = p.sync().await;
                assert_eq!(got.len(), 1);
                assert_eq!(state_key(&got[0].1), Some(key::NUMBER_LED_SPEED.get()));
            })
            .await;
    }
}

#[tokio::test]
async fn entity_list_flood_yields_to_other_clients() {
    local()
        .run_until(async {
            let mut ts = TestServer::start(selftest_config(None));
            let mut rx = ts.take_events();
            let mut flood = ts.connect();
            flood.hello("flood").await;
            let mut other = ts.connect();
            other.hello("other").await;
            event(&mut rx).await;
            event(&mut rx).await;
            // 300 ListEntities в одной записи: ~1 MiB ответов; другой клиент обслуживается,
            // ничего не теряется.
            let mut req = Vec::new();
            for _ in 0..300 {
                append_plain(&mut req, msg::LIST_ENTITIES_REQ, &[]);
            }
            flood.write(&req).await;
            assert!(other.sync().await.is_empty());
            let mut done = 0;
            while done < 300 {
                if flood.recv().await.unwrap().0 == msg::LIST_DONE {
                    done += 1;
                }
            }
            assert!(flood.sync().await.is_empty());
            assert!(other.sync().await.is_empty());
            while rx.try_recv().is_ok() {}
        })
        .await;
}

#[tokio::test]
async fn eof_with_pending_batch_still_answers_everything() {
    local()
        .run_until(async {
            let ts = TestServer::start(selftest_config(None));
            let mut p = ts.connect();
            p.hello("eof").await;
            let mut req = Vec::new();
            for _ in 0..300 {
                append_plain(&mut req, msg::LIST_ENTITIES_REQ, &[]);
            }
            p.write(&req).await;
            p.s.shutdown().await.unwrap();
            let mut done = 0;
            while let Some((t, _)) = p.recv().await {
                if t == msg::LIST_DONE {
                    done += 1;
                }
            }
            assert_eq!(done, 300);
        })
        .await;
}

#[tokio::test]
async fn followup_duration_is_universal_config_and_replays_its_state() {
    for has_clock in [true, false] {
        local()
            .run_until(async {
                let mut cfg = selftest_config(None);
                cfg.capabilities.clock = has_clock;
                let mut ts = TestServer::start(cfg);
                let h = ts.api.clone();
                let mut rx = ts.take_events();
                let mut p = ts.connect();
                p.hello("HA").await;
                event(&mut rx).await;
                p.send(msg::LIST_ENTITIES_REQ, &[]).await;
                let mut found = false;
                loop {
                    let (t, m) = p.recv().await.unwrap();
                    if t == msg::LIST_DONE {
                        break;
                    }
                    if t == msg::LIST_NUMBER
                        && entity_keys(&m) == Some(key::NUMBER_FOLLOWUP_DURATION.get())
                    {
                        found = true;
                        assert_eq!(field(&m, 1).unwrap().str(), "followup_duration");
                        assert_eq!(field(&m, 3).unwrap().str(), "Follow-up duration");
                        assert_eq!(field(&m, 6).map_or(0.0, Field::flt), 0.0);
                        assert_eq!(field(&m, 7).unwrap().flt(), FOLLOWUP_DURATION_MAX_S as f32);
                        assert_eq!(field(&m, 8).unwrap().flt(), 1.0);
                        assert_eq!(field(&m, 10).unwrap().u32(), 1); // категория config
                        assert_eq!(field(&m, 11).unwrap().str(), "s");
                    }
                }
                assert!(found);
                h.publish_number(NumberId::FollowupDuration, 10.0);
                p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
                let initial = p.sync().await;
                let (_, m) = initial
                    .iter()
                    .find(|(t, m)| {
                        *t == msg::NUMBER_STATE
                            && state_key(m) == Some(key::NUMBER_FOLLOWUP_DURATION.get())
                    })
                    .unwrap();
                assert_eq!(field(m, 2).unwrap().flt(), 10.0);
                for (input, expected) in [
                    (-1.0, Some(0.0)),
                    (0.0, Some(0.0)),
                    (7.6, Some(8.0)),
                    (50.0, Some(10.0)),
                    (f32::NAN, None),
                    (f32::INFINITY, None),
                ] {
                    p.send_msg(
                        msg::NUMBER_CMD,
                        &NumberCommandRequest {
                            key: Some(key::NUMBER_FOLLOWUP_DURATION.get()),
                            state: Some(input),
                        },
                    )
                    .await;
                    let got = drain(&mut p, &mut rx).await;
                    assert_eq!(
                        got,
                        expected
                            .map(|v| ApiEvent::Number(NumberId::FollowupDuration, v))
                            .into_iter()
                            .collect::<Vec<_>>()
                    );
                }
                for value in [0.0, 7.0] {
                    h.publish_number(NumberId::FollowupDuration, value);
                    let pushed = p.sync().await;
                    let (_, m) = pushed
                        .iter()
                        .find(|(_, m)| state_key(m) == Some(key::NUMBER_FOLLOWUP_DURATION.get()))
                        .unwrap();
                    assert_eq!(field(m, 2).map_or(0.0, Field::flt), value);
                    p.send(msg::SUBSCRIBE_STATES_REQ, &[]).await;
                    let replayed = p.sync().await;
                    let (_, m) = replayed
                        .iter()
                        .find(|(_, m)| state_key(m) == Some(key::NUMBER_FOLLOWUP_DURATION.get()))
                        .unwrap();
                    assert_eq!(field(m, 2).map_or(0.0, Field::flt), value);
                }
            })
            .await;
    }
}
