//! Build the control catalog from Home Assistant itself.
//!
//! Entities (name, icon, state) come from `/api/states`; areas, hidden flags
//! and the scenario/profile role labels come from read-only template rendering.
//! Results are cached briefly. If Home Assistant becomes unreachable the last
//! good catalog is served flagged as stale instead of inventing or hardcoding
//! anything.
//!
//! The scenario label decides which scripts are exposed, so an unreadable
//! scenario label fails closed (no scripts). An unreadable profile label only
//! loses the profile role: the entity stays a plain switch, which still works.
//!
//! Device metadata is read best effort: `device_id` and the device name come
//! from one more read-only template, the entity category comes from the
//! WebSocket entity registry (`config/entity_registry/list`, see
//! [`home_ha::Client::entity_registry`]). If either read fails, the fields stay
//! empty, entities are kept, and a warning is logged; a catalog refresh retries
//! on the next TTL expiry. Missing device metadata never removes an entity.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use home_ha::{Entity, Error, RegistryEntity};

use crate::catalog::{self, Catalog, Light, LightInput, Raw, Rules};

const DOMAINS: &str = "['script','scene','light','switch']";

/// The Home Assistant read surface discovery needs.
pub trait HomeAssistant: Send + Sync {
    fn entities(&self) -> impl Future<Output = Result<Vec<Entity>, Error>> + Send;
    fn render_template(&self, template: &str)
    -> impl Future<Output = Result<String, Error>> + Send;

    /// Entity registry records (entity id, device id, category). The default
    /// returns nothing so fakes and older test doubles keep device names and
    /// titles without categories.
    fn entity_registry(&self) -> impl Future<Output = Result<Vec<RegistryEntity>, Error>> + Send {
        async { Ok(Vec::new()) }
    }
}

impl HomeAssistant for home_ha::Client {
    async fn entities(&self) -> Result<Vec<Entity>, Error> {
        self.entities().await
    }

    async fn render_template(&self, template: &str) -> Result<String, Error> {
        self.render_template(template).await
    }

    async fn entity_registry(&self) -> Result<Vec<RegistryEntity>, Error> {
        self.entity_registry().await
    }
}

impl<T: HomeAssistant + ?Sized> HomeAssistant for Arc<T> {
    fn entities(&self) -> impl Future<Output = Result<Vec<Entity>, Error>> + Send {
        (**self).entities()
    }

    fn render_template(
        &self,
        template: &str,
    ) -> impl Future<Output = Result<String, Error>> + Send {
        (**self).render_template(template)
    }

    fn entity_registry(&self) -> impl Future<Output = Result<Vec<RegistryEntity>, Error>> + Send {
        (**self).entity_registry()
    }
}

/// A discovered catalog.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub catalog: Arc<Catalog>,
    /// False when Home Assistant could not be read and this is the last known
    /// catalog; every entry is then reported unavailable.
    pub states_ok: bool,
}

/// Why a catalog is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    #[error("home assistant is not reachable")]
    Unavailable,
}

/// A catalog source. The HTTP server and the control executor share it.
///
/// The boxed future keeps the trait object-safe, so [`crate::catalog_api`] can
/// hold `Arc<dyn CatalogSource>` without a generic parameter leaking into every
/// type.
pub trait CatalogSource: Send + Sync {
    fn get(&self) -> Pin<Box<dyn Future<Output = Result<Snapshot, DiscoveryError>> + Send + '_>>;
}

/// Discovers and caches the catalog.
pub struct Provider<H: HomeAssistant> {
    ha: H,
    rules: Rules,
    ttl: Duration,
    last: Mutex<Option<(Arc<Catalog>, Instant)>>,
}

impl<H: HomeAssistant> std::fmt::Debug for Provider<H> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Provider")
            .field("rules", &self.rules)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl<H: HomeAssistant> Provider<H> {
    /// Build a provider. `ttl` bounds how often Home Assistant is queried.
    #[must_use]
    pub fn new(ha: H, rules: Rules, ttl: Duration) -> Self {
        Self {
            ha,
            rules,
            ttl,
            last: Mutex::new(None),
        }
    }

    async fn discover(&self) -> Result<Vec<catalog::Entry>, DiscoveryError> {
        let entities = self
            .ha
            .entities()
            .await
            .map_err(|_error| DiscoveryError::Unavailable)?;

        // Areas and hidden flags are nice-to-have. Labels are best effort too,
        // with the fail-closed rule for scripts.
        let rooms = self.rooms().await.unwrap_or_else(|| {
            tracing::warn!("could not read areas; rooms will be empty");
            Vec::new()
        });
        let hidden = self.hidden().await.unwrap_or_else(|| {
            tracing::warn!("could not read hidden flags; hidden entities will not be filtered");
            Vec::new()
        });
        let devices = self.devices().await.unwrap_or_else(|| {
            tracing::warn!("could not read device metadata; device ids and names stay empty");
            Vec::new()
        });
        let registry = match self.ha.entity_registry().await {
            Ok(entries) => Some(entries),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "could not read the entity registry; entity roles stay unknown"
                );
                None
            }
        };
        let scenarios = self
            .label_entities(&self.rules.scenario_label)
            .await
            .unwrap_or_else(|| {
                tracing::warn!(
                    label = %self.rules.scenario_label,
                    "could not read the scenario label; no scripts will be exposed"
                );
                Vec::new()
            });
        let profiles = self
            .label_entities(&self.rules.profile_label)
            .await
            .unwrap_or_else(|| {
                tracing::warn!(
                    label = %self.rules.profile_label,
                    "could not read the profile label; the profile switch stays a normal device"
                );
                Vec::new()
            });

        let raws = entities
            .iter()
            .map(|entity| {
                let room = rooms
                    .iter()
                    .find(|(id, _)| id == &entity.entity_id)
                    .map(|(_, room)| room.clone())
                    .unwrap_or_default();
                let device = devices
                    .iter()
                    .find(|device| device.entity == entity.entity_id);
                let registry_entry = registry.as_ref().and_then(|entries| {
                    entries
                        .iter()
                        .find(|entry| entry.entity_id == entity.entity_id)
                });
                Raw {
                    entity: entity.entity_id.clone(),
                    state: entity.state.clone(),
                    name: entity.name().to_owned(),
                    icon: entity.icon().to_owned(),
                    room,
                    device_id: device.map(|device| device.id.clone()).unwrap_or_default(),
                    device: device.map(|device| device.name.clone()).unwrap_or_default(),
                    role: registry_entry
                        .map(|entry| role_from_category(entry).to_owned())
                        .unwrap_or_default(),
                    hidden: hidden.iter().any(|id| id == &entity.entity_id),
                    scenario: scenarios.iter().any(|id| id == &entity.entity_id),
                    profile: profiles.iter().any(|id| id == &entity.entity_id),
                    light: light_of(entity),
                }
            })
            .collect::<Vec<_>>();
        Ok(catalog::build(&raws, &self.rules))
    }

    async fn rooms(&self) -> Option<Vec<(String, String)>> {
        let text = self
            .ha
            .render_template(&format!(
                "{{% for s in states if s.domain in {DOMAINS} %}}{{{{ s.entity_id }}}}|{{{{ area_name(s.entity_id) or '' }}}}\n{{% endfor %}}"
            ))
            .await
            .ok()?;
        Some(
            text.lines()
                .filter_map(|line| {
                    let (id, room) = line.trim().split_once('|')?;
                    let room = room.trim();
                    if room.is_empty() || room == "None" {
                        return None;
                    }
                    Some((id.to_owned(), room.to_owned()))
                })
                .collect(),
        )
    }

    /// Device id and device name of every exposed entity.
    ///
    /// The entity registry has no REST endpoint, so the device link goes
    /// through a template; the category itself comes from the WebSocket
    /// registry. A template error yields `None` and discovery keeps the
    /// entities without a device instead of failing.
    async fn devices(&self) -> Option<Vec<DeviceRow>> {
        let text = self
            .ha
            .render_template(&format!(
                "{{% for s in states if s.domain in {DOMAINS} %}}\
                 {{% set d = device_id(s.entity_id) %}}\
                 {{{{ s.entity_id }}}}|{{{{ d or '' }}}}|\
                 {{{{ ((device_attr(d, 'name_by_user') or device_attr(d, 'name')) if d else '') or '' }}}}\n\
                 {{% endfor %}}"
            ))
            .await
            .ok()?;
        Some(
            text.lines()
                .filter_map(|line| {
                    // The device name is last and may itself contain '|'.
                    let mut fields = line.trim().splitn(3, '|');
                    let entity = fields.next()?.trim();
                    if entity.is_empty() {
                        return None;
                    }
                    Some(DeviceRow {
                        entity: entity.to_owned(),
                        id: fields.next().unwrap_or_default().trim().to_owned(),
                        name: fields.next().unwrap_or_default().trim().to_owned(),
                    })
                })
                .collect(),
        )
    }

    async fn hidden(&self) -> Option<Vec<String>> {
        let text = self
            .ha
            .render_template(&format!(
                "{{% for s in states if s.domain in {DOMAINS} and is_hidden_entity(s.entity_id) %}}{{{{ s.entity_id }}}}\n{{% endfor %}}"
            ))
            .await
            .ok()?;
        Some(lines(&text))
    }

    async fn label_entities(&self, label: &str) -> Option<Vec<String>> {
        // Rules::validate guarantees the label is [a-z0-9_-], so inlining it is
        // safe.
        let text = self
            .ha
            .render_template(&format!(
                "{{{{ label_entities('{label}') | join('\\n') }}}}"
            ))
            .await
            .ok()?;
        Some(lines(&text))
    }
}

impl<H: HomeAssistant> CatalogSource for Provider<H> {
    fn get(&self) -> Pin<Box<dyn Future<Output = Result<Snapshot, DiscoveryError>> + Send + '_>> {
        Box::pin(async move { self.get_cached().await })
    }
}

impl<H: HomeAssistant> Provider<H> {
    async fn get_cached(&self) -> Result<Snapshot, DiscoveryError> {
        // Never hold the std mutex across the discovery await: the future must
        // stay Send for the worker and the axum handler.
        let previous = {
            let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((catalog, expires)) = last.as_ref()
                && Instant::now() < *expires
            {
                return Ok(Snapshot {
                    catalog: Arc::clone(catalog),
                    states_ok: true,
                });
            }
            last.as_ref().map(|(catalog, _)| Arc::clone(catalog))
        };
        match self.discover().await {
            Ok(entries) => {
                let catalog = Arc::new(Catalog::new(entries));
                *self.last.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some((Arc::clone(&catalog), Instant::now() + self.ttl));
                Ok(Snapshot {
                    catalog,
                    states_ok: true,
                })
            }
            Err(_error) => {
                let Some(catalog) = previous else {
                    return Err(DiscoveryError::Unavailable);
                };
                tracing::warn!("home assistant discovery failed; serving the last known catalog");
                Ok(Snapshot {
                    catalog: Arc::new(catalog.mark_unavailable()),
                    states_ok: false,
                })
            }
        }
    }
}

/// One row of the device-metadata template.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceRow {
    entity: String,
    id: String,
    name: String,
}

/// Map a registry record to the catalog role.
///
/// `config` and `diagnostic` entities are device settings (the station's
/// `Clock`/`Wake`/`Microphone` switches); a `null` category is a primary
/// control. An entity that is not in the registry at all gets an empty role
/// instead, so the catalog never guesses from missing data.
fn role_from_category(entry: &RegistryEntity) -> &'static str {
    match entry.entity_category.as_deref() {
        Some(category)
            if category.trim().eq_ignore_ascii_case("config")
                || category.trim().eq_ignore_ascii_case("diagnostic") =>
        {
            "setting"
        }
        _ => "primary",
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn light_of(entity: &Entity) -> Option<Light> {
    if catalog::domain_of(&entity.entity_id) != "light" {
        return None;
    }
    let attributes = &entity.attributes;
    let (hue, saturation, has_hs) = match attributes.hs_color.as_deref() {
        Some([hue, saturation]) => (*hue, *saturation, true),
        _ => (0.0, 0.0, false),
    };
    Light::new(LightInput {
        color_modes: attributes.supported_color_modes.clone(),
        min_kelvin: int_of(attributes.min_color_temp_kelvin),
        max_kelvin: int_of(attributes.max_color_temp_kelvin),
        brightness: attributes
            .brightness
            .map_or(-1, |value| int_of(Some(value))),
        kelvin: int_of(attributes.color_temp_kelvin),
        hue,
        saturation,
        has_hs,
        color_mode: attributes.color_mode.clone().unwrap_or_default(),
    })
}

/// `int()` of the Go client: absent is 0, values truncate toward zero.
fn int_of(value: Option<f64>) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    value.map_or(0, |value| value as i32)
}

#[cfg(test)]
mod tests {
    use home_ha::{Entity, EntityAttributes};

    use super::*;

    fn entity(id: &str, friendly_name: &str) -> Entity {
        Entity {
            entity_id: id.to_owned(),
            state: "on".to_owned(),
            attributes: EntityAttributes {
                friendly_name: Some(friendly_name.to_owned()),
                ..EntityAttributes::default()
            },
        }
    }

    fn rules() -> Rules {
        Rules {
            scenario_label: "scenario".to_owned(),
            profile_label: "voice_profile".to_owned(),
            exclude: Vec::new(),
        }
    }

    struct Fake {
        devices: Result<&'static str, ()>,
        registry: Result<Vec<RegistryEntity>, ()>,
    }

    impl HomeAssistant for Fake {
        async fn entities(&self) -> Result<Vec<Entity>, Error> {
            Ok(vec![
                entity("light.lamp", "Светильник  Light"),
                entity("switch.delay", "Светильник * yl-light delay-off"),
                entity("switch.mute", "Яндекс Станция Миди Mute microphones"),
            ])
        }

        async fn render_template(&self, template: &str) -> Result<String, Error> {
            if template.contains("device_attr") {
                return self.devices.map(str::to_owned).map_err(|()| Error::Status {
                    op: "render template",
                    status: 500,
                    message: String::new(),
                });
            }
            Ok(String::new())
        }

        async fn entity_registry(&self) -> Result<Vec<RegistryEntity>, Error> {
            self.registry.clone().map_err(|()| Error::Status {
                op: "read entity registry",
                status: 500,
                message: String::new(),
            })
        }
    }

    fn registry(rows: &[(&str, &str, Option<&str>)]) -> Vec<RegistryEntity> {
        rows.iter()
            .map(|(entity_id, device_id, category)| RegistryEntity {
                entity_id: (*entity_id).to_owned(),
                device_id: Some((*device_id).to_owned()),
                entity_category: category.map(str::to_owned),
            })
            .collect()
    }

    const DEVICE_ROWS: &str = "light.lamp|dev-1|Светильник\n\
                               switch.delay|dev-1|Светильник\n\
                               switch.mute|dev-2|Яндекс Станция Миди\n";

    async fn entries(
        devices: Result<&'static str, ()>,
        registry: Result<Vec<RegistryEntity>, ()>,
    ) -> Vec<catalog::Entry> {
        let provider = Provider::new(Fake { devices, registry }, rules(), Duration::from_secs(0));
        provider
            .get()
            .await
            .expect("snapshot")
            .catalog
            .entries()
            .to_vec()
    }

    #[tokio::test]
    async fn device_metadata_comes_from_the_template_and_roles_from_the_registry() {
        let entries = entries(
            Ok(DEVICE_ROWS),
            Ok(registry(&[
                ("light.lamp", "dev-1", None),
                ("switch.delay", "dev-1", Some("config")),
                ("switch.mute", "dev-2", None),
            ])),
        )
        .await;
        let lamp = entries
            .iter()
            .find(|entry| entry.id == "light.lamp")
            .expect("lamp");
        assert_eq!(lamp.title, "Light");
        assert_eq!(lamp.device_id, "dev-1");
        assert_eq!(lamp.device, "Светильник");
        assert_eq!(lamp.role, "primary");

        let delay = entries
            .iter()
            .find(|entry| entry.id == "switch.delay")
            .expect("delay-off");
        assert_eq!(delay.title, "yl-light delay-off");
        assert_eq!(delay.role, "setting");

        // A null category on a device without a light stays primary.
        let mute = entries
            .iter()
            .find(|entry| entry.id == "switch.mute")
            .expect("mute");
        assert_eq!(mute.role, "primary");
    }

    #[tokio::test]
    async fn a_null_category_switch_next_to_a_light_is_a_setting() {
        let entries = entries(
            Ok(DEVICE_ROWS),
            Ok(registry(&[
                ("light.lamp", "dev-1", None),
                ("switch.delay", "dev-1", None),
            ])),
        )
        .await;
        let delay = entries
            .iter()
            .find(|entry| entry.id == "switch.delay")
            .expect("delay-off");
        assert_eq!(delay.role, "setting");
    }

    #[tokio::test]
    async fn a_failed_device_template_only_leaves_the_fields_empty() {
        let entries = entries(
            Err(()),
            Ok(registry(&[
                ("light.lamp", "dev-1", None),
                ("switch.delay", "dev-1", Some("config")),
                ("switch.mute", "dev-2", None),
            ])),
        )
        .await;
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            assert!(entry.device_id.is_empty());
            assert!(entry.device.is_empty());
        }
        // Roles do not depend on the device template.
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.id == "switch.delay")
                .expect("delay")
                .role,
            "setting"
        );
    }

    #[tokio::test]
    async fn a_registry_outage_leaves_roles_unknown_and_keeps_entities() {
        let entries = entries(Ok(DEVICE_ROWS), Err(())).await;
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            assert!(entry.role.is_empty(), "{} got a role", entry.id);
        }
        // Titles still use the device template.
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.id == "switch.delay")
                .expect("delay")
                .title,
            "yl-light delay-off"
        );
    }

    #[test]
    fn categories_map_to_roles() {
        let role = |category: Option<&str>| {
            role_from_category(&RegistryEntity {
                entity_id: "switch.x".to_owned(),
                device_id: None,
                entity_category: category.map(str::to_owned),
            })
        };
        assert_eq!(role(Some("config")), "setting");
        assert_eq!(role(Some("diagnostic")), "setting");
        assert_eq!(role(None), "primary");
        assert_eq!(role(Some("something")), "primary");
    }
}
