//! MCP server on the official Rust SDK (`rmcp`).
//!
//! Only the server + Streamable HTTP transport features are enabled. The
//! dynamic [`ToolGroup`] registry is exposed through [`ServerHandler`]; input
//! schemas are hand-written JSON, never `schemars`.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::ErrorData as McpError;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeResult, JsonObject, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ProtocolVersion, ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};

use crate::tools::{Annotations, Deps, ToolGroup, ToolSpec};

/// The single MCP resource of `infra.read`.
pub const CATALOG_URI: &str = "infra://catalog";

const INSTRUCTIONS: &str = "Инфраструктурный шлюз домашнего кластера.
Начинай диагностику с infra_status (один агрегированный вызов), затем уточняй
infra_service или infra_logs по имени сервиса из каталога.
Перезапуск выполняется только в два шага: infra_prepare_restart, затем
infra_confirm после подтверждения человеком. Никогда не выдумывай имена:
бери их из infra_overview или ресурса infra://catalog.
Агенты bb: bb_status показывает только личные треды; запуск — только в личные
проекты, в два шага bb_prepare_spawn → bb_confirm, отмена — bb_cancel.
Рабочие проекты и треды недоступны и не обсуждаются.
Отвечай коротко; длинные отчёты отправляй через infra_send_report.";

/// The dynamic MCP server.
#[derive(Clone)]
pub struct GatewayHandler {
    deps: Arc<Deps>,
    version: String,
    tools: BTreeMap<&'static str, (Arc<dyn ToolGroup>, ToolSpec)>,
    resources: bool,
}

impl GatewayHandler {
    /// Build a handler from the enabled groups.
    #[must_use]
    pub fn new(deps: Arc<Deps>, groups: &[Arc<dyn ToolGroup>], version: impl Into<String>) -> Self {
        let mut tools = BTreeMap::new();
        let mut resources = false;
        for group in groups {
            if group.name() == "infra.read" {
                resources = true;
            }
            for spec in group.tools() {
                tools.insert(spec.name, (Arc::clone(group), spec));
            }
        }
        Self {
            deps,
            version: version.into(),
            tools,
            resources,
        }
    }

    fn tool(&self, name: &str) -> Option<Tool> {
        let (_, spec) = self.tools.get(name)?;
        let schema = spec.input_schema.as_object().cloned().unwrap_or_default();
        Some(
            Tool::new(spec.name, spec.description, schema)
                .with_annotations(to_annotations(spec.annotations)),
        )
    }

    fn resource(&self) -> Resource {
        Resource::new(CATALOG_URI, "infra catalog")
            .with_title("Infra catalog")
            .with_description("Sanitized topology overview plus the full service catalog.")
            .with_mime_type("text/markdown")
    }
}

fn to_annotations(annotations: Annotations) -> ToolAnnotations {
    let mut result = ToolAnnotations::new().read_only(annotations.read_only);
    if let Some(destructive) = annotations.destructive {
        result = result.destructive(destructive);
    }
    if let Some(open_world) = annotations.open_world {
        result = result.open_world(open_world);
    }
    result
}

impl ServerHandler for GatewayHandler {
    #[allow(deprecated)]
    fn get_info(&self) -> ServerConfig {
        let capabilities = if self.resources {
            ServerCapabilities::builder()
                .enable_logging()
                .enable_resources()
                .enable_resources_list_changed()
                .enable_tools()
                .enable_tool_list_changed()
                .build()
        } else {
            ServerCapabilities::builder()
                .enable_logging()
                .enable_tools()
                .enable_tool_list_changed()
                .build()
        };
        InitializeResult::new(capabilities)
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_server_info(
                Implementation::new("mcp-gateway", self.version.clone())
                    .with_title("Infra gateway"),
            )
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .tools
            .values()
            .filter_map(|(_, spec)| self.tool(spec.name))
            .collect();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some((group, spec)) = self.tools.get(request.name.as_ref()) else {
            return Err(McpError::invalid_params(
                format!("unknown tool {:?}", request.name),
                None,
            ));
        };
        let arguments = request
            .arguments
            .map_or(serde_json::Value::Null, serde_json::Value::Object);
        let started = std::time::Instant::now();
        let outcome = group.call(spec.name, arguments).await;
        match outcome {
            Ok(outcome) => {
                self.deps
                    .metrics
                    .observe_tool(spec.name, "ok", started.elapsed());
                let envelope = outcome.envelope();
                let structured = outcome.structured();
                let mut result = CallToolResult::success(vec![ContentBlock::text(envelope)]);
                result.structured_content = Some(structured);
                Ok(CallToolResponse::Complete(result))
            }
            Err(error) => {
                self.deps
                    .metrics
                    .observe_tool(spec.name, "error", started.elapsed());
                Err(McpError::internal_error(error, None))
            }
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        if !self.resources {
            return Err(McpError::method_not_found::<
                rmcp::model::ListResourcesRequestMethod,
            >());
        }
        Ok(ListResourcesResult {
            resources: vec![self.resource()],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        if !self.resources || request.uri != CATALOG_URI {
            return Err(McpError::invalid_params(
                format!("unknown resource {:?}", request.uri),
                None,
            ));
        }
        let Some(catalog) = self.deps.catalog.as_ref() else {
            return Err(McpError::internal_error("catalog is not loaded", None));
        };
        let text = format!("{}\n\n{}", catalog.overview_text(), catalog.catalog_text());
        let mut contents = ResourceContents::text(text, request.uri);
        if let ResourceContents::TextResourceContents { mime_type, .. } = &mut contents {
            *mime_type = Some("text/markdown".to_owned());
        }
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

/// Unused marker that keeps the `JsonObject` import meaningful for schema
/// construction in tests.
#[allow(dead_code)]
fn _empty_schema() -> JsonObject {
    JsonObject::new()
}
