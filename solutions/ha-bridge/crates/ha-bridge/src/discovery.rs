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

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use home_ha::{Entity, Error};

use crate::catalog::{self, Catalog, Light, LightInput, Raw, Rules};

const DOMAINS: &str = "['script','scene','light','switch']";

/// The Home Assistant read surface discovery needs.
pub trait HomeAssistant: Send + Sync {
    fn entities(&self) -> impl Future<Output = Result<Vec<Entity>, Error>> + Send;
    fn render_template(&self, template: &str)
    -> impl Future<Output = Result<String, Error>> + Send;
}

impl HomeAssistant for home_ha::Client {
    async fn entities(&self) -> Result<Vec<Entity>, Error> {
        self.entities().await
    }

    async fn render_template(&self, template: &str) -> Result<String, Error> {
        self.render_template(template).await
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
                Raw {
                    entity: entity.entity_id.clone(),
                    state: entity.state.clone(),
                    name: entity.name().to_owned(),
                    icon: entity.icon().to_owned(),
                    room,
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
