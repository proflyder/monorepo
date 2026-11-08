//! In-memory call registry and state machine.
//!
//! The service is deliberately ephemeral: a call exists only while both peers
//! are reachable through their Watch streams. There is no store and no
//! replay after a restart. Every transition happens under one mutex and is
//! followed by a broadcast to the affected endpoints' Watch subscribers.
//!
//! Signaling details: SDP and ICE payloads are opaque to the state machine.
//! The only SDP facts used are the direction (offer from the caller, answer
//! from the callee) and the transition to ACTIVE once both halves were seen.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::broadcast;

use crate::config::{EndpointConfig, IceServerConfig};

/// Capacity of one endpoint's Watch broadcast. ICE trickle can be bursty, but
/// a slow client is a broken client: overflow surfaces as a stream error.
const EVENT_CAPACITY: usize = 256;

/// Wall-clock source; tests use a manual clock.
pub trait Clock: Send + Sync {
    fn now_unix(&self) -> i64;
}

/// The process clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |value| {
                i64::try_from(value.as_secs()).unwrap_or(i64::MAX)
            })
    }
}

/// Call time limits and retention.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub ring: Duration,
    pub connect: Duration,
    pub max_call: Duration,
    pub ended_retention: Duration,
}

impl Timeouts {
    fn seconds(value: Duration) -> i64 {
        i64::try_from(value.as_secs()).unwrap_or(i64::MAX).max(1)
    }
}

/// The role of an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    Phone,
    Station,
}

/// A registered endpoint snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub id: String,
    pub kind: EndpointKind,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub online: bool,
    pub last_seen_unix: i64,
}

/// A call snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: String,
    pub caller: Endpoint,
    pub callee: Endpoint,
    pub state: CallState,
    pub end_reason: EndReason,
    pub created_unix: i64,
    pub updated_unix: i64,
    pub answered_unix: i64,
    pub ended_unix: i64,
}

/// Live state of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    Ringing,
    Connecting,
    Active,
    Ended,
}

impl CallState {
    pub fn is_live(self) -> bool {
        !matches!(self, Self::Ended)
    }
}

/// Why a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Unspecified,
    Hangup,
    Declined,
    Busy,
    Canceled,
    Timeout,
    Unreachable,
    Error,
}

/// Direction of an SDP blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdpType {
    Offer,
    Answer,
}

/// One SDP signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdpSignal {
    pub kind: SdpType,
    pub sdp: String,
}

/// One trickled ICE candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: String,
    pub sdp_mline_index: i32,
}

/// A signaling payload sent by one endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Sdp(SdpSignal),
    IceCandidate(IceCandidate),
    IceEnd,
}

/// A Watch event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Call {
        call: Call,
        previous_state: Option<CallState>,
    },
    Sdp {
        call_id: String,
        from_endpoint_id: String,
        signal: SdpSignal,
    },
    IceCandidate {
        call_id: String,
        from_endpoint_id: String,
        candidate: IceCandidate,
    },
    IceEnd {
        call_id: String,
        from_endpoint_id: String,
    },
}

/// Error classes mapped to Connect codes at the API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidArgument,
    NotFound,
    AlreadyExists,
    FailedPrecondition,
    PermissionDenied,
    Unavailable,
}

/// A rejected operation. Messages never contain tokens or SDP bodies.
#[derive(Debug, Clone, Copy)]
pub struct DomainError {
    pub kind: ErrorKind,
    pub message: &'static str,
}

impl DomainError {
    fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self { kind, message }
    }
}

/// Server-computed status for GetStatus and metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct Status {
    pub endpoints_registered: usize,
    pub endpoints_online: usize,
    pub active_calls: usize,
}

/// An authenticated endpoint identity derived from its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub endpoint_id: String,
    pub kind: EndpointKind,
}

struct EndpointState {
    info: Endpoint,
    online: u32,
    events: broadcast::Sender<Event>,
}

struct CallRecord {
    id: String,
    caller_id: String,
    callee_id: String,
    state: CallState,
    end_reason: EndReason,
    created: i64,
    updated: i64,
    answered: i64,
    ended: i64,
    offer_seen: bool,
    answer_seen: bool,
    ring_deadline: i64,
    connect_deadline: i64,
    active_deadline: i64,
    idempotency_key: Option<String>,
}

struct Inner {
    endpoints: BTreeMap<String, EndpointState>,
    calls: BTreeMap<String, CallRecord>,
    idempotency: BTreeMap<String, String>,
}

/// Counters rendered at `/metrics`; gauges are computed from live state.
#[derive(Debug, Default)]
pub struct Metrics {
    started: AtomicU64,
    ended: AtomicU64,
    signals: AtomicU64,
    watch_opens: AtomicU64,
    rejected: AtomicU64,
}

impl Metrics {
    fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// The shared call registry.
pub struct Intercom {
    inner: Mutex<Inner>,
    timeouts: Timeouts,
    ice_servers: Vec<IceServerConfig>,
    clock: Arc<dyn Clock>,
    metrics: Metrics,
}

impl Intercom {
    /// Creates the registry with exactly the two configured identities.
    pub fn new(
        phone: &EndpointConfig,
        station: &EndpointConfig,
        timeouts: Timeouts,
        ice_servers: Vec<IceServerConfig>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let mut endpoints = BTreeMap::new();
        endpoints.insert(
            phone.id.clone(),
            EndpointState {
                info: Endpoint {
                    id: phone.id.clone(),
                    kind: EndpointKind::Phone,
                    display_name: phone.display_name.clone(),
                    capabilities: phone.capabilities.clone(),
                    online: false,
                    last_seen_unix: 0,
                },
                online: 0,
                events: broadcast::channel(EVENT_CAPACITY).0,
            },
        );
        endpoints.insert(
            station.id.clone(),
            EndpointState {
                info: Endpoint {
                    id: station.id.clone(),
                    kind: EndpointKind::Station,
                    display_name: station.display_name.clone(),
                    capabilities: station.capabilities.clone(),
                    online: false,
                    last_seen_unix: 0,
                },
                online: 0,
                events: broadcast::channel(EVENT_CAPACITY).0,
            },
        );
        Self {
            inner: Mutex::new(Inner {
                endpoints,
                calls: BTreeMap::new(),
                idempotency: BTreeMap::new(),
            }),
            timeouts,
            ice_servers,
            clock,
            metrics: Metrics::default(),
        }
    }

    /// Registered ICE servers (empty means host candidates only).
    pub fn ice_servers(&self) -> &[IceServerConfig] {
        &self.ice_servers
    }

    /// Current time of the injected clock.
    pub fn now_unix(&self) -> i64 {
        self.clock.now_unix()
    }

    /// Ring timeout reported by GetStatus.
    pub fn ring_timeout_seconds(&self) -> i32 {
        i32::try_from(self.timeouts.ring.as_secs()).unwrap_or(i32::MAX)
    }

    /// Connect timeout reported by GetStatus.
    pub fn connect_timeout_seconds(&self) -> i32 {
        i32::try_from(self.timeouts.connect.as_secs()).unwrap_or(i32::MAX)
    }

    /// Increments the rejected-transition counter when an RPC fails.
    pub fn note_rejected(&self) {
        Metrics::inc(&self.metrics.rejected);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Updates the descriptive fields of an authenticated endpoint.
    pub fn register(
        &self,
        identity: &Identity,
        display_name: &str,
        capabilities: &[String],
    ) -> Result<Endpoint, DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        let state = inner
            .endpoints
            .get_mut(&identity.endpoint_id)
            .ok_or_else(|| DomainError::new(ErrorKind::PermissionDenied, "unknown endpoint"))?;
        if state.info.kind != identity.kind {
            return Err(DomainError::new(
                ErrorKind::PermissionDenied,
                "endpoint kind does not match the token",
            ));
        }
        if !display_name.trim().is_empty() {
            display_name.trim().clone_into(&mut state.info.display_name);
        }
        if !capabilities.is_empty() {
            state.info.capabilities = capabilities.to_vec();
        }
        state.info.last_seen_unix = now;
        let endpoint = state.info.clone();
        drop(inner);
        Ok(endpoint)
    }

    /// All known endpoints in stable id order.
    pub fn list_endpoints(&self) -> Vec<Endpoint> {
        self.lock()
            .endpoints
            .values()
            .map(|state| state.info.clone())
            .collect()
    }

    /// Starts a call. `actor` is the authenticated caller.
    pub fn start_call(
        &self,
        actor: &Identity,
        callee_endpoint_id: &str,
        idempotency_key: &str,
    ) -> Result<Call, DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        touch(&mut inner, &actor.endpoint_id, now);

        let callee_id = callee_endpoint_id.trim();
        if callee_id.is_empty() {
            return Err(DomainError::new(
                ErrorKind::InvalidArgument,
                "callee_endpoint_id is required",
            ));
        }
        if callee_id == actor.endpoint_id {
            return Err(DomainError::new(
                ErrorKind::InvalidArgument,
                "caller and callee are the same endpoint",
            ));
        }
        let key = idempotency_key.trim();
        if !key.is_empty()
            && let Some(existing_id) = inner.idempotency.get(key).cloned()
        {
            let existing = inner.calls.get(&existing_id).ok_or_else(|| {
                DomainError::new(ErrorKind::AlreadyExists, "idempotency conflict")
            })?;
            if existing.callee_id != callee_id || existing.caller_id != actor.endpoint_id {
                return Err(DomainError::new(
                    ErrorKind::AlreadyExists,
                    "idempotency key belongs to another call",
                ));
            }
            return Ok(call_view(existing, &inner.endpoints));
        }

        let callee = inner
            .endpoints
            .get(callee_id)
            .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown callee"))?;
        if !callee.info.online {
            return Err(DomainError::new(
                ErrorKind::Unavailable,
                "callee is offline",
            ));
        }
        if live_call_for(&inner, &actor.endpoint_id).is_some()
            || live_call_for(&inner, callee_id).is_some()
        {
            return Err(DomainError::new(ErrorKind::FailedPrecondition, "busy"));
        }

        let record = CallRecord {
            id: uuid::Uuid::new_v4().to_string(),
            caller_id: actor.endpoint_id.clone(),
            callee_id: callee_id.to_owned(),
            state: CallState::Ringing,
            end_reason: EndReason::Unspecified,
            created: now,
            updated: now,
            answered: 0,
            ended: 0,
            offer_seen: false,
            answer_seen: false,
            ring_deadline: now.saturating_add(Timeouts::seconds(self.timeouts.ring)),
            connect_deadline: 0,
            active_deadline: 0,
            idempotency_key: (!key.is_empty()).then(|| key.to_owned()),
        };
        let id = record.id.clone();
        inner.calls.insert(id.clone(), record);
        if !key.is_empty() {
            inner.idempotency.insert(key.to_owned(), id.clone());
        }
        Metrics::inc(&self.metrics.started);
        let snapshot = call_view(
            inner.calls.get(&id).expect("just inserted"),
            &inner.endpoints,
        );
        broadcast_call(&inner, &snapshot, None);
        drop(inner);
        Ok(snapshot)
    }

    /// Answers a ringing call. Only the callee may answer.
    pub fn answer(&self, actor: &Identity, call_id: &str) -> Result<Call, DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        let previous = {
            let record = live_call_mut(&mut inner, call_id)?;
            if record.callee_id != actor.endpoint_id {
                return Err(DomainError::new(
                    ErrorKind::PermissionDenied,
                    "only the callee can answer",
                ));
            }
            if record.state != CallState::Ringing {
                return Err(DomainError::new(
                    ErrorKind::FailedPrecondition,
                    "call is not ringing",
                ));
            }
            let previous = record.state;
            record.state = CallState::Connecting;
            record.answered = now;
            record.updated = now;
            record.connect_deadline = now.saturating_add(Timeouts::seconds(self.timeouts.connect));
            previous
        };
        touch(&mut inner, &actor.endpoint_id, now);
        let snapshot = call_view(inner.calls.get(call_id).expect("checked"), &inner.endpoints);
        broadcast_call(&inner, &snapshot, Some(previous));
        drop(inner);
        Ok(snapshot)
    }

    /// Hangs up or declines the call from either participant.
    pub fn hangup(
        &self,
        actor: &Identity,
        call_id: &str,
        reason: EndReason,
    ) -> Result<Call, DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        let record = inner
            .calls
            .get(call_id)
            .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown call"))?;
        if !is_participant(record, &actor.endpoint_id) {
            return Err(DomainError::new(
                ErrorKind::PermissionDenied,
                "not a participant of this call",
            ));
        }
        if !record.state.is_live() {
            return Ok(call_view(record, &inner.endpoints));
        }
        let inferred = if record.state == CallState::Ringing {
            if record.caller_id == actor.endpoint_id {
                EndReason::Canceled
            } else {
                EndReason::Declined
            }
        } else {
            EndReason::Hangup
        };
        let end_reason = if reason == EndReason::Unspecified {
            inferred
        } else {
            reason
        };
        let previous = end_call(&mut inner, call_id, end_reason, now);
        touch(&mut inner, &actor.endpoint_id, now);
        let snapshot = call_view(inner.calls.get(call_id).expect("checked"), &inner.endpoints);
        broadcast_call(&inner, &snapshot, Some(previous));
        Metrics::inc(&self.metrics.ended);
        drop(inner);
        Ok(snapshot)
    }

    /// Returns a call visible to one of its participants.
    pub fn get_call(&self, actor: &Identity, call_id: &str) -> Result<Call, DomainError> {
        let inner = self.lock();
        let record = inner
            .calls
            .get(call_id)
            .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown call"))?;
        if !is_participant(record, &actor.endpoint_id) {
            return Err(DomainError::new(
                ErrorKind::PermissionDenied,
                "not a participant of this call",
            ));
        }
        Ok(call_view(record, &inner.endpoints))
    }

    /// Routes one signaling payload to the other participant.
    ///
    /// Returns whether the peer currently has at least one Watch subscriber.
    pub fn send_signal(
        &self,
        actor: &Identity,
        call_id: &str,
        signal: &Signal,
    ) -> Result<bool, DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        let previous_state = {
            let record = inner
                .calls
                .get(call_id)
                .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown call"))?;
            if !is_participant(record, &actor.endpoint_id) {
                return Err(DomainError::new(
                    ErrorKind::PermissionDenied,
                    "not a participant of this call",
                ));
            }
            if !record.state.is_live() {
                return Err(DomainError::new(
                    ErrorKind::FailedPrecondition,
                    "call has ended",
                ));
            }
            record.state
        };

        let peer_id = {
            let record = inner.calls.get(call_id).expect("checked");
            if record.caller_id == actor.endpoint_id {
                record.callee_id.clone()
            } else {
                record.caller_id.clone()
            }
        };

        if let Signal::IceCandidate(candidate) = signal
            && (candidate.candidate.len() > 8 * 1024 || candidate.sdp_mid.len() > 256)
        {
            return Err(DomainError::new(
                ErrorKind::InvalidArgument,
                "ICE candidate is too large",
            ));
        }
        let became_active = match signal {
            Signal::Sdp(sdp) => {
                validate_sdp(&inner, call_id, actor, sdp)?;
                let record = inner.calls.get_mut(call_id).expect("checked");
                match sdp.kind {
                    SdpType::Offer => record.offer_seen = true,
                    SdpType::Answer => record.answer_seen = true,
                }
                record.updated = now;
                if record.state == CallState::Connecting && record.offer_seen && record.answer_seen
                {
                    record.state = CallState::Active;
                    record.active_deadline =
                        now.saturating_add(Timeouts::seconds(self.timeouts.max_call));
                    true
                } else {
                    false
                }
            }
            Signal::IceCandidate(_) | Signal::IceEnd => false,
        };

        let delivered = deliver(&inner, &peer_id, &actor.endpoint_id, call_id, signal);
        Metrics::inc(&self.metrics.signals);
        touch(&mut inner, &actor.endpoint_id, now);

        if became_active {
            let snapshot = call_view(inner.calls.get(call_id).expect("checked"), &inner.endpoints);
            broadcast_call(&inner, &snapshot, Some(previous_state));
        }
        drop(inner);
        Ok(delivered)
    }

    /// Opens a Watch stream and marks the endpoint online.
    ///
    /// The returned guard marks the endpoint offline when the stream is
    /// dropped; losing the last stream during a live call ends it as
    /// UNREACHABLE, because the service has no out-of-band way to reach the
    /// endpoint.
    pub fn watch(
        self: &Arc<Self>,
        identity: &Identity,
    ) -> Result<(broadcast::Receiver<Event>, WatchGuard), DomainError> {
        let now = self.now_unix();
        let mut inner = self.lock();
        let state = inner
            .endpoints
            .get_mut(&identity.endpoint_id)
            .ok_or_else(|| DomainError::new(ErrorKind::PermissionDenied, "unknown endpoint"))?;
        if state.info.kind != identity.kind {
            return Err(DomainError::new(
                ErrorKind::PermissionDenied,
                "endpoint kind does not match the token",
            ));
        }
        let receiver = state.events.subscribe();
        state.online = state.online.saturating_add(1);
        state.info.online = true;
        state.info.last_seen_unix = now;
        Metrics::inc(&self.metrics.watch_opens);
        drop(inner);
        Ok((
            receiver,
            WatchGuard {
                intercom: Arc::clone(self),
                endpoint_id: identity.endpoint_id.clone(),
            },
        ))
    }

    fn mark_offline(&self, endpoint_id: &str) {
        let now = self.now_unix();
        let mut inner = self.lock();
        let Some(state) = inner.endpoints.get_mut(endpoint_id) else {
            return;
        };
        state.online = state.online.saturating_sub(1);
        state.info.online = state.online > 0;
        state.info.last_seen_unix = now;
        if state.online > 0 {
            return;
        }
        let Some(call_id) = live_call_for(&inner, endpoint_id) else {
            return;
        };
        let previous = end_call(&mut inner, &call_id, EndReason::Unreachable, now);
        Metrics::inc(&self.metrics.ended);
        let snapshot = call_view(
            inner.calls.get(&call_id).expect("checked"),
            &inner.endpoints,
        );
        broadcast_call(&inner, &snapshot, Some(previous));
        drop(inner);
    }

    /// Expires ring/connect/max-call deadlines and purges old ended calls.
    pub fn tick(&self) {
        let now = self.now_unix();
        let mut inner = self.lock();
        let expired: Vec<String> = inner
            .calls
            .iter()
            .filter_map(|(id, record)| {
                let deadline = match record.state {
                    CallState::Ringing => record.ring_deadline,
                    CallState::Connecting => record.connect_deadline,
                    CallState::Active => record.active_deadline,
                    CallState::Ended => 0,
                };
                (deadline != 0 && deadline <= now).then(|| id.clone())
            })
            .collect();
        for id in expired {
            let previous = end_call(&mut inner, &id, EndReason::Timeout, now);
            Metrics::inc(&self.metrics.ended);
            let snapshot = call_view(inner.calls.get(&id).expect("checked"), &inner.endpoints);
            broadcast_call(&inner, &snapshot, Some(previous));
        }

        let retain = Timeouts::seconds(self.timeouts.ended_retention);
        let old: Vec<String> = inner
            .calls
            .iter()
            .filter(|(_, record)| {
                record.state == CallState::Ended && record.ended.saturating_add(retain) <= now
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in old {
            if let Some(record) = inner.calls.remove(&id)
                && let Some(key) = record.idempotency_key
            {
                inner.idempotency.remove(&key);
            }
        }
        drop(inner);
    }

    /// Server-computed status.
    pub fn status(&self) -> Status {
        let inner = self.lock();
        Status {
            endpoints_registered: inner.endpoints.len(),
            endpoints_online: inner
                .endpoints
                .values()
                .filter(|state| state.info.online)
                .count(),
            active_calls: inner
                .calls
                .values()
                .filter(|record| record.state.is_live())
                .count(),
        }
    }

    /// Prometheus text exposition. Gauges are read from live state.
    pub fn render_metrics(&self) -> String {
        let status = self.status();
        let mut out = String::new();
        let _ = writeln!(out, "# HELP intercom_calls_started_total Calls created.");
        let _ = writeln!(out, "# TYPE intercom_calls_started_total counter");
        let _ = writeln!(
            out,
            "intercom_calls_started_total {}",
            self.metrics.started.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# HELP intercom_calls_ended_total Calls that reached ENDED."
        );
        let _ = writeln!(out, "# TYPE intercom_calls_ended_total counter");
        let _ = writeln!(
            out,
            "intercom_calls_ended_total {}",
            self.metrics.ended.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# HELP intercom_signals_total SDP/ICE signals relayed."
        );
        let _ = writeln!(out, "# TYPE intercom_signals_total counter");
        let _ = writeln!(
            out,
            "intercom_signals_total {}",
            self.metrics.signals.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# HELP intercom_watch_opens_total Watch streams opened."
        );
        let _ = writeln!(out, "# TYPE intercom_watch_opens_total counter");
        let _ = writeln!(
            out,
            "intercom_watch_opens_total {}",
            self.metrics.watch_opens.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# HELP intercom_rejected_total Rejected state transitions."
        );
        let _ = writeln!(out, "# TYPE intercom_rejected_total counter");
        let _ = writeln!(
            out,
            "intercom_rejected_total {}",
            self.metrics.rejected.load(Ordering::Relaxed)
        );
        let _ = writeln!(out, "# HELP intercom_active_calls Live calls.");
        let _ = writeln!(out, "# TYPE intercom_active_calls gauge");
        let _ = writeln!(out, "intercom_active_calls {}", status.active_calls);
        let _ = writeln!(
            out,
            "# HELP intercom_endpoints_online Endpoints with an open Watch stream."
        );
        let _ = writeln!(out, "# TYPE intercom_endpoints_online gauge");
        let _ = writeln!(out, "intercom_endpoints_online {}", status.endpoints_online);
        out
    }
}

/// Marks an endpoint offline when its Watch stream is dropped.
pub struct WatchGuard {
    intercom: Arc<Intercom>,
    endpoint_id: String,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.intercom.mark_offline(&self.endpoint_id);
    }
}

fn validate_sdp(
    inner: &Inner,
    call_id: &str,
    actor: &Identity,
    sdp: &SdpSignal,
) -> Result<(), DomainError> {
    let record = inner
        .calls
        .get(call_id)
        .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown call"))?;
    match sdp.kind {
        SdpType::Offer => {
            if record.caller_id != actor.endpoint_id {
                return Err(DomainError::new(
                    ErrorKind::PermissionDenied,
                    "only the caller can send an offer",
                ));
            }
            if record.offer_seen {
                return Err(DomainError::new(
                    ErrorKind::FailedPrecondition,
                    "offer already sent",
                ));
            }
        }
        SdpType::Answer => {
            if record.callee_id != actor.endpoint_id {
                return Err(DomainError::new(
                    ErrorKind::PermissionDenied,
                    "only the callee can send an answer",
                ));
            }
            if record.state == CallState::Ringing {
                return Err(DomainError::new(
                    ErrorKind::FailedPrecondition,
                    "answer before the call was answered",
                ));
            }
            if record.answer_seen {
                return Err(DomainError::new(
                    ErrorKind::FailedPrecondition,
                    "answer already sent",
                ));
            }
        }
    }
    if sdp.sdp.len() > 256 * 1024 {
        return Err(DomainError::new(
            ErrorKind::InvalidArgument,
            "SDP body is too large",
        ));
    }
    Ok(())
}

fn deliver(
    inner: &Inner,
    peer_id: &str,
    from_endpoint_id: &str,
    call_id: &str,
    signal: &Signal,
) -> bool {
    let Some(peer) = inner.endpoints.get(peer_id) else {
        return false;
    };
    let event = match signal {
        Signal::Sdp(signal) => Event::Sdp {
            call_id: call_id.to_owned(),
            from_endpoint_id: from_endpoint_id.to_owned(),
            signal: signal.clone(),
        },
        Signal::IceCandidate(candidate) => Event::IceCandidate {
            call_id: call_id.to_owned(),
            from_endpoint_id: from_endpoint_id.to_owned(),
            candidate: candidate.clone(),
        },
        Signal::IceEnd => Event::IceEnd {
            call_id: call_id.to_owned(),
            from_endpoint_id: from_endpoint_id.to_owned(),
        },
    };
    peer.events.send(event).is_ok()
}

fn broadcast_call(inner: &Inner, call: &Call, previous_state: Option<CallState>) {
    let event = Event::Call {
        call: call.clone(),
        previous_state,
    };
    for endpoint_id in [&call.caller.id, &call.callee.id] {
        if let Some(state) = inner.endpoints.get(endpoint_id) {
            let _ = state.events.send(event.clone());
        }
    }
}

fn touch(inner: &mut Inner, endpoint_id: &str, now: i64) {
    if let Some(state) = inner.endpoints.get_mut(endpoint_id) {
        state.info.last_seen_unix = now;
    }
}

fn live_call_for(inner: &Inner, endpoint_id: &str) -> Option<String> {
    inner
        .calls
        .values()
        .find(|record| {
            record.state.is_live()
                && (record.caller_id == endpoint_id || record.callee_id == endpoint_id)
        })
        .map(|record| record.id.clone())
}

fn live_call_mut<'a>(
    inner: &'a mut Inner,
    call_id: &str,
) -> Result<&'a mut CallRecord, DomainError> {
    let record = inner
        .calls
        .get_mut(call_id)
        .ok_or_else(|| DomainError::new(ErrorKind::NotFound, "unknown call"))?;
    if !record.state.is_live() {
        return Err(DomainError::new(
            ErrorKind::FailedPrecondition,
            "call has ended",
        ));
    }
    Ok(record)
}

fn end_call(inner: &mut Inner, call_id: &str, reason: EndReason, now: i64) -> CallState {
    let record = inner.calls.get_mut(call_id).expect("checked");
    let previous = record.state;
    record.state = CallState::Ended;
    record.end_reason = reason;
    record.ended = now;
    record.updated = now;
    previous
}

fn is_participant(record: &CallRecord, endpoint_id: &str) -> bool {
    record.caller_id == endpoint_id || record.callee_id == endpoint_id
}

fn call_view(record: &CallRecord, endpoints: &BTreeMap<String, EndpointState>) -> Call {
    Call {
        id: record.id.clone(),
        caller: endpoints.get(&record.caller_id).map_or_else(
            || missing_endpoint(&record.caller_id, EndpointKind::Phone),
            |state| state.info.clone(),
        ),
        callee: endpoints.get(&record.callee_id).map_or_else(
            || missing_endpoint(&record.callee_id, EndpointKind::Station),
            |state| state.info.clone(),
        ),
        state: record.state,
        end_reason: record.end_reason,
        created_unix: record.created,
        updated_unix: record.updated,
        answered_unix: record.answered,
        ended_unix: record.ended,
    }
}

fn missing_endpoint(id: &str, kind: EndpointKind) -> Endpoint {
    Endpoint {
        id: id.to_owned(),
        kind,
        display_name: String::new(),
        capabilities: Vec::new(),
        online: false,
        last_seen_unix: 0,
    }
}

/// A manual clock for deterministic tests.
#[cfg(test)]
pub(crate) struct TestClock {
    now: AtomicU64,
}

#[cfg(test)]
impl TestClock {
    pub(crate) fn new(now: i64) -> Arc<Self> {
        Arc::new(Self {
            now: AtomicU64::new(u64::try_from(now).unwrap_or(0)),
        })
    }

    pub(crate) fn advance(&self, seconds: u64) {
        self.now.fetch_add(seconds, Ordering::Relaxed);
    }
}

#[cfg(test)]
impl Clock for TestClock {
    fn now_unix(&self) -> i64 {
        i64::try_from(self.now.load(Ordering::Relaxed)).unwrap_or(i64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EndpointConfig;

    struct Harness {
        intercom: Arc<Intercom>,
        clock: Arc<TestClock>,
        phone: Identity,
        station: Identity,
    }

    fn harness() -> Harness {
        let clock = TestClock::new(1_000);
        let phone_config = EndpointConfig {
            id: "phone".into(),
            display_name: "Phone".into(),
            capabilities: vec!["audio".into()],
        };
        let station_config = EndpointConfig {
            id: "station".into(),
            display_name: "Station".into(),
            capabilities: vec!["audio".into()],
        };
        let intercom = Arc::new(Intercom::new(
            &phone_config,
            &station_config,
            Timeouts {
                ring: Duration::from_secs(30),
                connect: Duration::from_secs(20),
                max_call: Duration::from_secs(3600),
                ended_retention: Duration::from_secs(60),
            },
            Vec::new(),
            clock.clone(),
        ));
        Harness {
            intercom,
            clock,
            phone: Identity {
                endpoint_id: "phone".into(),
                kind: EndpointKind::Phone,
            },
            station: Identity {
                endpoint_id: "station".into(),
                kind: EndpointKind::Station,
            },
        }
    }

    impl Harness {
        fn online(&self, identity: &Identity) -> (broadcast::Receiver<Event>, WatchGuard) {
            self.intercom.watch(identity).expect("watch")
        }

        fn next_call(&self, rx: &mut broadcast::Receiver<Event>) -> (Call, Option<CallState>) {
            match rx.try_recv().expect("event") {
                Event::Call {
                    call,
                    previous_state,
                } => (call, previous_state),
                other => panic!("expected a call event, got {other:?}"),
            }
        }
    }

    #[test]
    fn ring_answer_active_hangup() {
        let h = harness();
        let (_station_events, _station_guard) = h.online(&h.station);
        let (mut phone_events, _phone_guard) = h.online(&h.phone);

        let call = h
            .intercom
            .start_call(&h.phone, "station", "k1")
            .expect("start");
        assert_eq!(call.state, CallState::Ringing);
        assert_eq!(call.caller.id, "phone");
        assert_eq!(call.callee.id, "station");
        assert_eq!(call.caller.kind, EndpointKind::Phone);
        assert_eq!(call.callee.kind, EndpointKind::Station);
        let (event, previous) = h.next_call(&mut phone_events);
        assert_eq!(event.id, call.id);
        assert_eq!(previous, None);

        let answered = h.intercom.answer(&h.station, &call.id).expect("answer");
        assert_eq!(answered.state, CallState::Connecting);
        assert!(answered.answered_unix > 0);

        let delivered = h
            .intercom
            .send_signal(
                &h.phone,
                &call.id,
                &Signal::Sdp(SdpSignal {
                    kind: SdpType::Offer,
                    sdp: "v=0".into(),
                }),
            )
            .expect("offer");
        assert!(delivered);
        let active = h
            .intercom
            .send_signal(
                &h.station,
                &call.id,
                &Signal::Sdp(SdpSignal {
                    kind: SdpType::Answer,
                    sdp: "v=0".into(),
                }),
            )
            .expect("answer sdp");
        assert!(active);
        assert_eq!(
            h.intercom.get_call(&h.phone, &call.id).expect("get").state,
            CallState::Active
        );

        let ended = h
            .intercom
            .hangup(&h.phone, &call.id, EndReason::Unspecified)
            .expect("hangup");
        assert_eq!(ended.state, CallState::Ended);
        assert_eq!(ended.end_reason, EndReason::Hangup);
        assert!(ended.ended_unix > 0);
    }

    #[test]
    fn ringing_timeout_ends_the_call() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        h.clock.advance(31);
        h.intercom.tick();
        let ended = h.intercom.get_call(&h.phone, &call.id).expect("get");
        assert_eq!(ended.state, CallState::Ended);
        assert_eq!(ended.end_reason, EndReason::Timeout);
    }

    #[test]
    fn connecting_timeout_ends_the_call() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        h.intercom.answer(&h.station, &call.id).expect("answer");
        h.clock.advance(21);
        h.intercom.tick();
        let ended = h.intercom.get_call(&h.station, &call.id).expect("get");
        assert_eq!(ended.end_reason, EndReason::Timeout);
    }

    #[test]
    fn ended_calls_are_purged_after_retention_and_idempotency_is_released() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "once")
            .expect("start");
        h.intercom
            .hangup(&h.station, &call.id, EndReason::Declined)
            .expect("decline");
        h.clock.advance(61);
        h.intercom.tick();
        assert!(h.intercom.get_call(&h.phone, &call.id).is_err());
        // The same key is free again after the ended call was purged.
        let next = h
            .intercom
            .start_call(&h.phone, "station", "once")
            .expect("reuse key");
        assert_ne!(next.id, call.id);
    }

    #[test]
    fn busy_is_rejected_on_either_side() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        let error = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect_err("busy");
        assert_eq!(error.kind, ErrorKind::FailedPrecondition);
        assert_eq!(error.message, "busy");
        h.intercom
            .hangup(&h.phone, &call.id, EndReason::Unspecified)
            .expect("hangup");
        let again = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("not busy after hangup");
        assert_eq!(again.state, CallState::Ringing);
    }

    #[test]
    fn hangup_during_ringing_infers_canceled_or_declined() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);

        let canceled = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        let ended = h
            .intercom
            .hangup(&h.phone, &canceled.id, EndReason::Unspecified)
            .expect("caller hangup");
        assert_eq!(ended.end_reason, EndReason::Canceled);

        let declined = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        let ended = h
            .intercom
            .hangup(&h.station, &declined.id, EndReason::Unspecified)
            .expect("callee hangup");
        assert_eq!(ended.end_reason, EndReason::Declined);
    }

    #[test]
    fn caller_cannot_call_itself_or_an_offline_callee() {
        let h = harness();
        let error = h
            .intercom
            .start_call(&h.phone, "phone", "")
            .expect_err("self call");
        assert_eq!(error.kind, ErrorKind::InvalidArgument);

        let error = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect_err("offline");
        assert_eq!(error.kind, ErrorKind::Unavailable);

        let error = h
            .intercom
            .start_call(&h.phone, "unknown", "")
            .expect_err("unknown");
        assert_eq!(error.kind, ErrorKind::NotFound);
    }

    #[test]
    fn only_the_callee_can_answer_and_signaling_is_directional() {
        let h = harness();
        let (_station_events, _guard) = h.online(&h.station);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");

        let error = h
            .intercom
            .answer(&h.phone, &call.id)
            .expect_err("caller answer");
        assert_eq!(error.kind, ErrorKind::PermissionDenied);

        let error = h
            .intercom
            .send_signal(
                &h.station,
                &call.id,
                &Signal::Sdp(SdpSignal {
                    kind: SdpType::Offer,
                    sdp: "v=0".into(),
                }),
            )
            .expect_err("callee offer");
        assert_eq!(error.kind, ErrorKind::PermissionDenied);

        // ICE may be trickled while the callee is still ringing: candidates
        // can arrive before the answer. It is routed to the peer.
        let delivered = h
            .intercom
            .send_signal(
                &h.phone,
                &call.id,
                &Signal::IceCandidate(IceCandidate {
                    candidate: "candidate:1 1 udp 1 192.0.2.10 5000 typ host".into(),
                    sdp_mid: "0".into(),
                    sdp_mline_index: 0,
                }),
            )
            .expect("ice while ringing");
        assert!(delivered);
    }

    #[test]
    fn watch_delivers_to_both_sides_and_routes_signals_to_the_peer() {
        let h = harness();
        let (mut station_events, _station_guard) = h.online(&h.station);
        let (mut phone_events, _phone_guard) = h.online(&h.phone);
        let call = h
            .intercom
            .start_call(&h.phone, "station", "")
            .expect("start");

        let (phone_view, _) = h.next_call(&mut phone_events);
        let (station_view, _) = h.next_call(&mut station_events);
        assert_eq!(phone_view.id, call.id);
        assert_eq!(station_view.id, call.id);
        assert_eq!(station_view.caller.id, "phone");

        h.intercom.answer(&h.station, &call.id).expect("answer");
        // Both sides observe the state change.
        h.next_call(&mut phone_events);
        h.next_call(&mut station_events);

        let delivered = h
            .intercom
            .send_signal(
                &h.phone,
                &call.id,
                &Signal::Sdp(SdpSignal {
                    kind: SdpType::Offer,
                    sdp: "v=0".into(),
                }),
            )
            .expect("offer");
        assert!(delivered);
        // Only the callee receives the offer, the caller receives no echo.
        match station_events.try_recv().expect("offer event") {
            Event::Sdp {
                call_id,
                from_endpoint_id,
                signal,
            } => {
                assert_eq!(call_id, call.id);
                assert_eq!(from_endpoint_id, "phone");
                assert_eq!(signal.sdp, "v=0");
            }
            other => panic!("expected SDP event, got {other:?}"),
        }
        assert!(phone_events.try_recv().is_err());

        h.intercom
            .send_signal(
                &h.phone,
                &call.id,
                &Signal::IceCandidate(IceCandidate {
                    candidate: "candidate:1 1 udp 1 192.0.2.10 5000 typ host".into(),
                    sdp_mid: "0".into(),
                    sdp_mline_index: 0,
                }),
            )
            .expect("ice");
        assert!(matches!(
            station_events.try_recv().expect("ice event"),
            Event::IceCandidate { .. }
        ));
    }

    #[test]
    fn idempotency_returns_the_same_call_or_rejects_a_different_callee() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        let first = h
            .intercom
            .start_call(&h.phone, "station", "same")
            .expect("start");
        let replay = h
            .intercom
            .start_call(&h.phone, "station", "same")
            .expect("replay");
        assert_eq!(first.id, replay.id);

        h.intercom
            .hangup(&h.phone, &first.id, EndReason::Unspecified)
            .expect("hangup");
        h.clock.advance(61);
        h.intercom.tick();
        // After purge the key is free; before purge a different callee conflicts.
        let h2 = harness();
        let (_events2, _guard2) = h2.online(&h2.station);
        h2.intercom
            .start_call(&h2.phone, "station", "k")
            .expect("start");
        let error = h2
            .intercom
            .start_call(&h2.station, "phone", "k")
            .expect_err("conflict");
        assert_eq!(error.kind, ErrorKind::AlreadyExists);
    }

    #[test]
    fn dropping_the_last_watch_ends_a_live_call_as_unreachable() {
        let h = harness();
        let (mut phone_events, _phone_guard) = h.online(&h.phone);
        {
            let (_station_events, _station_guard) = h.online(&h.station);
            h.intercom
                .start_call(&h.phone, "station", "")
                .expect("start");
            h.next_call(&mut phone_events);
            assert_eq!(h.intercom.status().active_calls, 1);
        }
        assert_eq!(h.intercom.status().active_calls, 0);
        assert_eq!(h.intercom.status().endpoints_online, 1);
        let (event, previous) = h.next_call(&mut phone_events);
        assert_eq!(event.state, CallState::Ended);
        assert_eq!(event.end_reason, EndReason::Unreachable);
        assert_eq!(previous, Some(CallState::Ringing));
    }

    #[test]
    fn metrics_render_counters_and_gauges() {
        let h = harness();
        let (_events, _guard) = h.online(&h.station);
        h.intercom
            .start_call(&h.phone, "station", "")
            .expect("start");
        let text = h.intercom.render_metrics();
        assert!(text.contains("intercom_calls_started_total 1"));
        assert!(text.contains("intercom_active_calls 1"));
        assert!(text.contains("intercom_endpoints_online 1"));
    }
}
