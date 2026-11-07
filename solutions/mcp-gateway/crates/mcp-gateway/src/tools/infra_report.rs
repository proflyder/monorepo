//! `infra.report`: deliver a long, HTML-escaped Telegram report.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use super::{Annotations, BoxFuture, Deps, ToolGroup, ToolOutcome, ToolSpec, failure, respond};
use crate::domain::redact;

/// The `infra.report` group.
pub struct ReportGroup {
    deps: Arc<Deps>,
}

impl ReportGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for ReportGroup {
    fn name(&self) -> &'static str {
        "infra.report"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "infra_send_report",
            description: "Send a long HTML-escaped report to the preconfigured Telegram chat. The recipient cannot be chosen by the model. Suitable for detailed reports that are too long to speak.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "title": {"type": "string", "description": "short report title"},
                    "text": {"type": "string", "description": "report body, plain text; HTML is escaped"},
                },
                "required": ["title", "text"],
            }),
            annotations: Annotations {
                read_only: false,
                destructive: Some(false),
                open_world: Some(true),
            },
        }]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            match name {
                "infra_send_report" => Ok(send_report(&self.deps, &arguments).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

const MAX_REPORT_TITLE: usize = 200;
const MAX_REPORT_BYTES: usize = 64 << 10;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportDetail {
    title: String,
    bytes: usize,
    status: String,
}

async fn send_report(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let title = arguments
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let text = arguments
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if title.is_empty() || text.is_empty() {
        return failure("Нужны заголовок и текст отчёта.", None::<Value>);
    }
    if title.chars().count() > MAX_REPORT_TITLE {
        return failure("Заголовок отчёта слишком длинный.", None::<Value>);
    }
    if text.len() > MAX_REPORT_BYTES {
        return failure("Отчёт слишком длинный.", Some(json!({"bytes": text.len()})));
    }
    let Some(sender) = deps.report() else {
        return failure("Telegram не настроен.", None::<Value>);
    };
    let text = redact::text(&text);
    if let Err(error) = sender.send(&title, &text).await {
        deps.audit("report", "telegram", "failure");
        return failure("Не удалось отправить отчёт.", Some(json!({"error": error})));
    }
    deps.audit("report", "telegram", "success");
    respond(
        format!("Отправил отчёт «{title}»."),
        Some(ReportDetail {
            title,
            bytes: text.len(),
            status: "sent".to_owned(),
        }),
    )
}
