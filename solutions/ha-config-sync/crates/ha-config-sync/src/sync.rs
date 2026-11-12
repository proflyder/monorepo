//! One sync cycle: measure drift, apply, validate, reload, verify, roll back.
//!
//! The state machine is synchronous from the caller's point of view: [`Syncer`]
//! owns the agent and Home Assistant clients and runs one cycle per call. A
//! failed desired-set digest is retried with exponential backoff; a new digest
//! resets it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agent::{self, Agent, AgentError};
use crate::config::Mode;
use crate::ha::{HaApi, HaError};
use crate::manifest::Plan;
use crate::metrics::{ApplyResult, Metrics};
use crate::protocol::{AgentHash, DesiredFile, PROTOCOL_VERSION};

/// First retry delay after a failed apply.
pub const BACKOFF_BASE: Duration = Duration::from_secs(5 * 60);
/// Upper bound for the retry delay.
pub const BACKOFF_CAP: Duration = Duration::from_secs(6 * 60 * 60);
/// Default wait for Home Assistant to answer after a restart.
pub const RESTART_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a restart must be observed going down before it counts.
pub const RESTART_DOWN_TIMEOUT: Duration = Duration::from_secs(90);
/// Poll period while observing the restart.
pub const RESTART_POLL: Duration = Duration::from_secs(5);

/// Exponential retry gate for one desired digest.
#[derive(Debug)]
pub struct Backoff {
    digest: Option<String>,
    failures: u32,
    retry_after: Option<Instant>,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    /// An empty gate that allows the first attempt.
    #[must_use]
    pub fn new() -> Self {
        Self {
            digest: None,
            failures: 0,
            retry_after: None,
        }
    }

    /// The delay after `failures` consecutive failures (1-based).
    #[must_use]
    pub fn delay(failures: u32) -> Duration {
        let shift = failures.saturating_sub(1).min(16);
        let factor = 1u32 << shift;
        BACKOFF_BASE.saturating_mul(factor).min(BACKOFF_CAP)
    }

    /// Whether `digest` may be attempted at `now`.
    #[must_use]
    pub fn allows(&self, digest: &str, now: Instant) -> bool {
        if self.digest.as_deref() != Some(digest) {
            return true;
        }
        self.retry_after
            .is_none_or(|retry_after| now >= retry_after)
    }

    /// Record a failure and schedule the next attempt.
    pub fn record_failure(&mut self, digest: &str, now: Instant) {
        if self.digest.as_deref() == Some(digest) {
            self.failures = self.failures.saturating_add(1);
        } else {
            self.digest = Some(digest.to_owned());
            self.failures = 1;
        }
        self.retry_after = Some(now + Self::delay(self.failures));
    }

    /// Forget the failure history after a success or a new desired digest.
    pub fn reset(&mut self) {
        self.digest = None;
        self.failures = 0;
        self.retry_after = None;
    }

    /// How many consecutive failures the current digest has.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }
}

/// How a cycle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The host already matched the desired set.
    Unchanged,
    /// Observe mode: drift was measured but not applied.
    Observed,
    /// Applied, validated, reloaded and verified.
    Applied,
    /// Restart postponed because the guard entity was not `off`.
    Deferred,
    /// A deferred restart was completed without touching the files.
    Restarted,
    /// A failure was rolled back to the backup.
    RolledBack,
    /// A check, apply or rollback failed.
    Failed,
    /// A failed digest is inside its backoff window.
    Backoff,
}

/// The result of one cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    /// What happened.
    pub outcome: Outcome,
    /// Number of drifted files observed at the start.
    pub drift: usize,
}

impl CycleReport {
    fn new(outcome: Outcome, drift: usize) -> Self {
        Self { outcome, drift }
    }
}

/// Why the apply steps stopped.
#[derive(Debug)]
enum Stop {
    Deferred,
    Failed(String),
}

/// The sync worker.
pub struct Syncer<A: Agent, H: HaApi> {
    agent: A,
    ha: H,
    desired: BTreeMap<String, DesiredFile>,
    desired_digest: String,
    plan: Plan,
    metrics: Arc<Metrics>,
    mode: Mode,
    restart_guard_entity: Option<String>,
    restart_timeout: Duration,
    restart_down_timeout: Duration,
    restart_poll: Duration,
    restart_pending: bool,
    prune_keep: u32,
    backoff: Backoff,
}

impl<A: Agent, H: HaApi> Syncer<A, H> {
    /// Build a worker. `desired` and `plan` are already validated by the caller.
    #[must_use]
    pub fn new(
        agent: A,
        ha: H,
        desired: BTreeMap<String, DesiredFile>,
        plan: Plan,
        metrics: Arc<Metrics>,
        mode: Mode,
        restart_guard_entity: Option<String>,
    ) -> Self {
        let desired_digest = crate::protocol::desired_digest(&desired);
        metrics.set_managed(desired.len());
        metrics.set_desired_digest(&desired_digest);
        Self {
            agent,
            ha,
            desired,
            desired_digest,
            plan,
            metrics,
            mode,
            restart_guard_entity,
            restart_timeout: RESTART_TIMEOUT,
            restart_down_timeout: RESTART_DOWN_TIMEOUT,
            restart_poll: RESTART_POLL,
            restart_pending: false,
            prune_keep: crate::config::DEFAULT_PRUNE_KEEP,
            backoff: Backoff::new(),
        }
    }

    /// Override the restart wait (tests use a short bound).
    #[must_use]
    pub fn with_restart_timeout(mut self, timeout: Duration) -> Self {
        self.restart_timeout = timeout;
        self
    }

    /// Override the time allowed for Home Assistant to go down (tests use a
    /// short bound; production needs the 90 s default).
    #[must_use]
    pub fn with_restart_down_timeout(mut self, timeout: Duration) -> Self {
        self.restart_down_timeout = timeout;
        self
    }

    /// Override the polling period used while observing a restart.
    #[must_use]
    pub fn with_restart_poll(mut self, poll: Duration) -> Self {
        self.restart_poll = poll;
        self
    }

    /// Override how many backups survive a successful cycle.
    #[must_use]
    pub fn with_prune_keep(mut self, keep: u32) -> Self {
        self.prune_keep = keep;
        self
    }

    fn set_restart_pending(&mut self, pending: bool) {
        self.restart_pending = pending;
        self.metrics.set_restart_pending(pending);
    }

    /// Run one cycle.
    pub async fn cycle(&mut self) -> CycleReport {
        let started = Instant::now();
        match agent::version(&self.agent).await {
            Ok(version) => {
                self.metrics.set_agent_version(&version.to_string());
                if version != PROTOCOL_VERSION {
                    tracing::error!(
                        expected = PROTOCOL_VERSION,
                        found = version,
                        "agent protocol version mismatch"
                    );
                    self.metrics.record_check(false, unix_now());
                    return CycleReport::new(Outcome::Failed, 0);
                }
            }
            Err(error) => {
                tracing::error!(error = %error, "agent version call failed");
                self.metrics.record_check(false, unix_now());
                return CycleReport::new(Outcome::Failed, 0);
            }
        }

        let paths: Vec<String> = self.desired.keys().cloned().collect();
        let hashes = match agent::hashes(&self.agent, &paths).await {
            Ok(hashes) => hashes,
            Err(error) => {
                tracing::error!(error = %error, "agent hash call failed");
                self.metrics.record_check(false, unix_now());
                return CycleReport::new(Outcome::Failed, 0);
            }
        };
        let drifted = self.drifted(&hashes);
        self.metrics.set_drift(&drifted);
        let drift = drifted.len();

        if self.mode == Mode::Observe {
            tracing::info!(drift, "observed drift");
            self.metrics.record_check(true, unix_now());
            return CycleReport::new(Outcome::Observed, drift);
        }
        if drifted.is_empty() && !self.restart_pending {
            tracing::debug!("no drift");
            self.backoff.reset();
            self.metrics.record_check(true, unix_now());
            return CycleReport::new(Outcome::Unchanged, 0);
        }
        if drifted.is_empty() {
            // The files are already in place; only the deferred restart is
            // outstanding. Retry guard -> restart -> wait -> verify.
            let mut restarted = false;
            return match self.restart_flow(&mut restarted).await {
                Ok(()) => {
                    if let Err(error) = self.verify_hashes().await {
                        tracing::error!(error = ?error, "deferred restart verification failed");
                        self.metrics.record_check(false, unix_now());
                        CycleReport::new(Outcome::Failed, 0)
                    } else {
                        self.set_restart_pending(false);
                        self.metrics.record_check(true, unix_now());
                        tracing::info!("deferred restart completed");
                        CycleReport::new(Outcome::Restarted, 0)
                    }
                }
                Err(Stop::Deferred) => {
                    self.metrics.record_check(false, unix_now());
                    CycleReport::new(Outcome::Deferred, 0)
                }
                Err(Stop::Failed(reason)) => {
                    tracing::error!(reason = %reason, "deferred restart failed");
                    self.metrics.record_check(false, unix_now());
                    CycleReport::new(Outcome::Failed, 0)
                }
            };
        }
        if !self.backoff.allows(&self.desired_digest, started) {
            tracing::warn!(
                drift,
                failures = self.backoff.failures(),
                "desired set is in retry backoff"
            );
            self.metrics.record_check(false, unix_now());
            return CycleReport::new(Outcome::Backoff, drift);
        }

        let backup = match agent::apply(&self.agent, &self.desired).await {
            Ok(backup) => backup,
            Err(error) => {
                tracing::error!(error = %error, drift, "agent apply failed");
                self.metrics.record_apply_timestamp(unix_now());
                self.metrics.count_apply(ApplyResult::Failed);
                self.backoff.record_failure(&self.desired_digest, started);
                self.metrics.record_check(false, unix_now());
                return CycleReport::new(Outcome::Failed, drift);
            }
        };
        tracing::info!(backup = %backup, drift, "agent applied desired files");

        let pending_before = self.restart_pending;
        let mut restarted = false;
        match self.apply_steps(&drifted, &mut restarted).await {
            Ok(()) => {
                self.set_restart_pending(false);
                self.metrics.record_apply_timestamp(unix_now());
                self.metrics.count_apply(ApplyResult::Success);
                self.metrics.record_check(true, unix_now());
                self.backoff.reset();
                if let Err(error) = agent::prune(&self.agent, self.prune_keep).await {
                    tracing::warn!(error = %error, "prune failed");
                }
                tracing::info!(drift, "desired files applied and verified");
                CycleReport::new(Outcome::Applied, drift)
            }
            Err(Stop::Deferred) => {
                self.set_restart_pending(true);
                self.metrics.record_apply_timestamp(unix_now());
                self.metrics.count_apply(ApplyResult::Deferred);
                self.metrics.record_check(false, unix_now());
                tracing::warn!("restart deferred by guard entity");
                CycleReport::new(Outcome::Deferred, drift)
            }
            Err(Stop::Failed(reason)) => {
                tracing::error!(reason = %reason, "apply failed, rolling back");
                let rolled_back = self.rollback(&backup, &drifted, restarted).await;
                // Restoring returns to the pre-apply files; a restart that was
                // already pending before this cycle is still pending.
                self.set_restart_pending(pending_before);
                self.metrics.record_apply_timestamp(unix_now());
                self.metrics.count_apply(if rolled_back {
                    ApplyResult::RolledBack
                } else {
                    ApplyResult::Failed
                });
                self.backoff.record_failure(&self.desired_digest, started);
                self.metrics.record_check(false, unix_now());
                CycleReport::new(
                    if rolled_back {
                        Outcome::RolledBack
                    } else {
                        Outcome::Failed
                    },
                    drift,
                )
            }
        }
    }

    fn drifted(&self, hashes: &BTreeMap<String, AgentHash>) -> BTreeSet<String> {
        self.desired
            .iter()
            .filter(|(path, file)| {
                hashes.get(path.as_str()) != Some(&AgentHash::Present(file.sha256.clone()))
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    async fn apply_steps(
        &self,
        drifted: &BTreeSet<String>,
        restarted: &mut bool,
    ) -> Result<(), Stop> {
        let check = self
            .ha
            .check_config()
            .await
            .map_err(|error| Stop::Failed(format!("check_config request: {error}")))?;
        if !check.valid {
            let detail = check.errors.unwrap_or_default();
            return Err(Stop::Failed(format!(
                "check_config invalid: {}",
                truncate(&detail, 256)
            )));
        }
        for service in self.plan.reload_services(drifted) {
            self.ha
                .call_service(&service)
                .await
                .map_err(|error| Stop::Failed(format!("reload {service}: {error}")))?;
        }
        if self.plan.needs_restart(drifted) || self.restart_pending {
            // A deferred restart is still outstanding even when this cycle's
            // drift belongs to other files.
            self.restart_flow(restarted).await?;
        }
        self.verify_hashes().await
    }

    /// Guard -> restart request -> observe down -> wait up.
    ///
    /// Shared by a normal apply and by a deferred restart whose files are
    /// already on disk.
    async fn restart_flow(&self, restarted: &mut bool) -> Result<(), Stop> {
        if let Some(entity) = self.restart_guard_entity.as_deref() {
            match self.ha.state(entity).await {
                Ok(state) if state == "off" => {}
                Ok(_) => return Err(Stop::Deferred),
                Err(error) => {
                    tracing::warn!(entity, error = %error, "guard state read failed; deferring");
                    return Err(Stop::Deferred);
                }
            }
        }
        self.request_restart().await?;
        *restarted = true;
        self.wait_for_ha_cycle().await
    }

    /// Request a restart. Home Assistant often drops the HTTP connection or
    /// answers 5xx while it shuts down; that is the expected "restart in
    /// progress" answer, not a failure. Only a 4xx is a real failure.
    async fn request_restart(&self) -> Result<(), Stop> {
        match self.ha.call_service("homeassistant.restart").await {
            Ok(()) => Ok(()),
            Err(error) if restart_in_progress(&error) => {
                tracing::warn!(error = %error, "restart request ended while HA was stopping");
                Ok(())
            }
            Err(error) => Err(Stop::Failed(format!("restart request: {error}"))),
        }
    }

    /// Watch the restart actually happen: first a failed `/api/` probe within
    /// [`RESTART_DOWN_TIMEOUT`], then a successful one within
    /// [`RESTART_TIMEOUT`].
    async fn wait_for_ha_cycle(&self) -> Result<(), Stop> {
        let down_deadline = Instant::now() + self.restart_down_timeout;
        loop {
            if self.ha.ping().await.is_err() {
                break;
            }
            if Instant::now() >= down_deadline {
                return Err(Stop::Failed(
                    "Home Assistant did not go down after the restart request".to_owned(),
                ));
            }
            tokio::time::sleep(self.restart_poll).await;
        }
        let up_deadline = Instant::now() + self.restart_timeout;
        loop {
            if self.ha.ping().await.is_ok() {
                return Ok(());
            }
            if Instant::now() >= up_deadline {
                return Err(Stop::Failed(
                    "Home Assistant did not answer after restart".to_owned(),
                ));
            }
            tokio::time::sleep(self.restart_poll).await;
        }
    }

    async fn verify_hashes(&self) -> Result<(), Stop> {
        let paths: Vec<String> = self.desired.keys().cloned().collect();
        let hashes = agent::hashes(&self.agent, &paths)
            .await
            .map_err(|error| Stop::Failed(format!("verify hashes: {error}")))?;
        let remaining = self.drifted(&hashes);
        if !remaining.is_empty() {
            return Err(Stop::Failed(format!(
                "hash mismatch after apply: {}",
                remaining.into_iter().collect::<Vec<_>>().join(",")
            )));
        }
        Ok(())
    }

    /// Restore the backup and repeat the reload plan.
    ///
    /// The return value says whether the files were restored. A reload or
    /// restart that fails after a successful restore is logged but does not
    /// change the outcome: the files are back, which is what "rolled back"
    /// reports to the operator.
    async fn rollback(&self, backup: &str, drifted: &BTreeSet<String>, restarted: bool) -> bool {
        let restored = match agent::restore(&self.agent, backup).await {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(error = %error, backup, "agent restore failed");
                false
            }
        };
        for service in self.plan.reload_services(drifted) {
            if let Err(error) = self.ha.call_service(&service).await {
                tracing::error!(service, error = %error, "rollback reload failed");
            }
        }
        if restarted && let Err(error) = self.ha.call_service("homeassistant.restart").await {
            tracing::error!(error = %error, "rollback restart failed");
        }
        if restored {
            tracing::warn!(backup, "rolled back to the previous files");
        }
        restored
    }
}

/// Whether a restart call failed only because Home Assistant was already
/// stopping. Transport failures, timeouts (transport errors) and 5xx mean the
/// restart is in progress; everything else, notably 4xx, is a real failure.
fn restart_in_progress(error: &HaError) -> bool {
    match error {
        HaError::Transport(_)
        | HaError::Client(home_ha::Error::Transport { .. })
        | HaError::Client(home_ha::Error::WebSocket { .. }) => true,
        HaError::Status(status) | HaError::Client(home_ha::Error::Status { status, .. }) => {
            *status >= 500
        }
        HaError::Client(_) | HaError::Decode => false,
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

/// Ensure an agent error is logged by class, not by content.
#[must_use]
pub fn describe_agent_error(error: &AgentError) -> &'static str {
    match error {
        AgentError::Transport(_) => "transport",
        AgentError::Exit(_) => "exit",
        AgentError::Protocol(_) => "protocol",
        AgentError::EmptyBackup => "backup",
    }
}

/// Ensure a Home Assistant error is logged by class, not by content.
#[must_use]
pub fn describe_ha_error(error: &HaError) -> &'static str {
    match error {
        HaError::Client(_) => "client",
        HaError::Transport(_) => "transport",
        HaError::Status(_) => "status",
        HaError::Decode => "decode",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::extract::{Path, State};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use base64::Engine as _;
    use home_secret::Token;
    use serde_json::json;
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::agent::{AgentAnswer, AgentFuture};
    use crate::ha::Ha;
    use crate::manifest::Plan;
    use crate::protocol::{self, DesiredFile};

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn desired(pairs: &[(&str, &[u8])]) -> BTreeMap<String, DesiredFile> {
        pairs
            .iter()
            .map(|(path, content)| {
                (
                    (*path).to_owned(),
                    DesiredFile {
                        sha256: sha(content),
                        content: content.to_vec(),
                    },
                )
            })
            .collect()
    }

    fn manifest_for(desired: &BTreeMap<String, DesiredFile>) -> String {
        let mut manifest = String::from("files:\n");
        for path in desired.keys() {
            let _ = writeln!(manifest, "  - glob: \"{path}\"");
            if path.ends_with("shell_commands.yaml") {
                manifest.push_str("    restart: true\n");
            } else if path.contains("custom_sentences") {
                manifest.push_str("    reload: [\"conversation.reload\"]\n");
            } else {
                manifest.push_str("    reload: [\"script.reload\"]\n");
            }
        }
        manifest
    }

    struct FakeAgent {
        files: Mutex<BTreeMap<String, Vec<u8>>>,
        corrupt_after_apply: bool,
        version: String,
        calls: Mutex<Vec<String>>,
        restore_calls: AtomicUsize,
        snapshots: Mutex<BTreeMap<String, Option<Vec<u8>>>>,
    }

    impl FakeAgent {
        fn new(files: &[(&str, &[u8])]) -> Self {
            Self {
                files: Mutex::new(
                    files
                        .iter()
                        .map(|(path, content)| ((*path).to_owned(), content.to_vec()))
                        .collect(),
                ),
                corrupt_after_apply: false,
                version: format!("ha-config-agent {PROTOCOL_VERSION}\n"),
                calls: Mutex::new(Vec::new()),
                restore_calls: AtomicUsize::new(0),
                snapshots: Mutex::new(BTreeMap::new()),
            }
        }

        fn hash_of(&self, path: &str) -> Option<String> {
            let files = self.files.lock().expect("files");
            files.get(path).map(|content| {
                if self.corrupt_after_apply {
                    sha(&[content.as_slice(), b"corrupt"].concat())
                } else {
                    sha(content)
                }
            })
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl Agent for FakeAgent {
        fn call<'a>(&'a self, verb: &'a str, stdin: &'a [u8]) -> AgentFuture<'a, AgentAnswer> {
            Box::pin(async move {
                self.calls.lock().expect("calls").push(verb.to_owned());
                let stdout = match verb {
                    "version" => self.version.clone().into_bytes(),
                    "hashes" => {
                        let text = std::str::from_utf8(stdin).expect("stdin");
                        let mut out = String::new();
                        for path in text.lines().filter(|line| !line.is_empty()) {
                            match self.hash_of(path) {
                                Some(hash) => {
                                    let _ = writeln!(out, "{hash} {path}");
                                }
                                None => {
                                    let _ = writeln!(out, "MISSING {path}");
                                }
                            }
                        }
                        out.into_bytes()
                    }
                    "apply" => {
                        let text = std::str::from_utf8(stdin).expect("stdin");
                        {
                            let mut snapshots = self.snapshots.lock().expect("snapshots");
                            snapshots.clear();
                            let mut files = self.files.lock().expect("files");
                            for line in text.lines().filter(|line| !line.is_empty()) {
                                let (path, rest) = line.split_once(' ').expect("path");
                                let (hash, encoded) = rest.split_once(' ').expect("hash");
                                let content = base64::engine::general_purpose::STANDARD
                                    .decode(encoded)
                                    .expect("base64");
                                assert_eq!(sha(&content), hash);
                                snapshots.insert(path.to_owned(), files.get(path).cloned());
                                files.insert(path.to_owned(), content);
                            }
                            drop(files);
                            drop(snapshots);
                        }
                        b"20260101T000000Z\n".to_vec()
                    }
                    verb if verb.starts_with("restore ") => {
                        self.restore_calls.fetch_add(1, Ordering::Relaxed);
                        {
                            let snapshots = self.snapshots.lock().expect("snapshots");
                            let mut files = self.files.lock().expect("files");
                            for (path, previous) in snapshots.iter() {
                                match previous {
                                    Some(content) => {
                                        files.insert(path.clone(), content.clone());
                                    }
                                    None => {
                                        files.remove(path);
                                    }
                                }
                            }
                            drop(files);
                            drop(snapshots);
                        }
                        Vec::new()
                    }
                    verb if verb.starts_with("prune ") => Vec::new(),
                    other => panic!("unexpected verb {other}"),
                };
                Ok(AgentAnswer {
                    stdout,
                    exit_code: Some(0),
                })
            })
        }
    }

    #[derive(Clone)]
    struct FakeHaState {
        config_valid: bool,
        reload_status: u16,
        restart_status: u16,
        restart_down_pings: usize,
        down_pings: Arc<Mutex<usize>>,
        guard_state: Arc<Mutex<String>>,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl FakeHaState {
        fn new() -> Self {
            Self {
                config_valid: true,
                reload_status: 200,
                restart_status: 200,
                restart_down_pings: 1,
                down_pings: Arc::new(Mutex::new(0)),
                guard_state: Arc::new(Mutex::new("off".to_owned())),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn set_guard(&self, state: &str) {
            *self.guard_state.lock().expect("guard") = state.to_owned();
        }
    }

    async fn start_ha(state: FakeHaState) -> String {
        let app = Router::new()
            .route(
                "/api/config/core/check_config",
                post(|State(state): State<FakeHaState>| async move {
                    state
                        .calls
                        .lock()
                        .expect("calls")
                        .push("check_config".to_owned());
                    let body = if state.config_valid {
                        json!({"result": "valid", "errors": null})
                    } else {
                        json!({"result": "invalid", "errors": {"scripts.yaml": "bad yaml"}})
                    };
                    Json(body)
                }),
            )
            .route(
                "/api/services/{domain}/{service}",
                post(
                    |State(state): State<FakeHaState>,
                     Path((domain, service)): Path<(String, String)>| async move {
                        let name = format!("{domain}.{service}");
                        state.calls.lock().expect("calls").push(name);
                        if service == "restart" {
                            // Even a 5xx or a dropped connection means HA is
                            // stopping; simulate the downtime in both cases.
                            *state.down_pings.lock().expect("down pings") =
                                state.restart_down_pings;
                            if state.restart_status != 200 {
                                let status = axum::http::StatusCode::from_u16(state.restart_status)
                                    .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                                return (status, Json(json!({})));
                            }
                            return (axum::http::StatusCode::OK, Json(json!({})));
                        }
                        let status = if state.reload_status == 200 {
                            axum::http::StatusCode::OK
                        } else {
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR
                        };
                        (status, Json(json!({})))
                    },
                ),
            )
            .route(
                "/api/states/{entity}",
                get(
                    |State(state): State<FakeHaState>, Path(entity): Path<String>| async move {
                        let guard = state.guard_state.lock().expect("guard").clone();
                        Json(json!({"entity_id": entity, "state": guard}))
                    },
                ),
            )
            .route(
                "/api/",
                get(|State(state): State<FakeHaState>| async move {
                    let down = {
                        let mut remaining = state.down_pings.lock().expect("down pings");
                        if *remaining > 0 {
                            *remaining -= 1;
                            true
                        } else {
                            false
                        }
                    };
                    if down {
                        return (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            Json(json!({"message": "restarting"})),
                        );
                    }
                    (
                        axum::http::StatusCode::OK,
                        Json(json!({"message": "API running."})),
                    )
                }),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn syncer(
        agent: FakeAgent,
        desired: BTreeMap<String, DesiredFile>,
        ha_url: &str,
        metrics: Arc<Metrics>,
        guard: Option<&str>,
    ) -> Syncer<FakeAgent, Ha> {
        let plan = Plan::parse(&manifest_for(&desired), &desired).expect("plan");
        Syncer::new(
            agent,
            Ha::new(ha_url, Token::new("test-token")).expect("ha"),
            desired,
            plan,
            metrics,
            Mode::Apply,
            guard.map(str::to_owned),
        )
        .with_restart_timeout(Duration::from_secs(2))
        .with_restart_down_timeout(Duration::from_secs(2))
        .with_restart_poll(Duration::from_millis(10))
    }

    #[tokio::test]
    async fn no_drift_only_reports_metrics() {
        let ha = FakeHaState::new();
        let calls = ha.calls.clone();
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("scripts.yaml", b"a: 1\n")]);
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Unchanged);
        assert_eq!(report.drift, 0);
        assert!(calls.lock().expect("calls").is_empty());
        assert!(!syncer.agent.calls().iter().any(|call| call == "apply"));
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_last_check_success 1\n"));
        assert!(text.contains("ha_config_sync_drift_files 0\n"));
    }

    #[tokio::test]
    async fn apply_success_reloads_and_verifies() {
        let ha = FakeHaState::new();
        let calls = ha.calls.clone();
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Applied);
        assert_eq!(report.drift, 1);
        assert_eq!(
            calls.lock().expect("calls").as_slice(),
            ["check_config", "script.reload"]
        );
        assert!(
            syncer
                .agent
                .calls()
                .iter()
                .any(|call| call.starts_with("prune "))
        );
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_apply_total{result=\"success\"} 1\n"));
        assert!(text.contains("ha_config_sync_last_check_success 1\n"));
    }

    #[tokio::test]
    async fn invalid_config_is_rolled_back_and_reloaded() {
        let mut ha = FakeHaState::new();
        ha.config_valid = false;
        let calls = ha.calls.clone();
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::RolledBack);
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            calls.lock().expect("calls").as_slice(),
            ["check_config", "script.reload"]
        );
        assert_eq!(
            syncer.agent.files.lock().expect("files")["scripts.yaml"],
            b"a: 0\n"
        );
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_apply_total{result=\"rolled_back\"} 1\n"));
    }

    #[tokio::test]
    async fn a_reload_error_is_rolled_back() {
        let mut ha = FakeHaState::new();
        ha.reload_status = 500;
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::RolledBack);
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 1);
        // one failed reload, then the rollback reload also fails.
        assert_eq!(
            metrics
                .render()
                .matches("ha_config_sync_apply_total{result=\"rolled_back\"} 1")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_restart_is_deferred_while_the_guard_is_on() {
        let ha = FakeHaState::new();
        ha.set_guard("on");
        let calls = ha.calls.clone();
        let url = start_ha(ha.clone()).await;
        let agent = FakeAgent::new(&[("shell_commands.yaml", b"a: 0\n")]);
        let desired = desired(&[("shell_commands.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(
            agent,
            desired,
            &url,
            metrics.clone(),
            Some("input_boolean.alarm_ringing"),
        );
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Deferred);
        let calls = calls.lock().expect("calls").clone();
        assert_eq!(calls, ["check_config"]);
        assert!(!calls.iter().any(|call| call == "homeassistant.restart"));
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 0);
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_apply_total{result=\"deferred\"} 1\n")
        );
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_restart_pending 1\n")
        );

        // Files are already in place, so the next cycle sees no drift but must
        // still complete the deferred restart.
        ha.set_guard("off");
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Restarted);
        assert_eq!(report.drift, 0);
        assert!(
            ha.calls
                .lock()
                .expect("calls")
                .iter()
                .any(|call| call == "homeassistant.restart")
        );
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_restart_pending 0\n")
        );
    }

    #[tokio::test]
    async fn a_hash_mismatch_after_apply_is_rolled_back() {
        let ha = FakeHaState::new();
        let url = start_ha(ha).await;
        let mut agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        agent.corrupt_after_apply = true;
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::RolledBack);
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_pending_restart_survives_drift_in_other_files() {
        let ha = FakeHaState::new();
        ha.set_guard("on");
        let url = start_ha(ha.clone()).await;
        let agent = FakeAgent::new(&[
            ("scripts.yaml", b"old\n"),
            ("shell_commands.yaml", b"old\n"),
        ]);
        let desired = desired(&[
            ("scripts.yaml", b"new\n"),
            ("shell_commands.yaml", b"new\n"),
        ]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(
            agent,
            desired,
            &url,
            metrics.clone(),
            Some("input_boolean.alarm_ringing"),
        );
        assert_eq!(syncer.cycle().await.outcome, Outcome::Deferred);
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_restart_pending 1\n")
        );

        ha.set_guard("off");
        // Another managed file drifts before the deferred restart happened.
        syncer
            .agent
            .files
            .lock()
            .expect("files")
            .insert("scripts.yaml".to_owned(), b"tampered\n".to_vec());
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Applied);
        assert!(
            ha.calls
                .lock()
                .expect("calls")
                .iter()
                .any(|call| call == "homeassistant.restart")
        );
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_restart_pending 0\n")
        );
    }

    #[tokio::test]
    async fn a_restart_5xx_is_treated_as_in_progress() {
        let mut ha = FakeHaState::new();
        ha.restart_status = 500;
        let calls = ha.calls.clone();
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("shell_commands.yaml", b"a: 0\n")]);
        let desired = desired(&[("shell_commands.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Applied);
        assert!(
            calls
                .lock()
                .expect("calls")
                .iter()
                .any(|call| call == "homeassistant.restart")
        );
        assert_eq!(
            metrics
                .render()
                .matches("ha_config_sync_apply_total{result=\"success\"} 1")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_restart_4xx_is_a_failure_and_rolls_back() {
        let mut ha = FakeHaState::new();
        ha.restart_status = 403;
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("shell_commands.yaml", b"a: 0\n")]);
        let desired = desired(&[("shell_commands.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::RolledBack);
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 1);
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_apply_total{result=\"rolled_back\"} 1\n")
        );
    }

    #[tokio::test]
    async fn a_restart_that_never_goes_down_is_a_failure() {
        let mut ha = FakeHaState::new();
        ha.restart_down_pings = 0;
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("shell_commands.yaml", b"a: 0\n")]);
        let desired = desired(&[("shell_commands.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics.clone(), None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::RolledBack);
        assert_eq!(syncer.agent.restore_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn restart_error_classification() {
        assert!(restart_in_progress(&HaError::Status(503)));
        assert!(restart_in_progress(&HaError::Status(500)));
        assert!(!restart_in_progress(&HaError::Status(400)));
        assert!(!restart_in_progress(&HaError::Decode));
        assert!(restart_in_progress(&HaError::Client(
            home_ha::Error::Status {
                op: "call service",
                status: 502,
                message: "bad gateway".to_owned(),
            }
        )));
        assert!(!restart_in_progress(&HaError::Client(
            home_ha::Error::Status {
                op: "call service",
                status: 401,
                message: "unauthorized".to_owned(),
            }
        )));
    }

    #[tokio::test]
    async fn observe_mode_never_applies() {
        let ha = FakeHaState::new();
        let url = start_ha(ha).await;
        let agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Observe));
        let plan = Plan::parse(&manifest_for(&desired), &desired).expect("plan");
        let mut syncer = Syncer::new(
            agent,
            Ha::new(&url, Token::new("t")).expect("ha"),
            desired,
            plan,
            metrics.clone(),
            Mode::Observe,
            None,
        );
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Observed);
        assert_eq!(report.drift, 1);
        assert!(!syncer.agent.calls().iter().any(|call| call == "apply"));
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_drift{file=\"scripts.yaml\"} 1\n")
        );
    }

    #[tokio::test]
    async fn version_mismatch_stops_the_cycle() {
        let ha = FakeHaState::new();
        let calls = ha.calls.clone();
        let url = start_ha(ha).await;
        let mut agent = FakeAgent::new(&[("scripts.yaml", b"a: 0\n")]);
        agent.version = "ha-config-agent 99\n".to_owned();
        let desired = desired(&[("scripts.yaml", b"a: 1\n")]);
        let metrics = Arc::new(Metrics::new(Mode::Apply));
        let mut syncer = syncer(agent, desired, &url, metrics, None);
        let report = syncer.cycle().await;
        assert_eq!(report.outcome, Outcome::Failed);
        assert!(calls.lock().expect("calls").is_empty());
        assert!(!syncer.agent.calls().iter().any(|call| call == "hashes"));
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(Backoff::delay(1), Duration::from_secs(300));
        assert_eq!(Backoff::delay(2), Duration::from_secs(600));
        assert_eq!(Backoff::delay(3), Duration::from_secs(1200));
        assert_eq!(Backoff::delay(20), BACKOFF_CAP);
    }

    #[test]
    fn backoff_is_scoped_to_one_digest() {
        let now = Instant::now();
        let mut backoff = Backoff::new();
        backoff.record_failure("digest-a", now);
        assert!(!backoff.allows("digest-a", now));
        assert!(backoff.allows("digest-b", now));
        assert!(backoff.allows("digest-a", now + BACKOFF_BASE));
        backoff.record_failure("digest-a", now + BACKOFF_BASE);
        assert_eq!(backoff.failures(), 2);
        assert!(!backoff.allows("digest-a", now + BACKOFF_BASE + Duration::from_secs(300)));
        backoff.reset();
        assert!(backoff.allows("digest-a", now));
    }

    #[test]
    fn backup_ids_are_validated() {
        assert!(protocol::is_backup_id("20260101T000000Z"));
        assert!(protocol::is_backup_id("20260101T000000Z-2"));
        assert!(!protocol::is_backup_id("../../etc"));
        assert!(!protocol::is_backup_id("20260101T000000Z/../x"));
        assert!(!protocol::is_backup_id(""));
        assert!(!protocol::is_backup_id("20260101T000000Z-"));
    }

    #[test]
    fn error_descriptions_are_stable() {
        assert_eq!(describe_agent_error(&AgentError::Exit(Some(1))), "exit");
        assert_eq!(describe_ha_error(&HaError::Decode), "decode");
    }
}
