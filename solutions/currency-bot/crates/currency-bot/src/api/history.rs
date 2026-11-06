//! History, latest, delete and trigger handlers.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use super::ApiState;
use super::error::write_error;
use crate::domain::model::{ExchangeRate, Rates, Record};
use crate::domain::time::{parse_date, parse_rfc3339, rfc3339_nano};
use crate::store::StoreError;

const HISTORY_SERVICE: &str = "CurrencyHistoryController.getHistory";
const LATEST_SERVICE: &str = "CurrencyHistoryController.getLatest";
const DELETE_SERVICE: &str = "CurrencyHistoryController.deleteHistory";
const TRIGGER_SERVICE: &str = "TriggerController.triggerCurrencyUpdate";

#[derive(Debug, Serialize)]
struct ExchangeRateDto {
    #[serde(serialize_with = "super::json::go_f64")]
    buy: f64,
    #[serde(serialize_with = "super::json::go_f64")]
    sell: f64,
}

#[derive(Debug, Serialize)]
struct RatesDto {
    #[serde(rename = "usdToKzt")]
    usd_to_kzt: ExchangeRateDto,
    #[serde(rename = "rubToKzt")]
    rub_to_kzt: ExchangeRateDto,
}

#[derive(Debug, Serialize)]
struct RecordDto {
    timestamp: String,
    rates: RatesDto,
}

#[derive(Debug, Serialize)]
struct HistoryResponse {
    records: Vec<RecordDto>,
    #[serde(rename = "totalCount")]
    total_count: usize,
}

#[derive(Debug, Serialize)]
struct LatestResponse {
    timestamp: String,
    rates: RatesDto,
}

#[derive(Debug, Serialize)]
struct DeleteResponse {
    #[serde(rename = "deletedCount")]
    deleted_count: usize,
    message: String,
}

fn rates_dto(rates: &Rates) -> RatesDto {
    let pair = |rate: &ExchangeRate| ExchangeRateDto {
        buy: rate.buy,
        sell: rate.sell,
    };
    RatesDto {
        usd_to_kzt: pair(&rates.usd_to_kzt),
        rub_to_kzt: pair(&rates.rub_to_kzt),
    }
}

fn record_dto(record: &Record) -> RecordDto {
    RecordDto {
        timestamp: rfc3339_nano(record.timestamp),
        rates: rates_dto(&record.rates),
    }
}

/// `GET /api/history`: date wins, otherwise from+to, otherwise everything.
pub async fn history(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let date = params.get("date").filter(|value| !value.is_empty());
    let from = params.get("from").filter(|value| !value.is_empty());
    let to = params.get("to").filter(|value| !value.is_empty());

    let range = if let Some(date) = date {
        let Some(day) = parse_date(date) else {
            return write_error(
                &state,
                "/api/history",
                StatusCode::BAD_REQUEST,
                "Bad Request",
                "Invalid date: expected YYYY-MM-DD",
                "BAD_REQUEST",
                HISTORY_SERVICE,
                None,
            );
        };
        let start = format!("{day}T00:00:00Z")
            .parse::<jiff::Timestamp>()
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        // Millisecond precision: ending one millisecond before the next
        // midnight keeps 00:00:00.000 in exactly one day.
        let end =
            jiff::Timestamp::from_millisecond(start.as_millisecond().saturating_add(86_399_999))
                .unwrap_or(start);
        Some((start, end))
    } else if let (Some(from), Some(to)) = (from, to) {
        let Some(start) = parse_rfc3339(from) else {
            return write_error(
                &state,
                "/api/history",
                StatusCode::BAD_REQUEST,
                "Bad Request",
                "Invalid from: expected RFC3339",
                "BAD_REQUEST",
                HISTORY_SERVICE,
                None,
            );
        };
        let Some(end) = parse_rfc3339(to) else {
            return write_error(
                &state,
                "/api/history",
                StatusCode::BAD_REQUEST,
                "Bad Request",
                "Invalid to: expected RFC3339",
                "BAD_REQUEST",
                HISTORY_SERVICE,
                None,
            );
        };
        Some((start, end))
    } else if from.is_some() || to.is_some() {
        return write_error(
            &state,
            "/api/history",
            StatusCode::BAD_REQUEST,
            "Bad Request",
            "Invalid range: both from and to are required",
            "BAD_REQUEST",
            HISTORY_SERVICE,
            None,
        );
    } else {
        None
    };

    let records = match range {
        Some((from, to)) => state.store.records_by_date_range(from, to),
        None => state.store.all_records(),
    };
    let records = match records {
        Ok(records) => records,
        Err(error) => {
            return write_error(
                &state,
                "/api/history",
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                format!("Failed to fetch currency history: {error}"),
                "DATABASE_ERROR",
                HISTORY_SERVICE,
                None,
            );
        }
    };
    let total_count = records.len();
    Json(HistoryResponse {
        records: records.iter().map(record_dto).collect(),
        total_count,
    })
    .into_response()
}

/// `GET /api/latest`.
pub async fn latest(State(state): State<Arc<ApiState>>) -> Response {
    match state.store.latest_record() {
        Ok(record) => Json(LatestResponse {
            timestamp: rfc3339_nano(record.timestamp),
            rates: rates_dto(&record.rates),
        })
        .into_response(),
        Err(StoreError::NotFound) => write_error(
            &state,
            "/api/latest",
            StatusCode::NOT_FOUND,
            "Not Found",
            "No currency rates found in database",
            "NOT_FOUND",
            LATEST_SERVICE,
            None,
        ),
        Err(error) => write_error(
            &state,
            "/api/latest",
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
            format!("Failed to fetch latest currency rate: {error}"),
            "DATABASE_ERROR",
            LATEST_SERVICE,
            None,
        ),
    }
}

/// `DELETE /api/history`; `sent_alerts` is deliberately untouched.
pub async fn delete_history(State(state): State<Arc<ApiState>>) -> Response {
    match state.store.delete_all() {
        Ok(deleted) => Json(DeleteResponse {
            deleted_count: deleted,
            message: format!("Successfully deleted {deleted} currency history records"),
        })
        .into_response(),
        Err(error) => write_error(
            &state,
            "/api/history",
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
            format!("Failed to delete currency history: {error}"),
            "DATABASE_ERROR",
            DELETE_SERVICE,
            None,
        ),
    }
}

#[derive(Debug, Deserialize)]
struct TriggerRequest {
    #[serde(rename = "chatId", default)]
    chat_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TriggerResponse {
    message: &'static str,
    #[serde(rename = "executionTimeMs")]
    execution_time_ms: i64,
}

/// `POST /api/trigger`: force a send, do not save history (the Kotlin
/// contract). An unparseable body is silently ignored.
pub async fn trigger(State(state): State<Arc<ApiState>>, body: Bytes) -> Response {
    let request: TriggerRequest = if body.len() > 64 * 1024 {
        TriggerRequest { chat_id: None }
    } else {
        serde_json::from_slice(&body).unwrap_or(TriggerRequest { chat_id: None })
    };
    let chat_id = request
        .chat_id
        .filter(|chat| !chat.is_empty())
        .unwrap_or_else(|| state.default_chat.clone());

    let started = Instant::now();
    let result = state.pipeline.run("trigger", &chat_id, true, false).await;
    let elapsed = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match result {
        Ok(()) => Json(TriggerResponse {
            message: "Currency rates updated and sent to Telegram successfully",
            execution_time_ms: elapsed,
        })
        .into_response(),
        Err(error) => {
            let mut details = BTreeMap::new();
            details.insert("chatId".to_owned(), chat_id.clone());
            details.insert("executionTimeMs".to_owned(), elapsed.to_string());
            write_error(
                &state,
                "/api/trigger",
                StatusCode::BAD_GATEWAY,
                "Bad Gateway",
                format!("Failed to update and send currency rates: {error}"),
                "EXTERNAL_SERVICE_ERROR",
                TRIGGER_SERVICE,
                Some(details),
            )
        }
    }
}
