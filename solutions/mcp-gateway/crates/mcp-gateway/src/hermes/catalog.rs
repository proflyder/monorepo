use serde::{Deserialize, Serialize};

use super::history::{HistoryError, RelayClient};
use crate::tools::BoxFuture;

pub const SKILLS_PATH: &str = "/v1/skills";
pub const TOOLSETS_PATH: &str = "/v1/toolsets";

pub trait CatalogApi: Send + Sync {
    fn skills(&self) -> BoxFuture<'_, Result<Vec<Skill>, HistoryError>>;

    fn toolsets(&self) -> BoxFuture<'_, Result<Vec<UpstreamToolset>, HistoryError>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpstreamList<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Skill {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpstreamToolset {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolset {
    pub name: String,
    pub label: String,
    pub description: String,
    pub enabled: bool,
    pub configured: bool,
    pub tool_count: usize,
    pub tools: Vec<String>,
}

impl From<UpstreamToolset> for Toolset {
    fn from(upstream: UpstreamToolset) -> Self {
        let label = if upstream.label.trim().is_empty() {
            upstream.name.clone()
        } else {
            upstream.label
        };
        Self {
            tool_count: upstream.tools.len(),
            name: upstream.name,
            label,
            description: upstream.description,
            enabled: upstream.enabled,
            configured: upstream.configured,
            tools: upstream.tools,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skills {
    pub available: bool,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Toolsets {
    pub available: bool,
    pub toolsets: Vec<Toolset>,
}

#[must_use]
pub fn skills(result: Option<Result<Vec<Skill>, HistoryError>>) -> Skills {
    match result {
        Some(Ok(mut skills)) => {
            skills.sort_by(|left, right| {
                left.category
                    .cmp(&right.category)
                    .then_with(|| left.name.cmp(&right.name))
            });
            Skills {
                available: true,
                skills,
            }
        }
        _ => Skills {
            available: false,
            skills: Vec::new(),
        },
    }
}

#[must_use]
pub fn toolsets(result: Option<Result<Vec<UpstreamToolset>, HistoryError>>) -> Toolsets {
    match result {
        Some(Ok(upstream)) => {
            let mut toolsets: Vec<Toolset> = upstream.into_iter().map(Toolset::from).collect();
            toolsets.sort_by_key(|toolset| !toolset.enabled);
            Toolsets {
                available: true,
                toolsets,
            }
        }
        _ => Toolsets {
            available: false,
            toolsets: Vec::new(),
        },
    }
}

impl CatalogApi for RelayClient {
    fn skills(&self) -> BoxFuture<'_, Result<Vec<Skill>, HistoryError>> {
        Box::pin(async move {
            let list: UpstreamList<Skill> = self.get(SKILLS_PATH).await?;
            Ok(list.data)
        })
    }

    fn toolsets(&self) -> BoxFuture<'_, Result<Vec<UpstreamToolset>, HistoryError>> {
        Box::pin(async move {
            let list: UpstreamList<UpstreamToolset> = self.get(TOOLSETS_PATH).await?;
            Ok(list.data)
        })
    }
}
