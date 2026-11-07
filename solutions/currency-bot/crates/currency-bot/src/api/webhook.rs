//! The Telegram webhook listener.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use home_telegram::Update;

use super::{ApiState, MAX_WEBHOOK_BYTES};
use crate::domain::texts;
use crate::pipeline::{MessageSender, Pipeline};

const SECRET_HEADER: &str = "X-Telegram-Bot-Api-Secret-Token";
const TRIGGER_TIMEOUT: Duration = Duration::from_secs(25);

/// Validate the Telegram secret, enforce the chat allowlist, acknowledge first
/// and dispatch `/start`, `/help` and `/trigger` by prefix match.
pub async fn handle_webhook(State(state): State<Arc<ApiState>>, request: Request) -> Response {
    let secret = request
        .headers()
        .get(SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if secret.is_empty() || !state.webhook_secret.ct_eq(secret) {
        tracing::warn!("rejected webhook with invalid secret");
        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            "unauthorized\n",
        )
            .into_response();
    }

    // A valid secret is enough to answer 200. Telegram retries every non-2xx
    // response, which would re-run /trigger, so failures are logged only.
    let body = match axum::body::to_bytes(request.into_body(), MAX_WEBHOOK_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(%error, step = "read webhook body", "telegram webhook failed");
            return ok();
        }
    };
    let update: Update = match serde_json::from_slice(&body) {
        Ok(update) => update,
        Err(error) => {
            tracing::error!(%error, step = "decode webhook update", "telegram webhook failed");
            return ok();
        }
    };

    if !state.mark_update_seen(update.update_id) {
        tracing::info!(
            update_id = update.update_id,
            "ignoring duplicate telegram update"
        );
        return ok();
    }

    let Some(message) = update.message() else {
        return ok();
    };
    if !state.is_allowed_chat(message.chat.id) {
        tracing::info!(
            chat_id = message.chat.id,
            "ignoring message from unknown chat"
        );
        return ok();
    }
    let text = message.text.as_deref().unwrap_or("").trim();
    if text.is_empty() {
        return ok();
    }
    let chat_id = message.chat.id.to_string();

    if text.starts_with("/trigger") {
        // A fetch plus send can take tens of seconds, close to Telegram's
        // timeout. Acknowledge first and run the pipeline in the background;
        // the update_id guard above makes a retried update a no-op.
        let pipeline = Arc::clone(&state.pipeline);
        let sender = Arc::clone(&state.sender);
        let cancel = state.cancel.clone();
        let trigger = async move {
            run_trigger(pipeline, sender, chat_id, cancel).await;
        };
        if state.dispatch_inline {
            let handle = state.tasks.spawn(trigger);
            let _ = handle.await;
        } else {
            state.tasks.spawn(trigger);
        }
    } else if text.starts_with("/start") {
        if let Err(error) = state.sender.send(&chat_id, texts::START_MESSAGE).await {
            tracing::error!(error = %error, step = "handle telegram command", "telegram webhook failed");
        }
    } else if text.starts_with("/help") {
        if let Err(error) = state.sender.send(&chat_id, texts::HELP_MESSAGE).await {
            tracing::error!(error = %error, step = "handle telegram command", "telegram webhook failed");
        }
    } else {
        tracing::debug!(chat_id, "ignoring non-command message");
    }
    ok()
}

async fn run_trigger(
    pipeline: Arc<Pipeline>,
    sender: Arc<dyn MessageSender>,
    chat_id: String,
    cancel: tokio_util::sync::CancellationToken,
) {
    let run = pipeline.run("trigger", &chat_id, true, false);
    let result = tokio::select! {
        () = cancel.cancelled() => return,
        result = tokio::time::timeout(TRIGGER_TIMEOUT, run) => result,
    };
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(chat_id, error = %error, "failed to trigger currency update");
            if let Err(send_error) = sender
                .send(&chat_id, &texts::trigger_failure(&error.to_string()))
                .await
            {
                tracing::error!(error = %send_error, "failed to report trigger failure");
            }
        }
        Err(_elapsed) => {
            tracing::error!(chat_id, "trigger pipeline timed out");
        }
    }
}

fn ok() -> Response {
    // The shared contract runner compares a string body byte for byte, while
    // the Go fixture stores the decoded `"OK"`. Both encodings decode to the
    // same JSON string; Telegram ignores the body.
    ([(header::CONTENT_TYPE, "application/json")], "OK").into_response()
}
