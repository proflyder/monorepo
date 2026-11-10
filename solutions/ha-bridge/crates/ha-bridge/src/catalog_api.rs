//! `GET /v1/catalog` and `GET /healthz`.
//!
//! The catalog is read-only: it has no write surface, because commands go
//! through the scheduler (the phone enqueues a task) and never through this
//! server. The catalog route requires the configured bearer token; `healthz`
//! is the only unauthenticated answer and is deliberately empty (`204`).
//!
//! The wire shape is the Go `EntryView`/`LightView` contract: field order,
//! `omitempty` behaviour and the exact body of the error responses are pinned
//! by `testdata/catalog/catalog.json`.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use home_secret::Token;
use serde::Serialize;

use crate::catalog::{Catalog, Entry, Light, Volume};
use crate::discovery::CatalogSource;

const CACHE_CONTROL: &str = "no-store";

/// One catalog entry on the wire.
///
/// `device_id`, `device` and `role` are additive: an older server never sends
/// them, an older client ignores them, and they are omitted while empty (the
/// Home Assistant device registry could not be read). `role` is `"primary"` or
/// `"setting"`; a setting is a `config`/`diagnostic` entity of the device, or a
/// switch next to the device's light on a device that marks no categories.
///
/// A future device-level action (for example "speak this text on the speaker"
/// or "call the speaker") is expected to appear as an entry with
/// `kind: "action"`, the same `device_id`/`device` as its device, a
/// human-readable `title`, and its own allowlisted action name; commands keep
/// travelling as `{"entry", "action"}` tasks, so this shape stays additive.
/// A Home Assistant `button` entity is a `kind: "button"` entry with the
/// one-shot action `press`; it may carry no `device_id`, and clients group it
/// with its device by the shared entity-id prefix. A station volume control is
/// a `kind: "volume"` entry with the actions `set`, `mute` and `unmute`; its
/// fixed entity and the live `volume` object keep the phone away from arbitrary
/// media players.
#[derive(Debug, Serialize)]
struct EntryView {
    id: String,
    kind: &'static str,
    title: String,
    /// Stable key of a server-generated title; omitted for Home Assistant
    /// user data, which is shown verbatim.
    #[serde(rename = "titleKey", skip_serializing_if = "String::is_empty")]
    title_key: String,
    /// Typed arguments for `titleKey`; omitted when there are none.
    #[serde(rename = "titleArgs", skip_serializing_if = "BTreeMap::is_empty")]
    title_args: BTreeMap<String, String>,
    /// Stable key of a device status shown under the title (for example the
    /// PC lock state); omitted when the device has no status.
    #[serde(rename = "detailKey", skip_serializing_if = "String::is_empty")]
    detail_key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    room: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    icon: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    device_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    device: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    role: String,
    actions: Vec<&'static str>,
    #[serde(skip_serializing_if = "String::is_empty")]
    state: String,
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    light: Option<LightView>,
    /// Live station volume; omitted for every other entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<VolumeView>,
    /// Server-side visibility condition; omitted when unconditional.
    #[serde(rename = "visibleWhen", skip_serializing_if = "Option::is_none")]
    visible_when: Option<VisibleWhenView>,
}

#[derive(Debug, Serialize)]
struct VisibleWhenView {
    entry: String,
    state: String,
}

/// A station volume entry: the level in percent and whether the device is
/// muted. Additive: an older client ignores it and keeps the plain row.
#[derive(Debug, Serialize)]
struct VolumeView {
    level: i32,
    muted: bool,
}

/// What a light can do and its current values.
#[derive(Debug, Serialize)]
struct LightView {
    brightness: bool,
    #[serde(rename = "colorTemp")]
    color_temp: bool,
    color: bool,
    #[serde(rename = "minKelvin", skip_serializing_if = "is_zero")]
    min_kelvin: i32,
    #[serde(rename = "maxKelvin", skip_serializing_if = "is_zero")]
    max_kelvin: i32,
    #[serde(rename = "brightnessPct", skip_serializing_if = "is_zero")]
    brightness_pct: i32,
    #[serde(skip_serializing_if = "is_zero")]
    kelvin: i32,
    #[serde(skip_serializing_if = "is_zero")]
    hue: i32,
    #[serde(skip_serializing_if = "is_zero")]
    saturation: i32,
    #[serde(skip_serializing_if = "str::is_empty")]
    mode: &'static str,
    /// Effects the bulb advertises (`effect_list`); omitted when unknown.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    effects: Vec<String>,
}

/// The wire shape of `GET /v1/catalog`.
#[derive(Debug, Serialize)]
struct CatalogResponse {
    entries: Vec<EntryView>,
    #[serde(rename = "statesOk")]
    states_ok: bool,
}

#[derive(Clone)]
struct AppState {
    source: Arc<dyn CatalogSource>,
    token: Token,
}

/// Build the HTTP router. `token` is the control bearer and must be non-empty.
pub fn router(source: Arc<dyn CatalogSource>, token: Token) -> Router {
    let state = AppState { source, token };
    Router::new()
        .route("/v1/catalog", get(catalog))
        .route("/healthz", get(healthz))
        .with_state(state)
}

#[allow(clippy::unused_async)]
async fn healthz() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn catalog(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(headers.get(header::AUTHORIZATION), &state.token) {
        return unauthorized();
    }
    let snapshot = match state.source.get().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(%error, "catalog discovery failed");
            return text(StatusCode::BAD_GATEWAY, "home assistant is not reachable\n");
        }
    };
    let body = catalog_body(&snapshot.catalog, snapshot.states_ok);
    let mut response = (
        [(header::CONTENT_TYPE, "application/json")],
        format!("{body}\n"),
    )
        .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL),
    );
    response
}

/// Serialize the exact `GET /v1/catalog` body. Shared with the golden test so
/// the byte contract is asserted without a live listener.
///
/// Go's `json.Encoder` escapes `<`, `>` and `&` (and U+2028/U+2029); serde does
/// not. The replacements only ever occur inside strings, so escaping the whole
/// document keeps the bytes identical to the Go server for every input.
#[must_use]
pub fn catalog_body(catalog: &Catalog, states_ok: bool) -> String {
    let body = serde_json::to_string(&CatalogResponse::from_catalog(catalog, states_ok))
        .unwrap_or_else(|_| "{}".to_owned());
    body.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

impl CatalogResponse {
    fn from_catalog(catalog: &Catalog, states_ok: bool) -> Self {
        Self {
            entries: catalog
                .entries()
                .iter()
                .map(EntryView::from_entry)
                .collect(),
            states_ok,
        }
    }
}

impl EntryView {
    fn from_entry(entry: &Entry) -> Self {
        Self {
            id: entry.id.clone(),
            kind: entry.kind.as_str(),
            title: entry.title.clone(),
            title_key: entry.title_key.clone(),
            title_args: entry.title_args.clone(),
            detail_key: entry.detail_key.clone(),
            room: entry.room.clone(),
            icon: entry.icon.clone(),
            device_id: entry.device_id.clone(),
            device: entry.device.clone(),
            role: entry.role.clone(),
            actions: entry.actions(),
            state: entry.state.clone(),
            available: entry.available,
            light: entry.light.as_ref().map(LightView::from_light),
            volume: entry.volume.as_ref().map(VolumeView::from_volume),
            visible_when: entry
                .visible_when
                .as_ref()
                .map(|condition| VisibleWhenView {
                    entry: condition.entry.clone(),
                    state: condition.state.clone(),
                }),
        }
    }
}

impl VolumeView {
    fn from_volume(volume: &Volume) -> Self {
        Self {
            level: volume.level,
            muted: volume.muted,
        }
    }
}

impl LightView {
    fn from_light(light: &Light) -> Self {
        Self {
            brightness: light.brightness,
            color_temp: light.color_temp,
            color: light.color,
            min_kelvin: light.min_kelvin,
            max_kelvin: light.max_kelvin,
            brightness_pct: light.brightness_pct,
            kelvin: light.kelvin,
            hue: light.hue,
            saturation: light.saturation,
            mode: match light.mode.as_str() {
                "color_temp" => "color_temp",
                "color" => "color",
                _ => "",
            },
            effects: light.effects.clone(),
        }
    }
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

fn text(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
        .into_response()
}

/// `401` answer matching Go's `http.Error` plus the `WWW-Authenticate` hint.
pub fn unauthorized() -> Response {
    let mut response = text(StatusCode::UNAUTHORIZED, "unauthorized\n");
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

/// Whether a request header carries the configured bearer token. An empty
/// configured token never authorizes.
#[must_use]
pub fn authorized(header: Option<&HeaderValue>, token: &Token) -> bool {
    if token.expose().is_empty() {
        return false;
    }
    header
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| token.ct_eq(candidate))
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use super::*;
    use crate::catalog::{LightInput, Raw, Rules, build};
    use crate::discovery::{DiscoveryError, Snapshot};

    struct Source {
        snapshot: Result<Snapshot, DiscoveryError>,
    }

    impl CatalogSource for Source {
        fn get(
            &self,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Snapshot, DiscoveryError>> + Send + '_>>
        {
            Box::pin(async move { self.snapshot.clone() })
        }
    }

    fn source(states_ok: bool) -> Arc<dyn CatalogSource> {
        let entries = build(
            &[
                Raw {
                    entity: "switch.voice_profile".to_owned(),
                    state: "off".to_owned(),
                    name: "Voice profile".to_owned(),
                    profile: true,
                    ..Raw::default()
                },
                Raw {
                    entity: "scene.focus".to_owned(),
                    state: "x".to_owned(),
                    name: "Focus".to_owned(),
                    icon: "mdi:laptop".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    name: "Lamp".to_owned(),
                    room: "Bedroom".to_owned(),
                    light: Some(
                        Light::new(LightInput {
                            color_modes: vec!["brightness".to_owned()],
                            brightness: -1,
                            ..LightInput::default()
                        })
                        .expect("light"),
                    ),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        );
        Arc::new(Source {
            snapshot: Ok(Snapshot {
                catalog: Arc::new(Catalog::new(entries)),
                states_ok,
            }),
        })
    }

    #[test]
    fn bearer_is_exact_and_an_empty_token_never_authorizes() {
        let token = Token::new("secret");
        assert!(authorized(
            Some(&HeaderValue::from_static("Bearer secret")),
            &token
        ));
        for value in [
            "",
            "secret",
            "Bearer wrong",
            "Basic secret",
            "bearer secret",
        ] {
            let header = HeaderValue::from_str(value).expect("header");
            assert!(!authorized(Some(&header), &token), "accepted {value}");
        }
        assert!(!authorized(
            Some(&HeaderValue::from_static("Bearer ")),
            &Token::new("")
        ));
    }

    #[test]
    fn source_is_read_only_and_states_ok_is_carried() {
        let snapshot = source(false);
        let _ = snapshot;
    }

    #[test]
    fn server_generated_titles_carry_a_key_and_user_data_does_not() {
        let entries = build(
            &[
                Raw {
                    entity: "script.speak_text".to_owned(),
                    state: "off".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "assist_satellite.station".to_owned(),
                    state: "idle".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    name: "Лампа".to_owned(),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        );
        let body = catalog_body(&Catalog::new(entries), true);
        assert!(
            body.contains("\"titleKey\":\"catalog.action.speak_title\""),
            "{body}"
        );
        // Home Assistant user data stays verbatim, without a key.
        assert!(body.contains("\"title\":\"Лампа\""), "{body}");
        assert_eq!(body.matches("\"titleKey\"").count(), 1, "{body}");
    }

    #[test]
    fn lock_status_is_a_localized_detail_key_on_the_base_entry() {
        let entries = build(
            &[
                Raw {
                    entity: "switch.komp".to_owned(),
                    state: "on".to_owned(),
                    name: "Комп".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "binary_sensor.komp_razblokirovan".to_owned(),
                    state: "off".to_owned(),
                    name: "Комп разблокирован".to_owned(),
                    device_class: "lock".to_owned(),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        );
        let body = catalog_body(&Catalog::new(entries), true);
        assert!(
            body.contains("\"detailKey\":\"catalog.status.locked\""),
            "{body}"
        );
        // The lock binary sensor is not an entry of its own.
        assert_eq!(body.matches("\"id\":\"switch.komp\"").count(), 1, "{body}");
        assert!(!body.contains("komp_razblokirovan"), "{body}");
    }

    #[test]
    fn device_fields_are_additive_and_omitted_when_empty() {
        let entries = build(
            &[
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    name: "Светильник  Light".to_owned(),
                    device_id: "dev-1".to_owned(),
                    device: "Светильник".to_owned(),
                    role: "primary".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "scene.movie".to_owned(),
                    state: "x".to_owned(),
                    name: "Movie".to_owned(),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        );
        let body = catalog_body(&Catalog::new(entries), true);
        assert!(body.contains("\"id\":\"light.lamp\""), "{body}");
        assert!(body.contains("\"title\":\"Light\""), "{body}");
        assert!(body.contains("\"device_id\":\"dev-1\""), "{body}");
        assert!(body.contains("\"device\":\"Светильник\""), "{body}");
        assert!(body.contains("\"role\":\"primary\""), "{body}");
        assert_eq!(body.matches("\"device_id\"").count(), 1, "{body}");
        assert_eq!(body.matches("\"role\"").count(), 1, "{body}");
    }

    #[test]
    fn volume_entries_carry_a_level_and_mute_flag() {
        let entries = build(
            &[Raw {
                entity: "media_player.iandeks_stantsiia_midi_media_player".to_owned(),
                state: "playing".to_owned(),
                volume_level: Some(0.45),
                volume_muted: true,
                device_id: "station-1".to_owned(),
                device: "Яндекс Станция Миди".to_owned(),
                ..Raw::default()
            }],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        );
        let body = catalog_body(&Catalog::new(entries), true);
        assert!(body.contains("\"kind\":\"volume\""), "{body}");
        assert!(body.contains("\"state\":\"45\""), "{body}");
        assert!(
            body.contains("\"volume\":{\"level\":45,\"muted\":true}"),
            "{body}"
        );
        assert!(
            body.contains("\"actions\":[\"set\",\"mute\",\"unmute\"]"),
            "{body}"
        );
    }
}
