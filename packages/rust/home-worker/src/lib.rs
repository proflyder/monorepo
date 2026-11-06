//! The shared executor loop: lease → handle → ack.
//!
//! The home services used to repeat the same loop three times (ha-bridge,
//! ha-control, the voice-proxy consumer). This crate owns the reusable part
//! and leaves the domain decisions to a [`Handler`]:
//!
//! * [`Queue`] leases due events and reports a terminal outcome;
//! * [`Handler`] answers [`Outcome::Delivered`], [`Outcome::Failed`],
//!   [`Outcome::Skipped`] or [`Outcome::Retry`]. `Failed` acks the event as
//!   terminal `FAILED` (a structural error that a retry cannot fix), `Skipped`
//!   acks `SKIPPED`, and `Retry` does **not** ack: the lease expires and the
//!   scheduler returns the event to `PENDING`, exactly like voice-proxy on a
//!   failed announcement;
//! * [`Handler::order`] may reorder one leased batch before it is handled;
//!   ha-bridge uses it for "stop before fire" inside a batch;
//! * an optional [`Gate`] ("may this executor take work right now?") is
//!   checked before every lease — voice-proxy uses it for an idle station;
//! * queue errors back off exponentially (with a ceiling) and cancellation is
//!   cooperative through [`CancellationToken`].
//!
//! [`SchedulerQueue`] implements [`Queue`] over the Connect client of
//! `home-proto`; there is no cycle because `home-proto` knows nothing about
//! the loop.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use backon::{BackoffBuilder, ExponentialBuilder};
use tokio_util::sync::CancellationToken;

mod scheduler_queue;

pub use scheduler_queue::SchedulerQueue;

/// One leased event, normalized from the wire `DueEvent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    /// Scheduler source identity (schedule/timer/task id).
    pub source_id: String,
    pub task_type: String,
    pub queue: String,
    /// Human-facing label of the occurrence (alarm name, reminder text). It is
    /// display data, not an identifier.
    pub label: String,
    /// Opaque `payload_json`. Treat it as data only, never as a service call
    /// or entity identifier.
    pub payload: String,
    pub action: Action,
    pub source: Source,
    pub attempts: i32,
    pub due_unix: i64,
    pub occurrence_unix: i64,
    pub lease_until_unix: i64,
}

/// What an executor should do for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start the occurrence. Legacy `UNSPECIFIED` rows also map here.
    Fire,
    /// Cancel sound for the occurrence.
    Stop,
    /// A wire value this version does not know.
    Other(i32),
}

/// What produced the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Unspecified,
    Alarm,
    Reminder,
    Timer,
    Scenario,
    Task,
    Other(i32),
}

/// Terminal outcome reported to the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    Delivered,
    Failed,
    Skipped,
}

/// The handler's decision for one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The event was handled; ack `DELIVERED` with an optional detail.
    Delivered(String),
    /// The event is structurally unusable; ack `FAILED` with an optional
    /// detail. The scheduler records `FAILED` as terminal, so the lease is not
    /// retried.
    Failed(String),
    /// The event is not applicable; ack `SKIPPED` with an optional detail.
    Skipped(String),
    /// Leave the event leased and unacked; expiry returns it to `PENDING`.
    Retry,
}

/// One lease request. The worker keeps a template and clones it per call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRequest {
    pub consumer_id: String,
    pub queue: String,
    pub lease_seconds: i32,
    pub limit: i32,
}

/// One terminal ack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ack {
    pub event_id: String,
    pub consumer_id: String,
    pub outcome: AckOutcome,
    pub detail: String,
}

/// A queue failure. The worker retries with exponential backoff.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("queue: {0}")]
    Client(#[from] home_proto::SchedulerError),
    #[error("queue: {detail}")]
    Other { detail: String },
}

impl QueueError {
    /// Build an implementation-specific error without pulling a new error
    /// crate into the trait.
    #[must_use]
    pub fn other(detail: impl Into<String>) -> Self {
        Self::Other {
            detail: detail.into(),
        }
    }
}

/// A durable event queue.
pub trait Queue: Send + Sync {
    /// Lease up to `request.limit` due events. An empty vector means "nothing
    /// due", not an error.
    fn lease(
        &self,
        request: LeaseRequest,
    ) -> impl Future<Output = Result<Vec<Event>, QueueError>> + Send;

    /// Report a terminal outcome. `Outcome::Retry` never reaches this method.
    fn ack(&self, ack: Ack) -> impl Future<Output = Result<(), QueueError>> + Send;
}

/// The domain action for one event.
pub trait Handler: Send + Sync {
    fn handle(&self, event: &Event) -> impl Future<Output = Outcome> + Send;

    /// Reorder or filter one leased batch before any event is handled.
    ///
    /// The default is a no-op, so the batch keeps the server order. The hook
    /// runs once per non-empty lease and may not block: it is synchronous
    /// because the only intended use is ordering (for example source order,
    /// then `due_unix`, with `STOP` ahead of `FIRE`).
    fn order(&self, _events: &mut Vec<Event>) {}
}

/// An optional "can this executor take work now?" predicate.
///
/// The boxed future keeps the trait object-safe: [`Worker`] stores
/// `Option<Box<dyn Gate>>` so a solution can install a runtime-specific gate
/// (for example the voice-proxy station-idle check) without a generic parameter
/// leaking into every type.
pub trait Gate: Send + Sync {
    fn ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
}

/// A gate that is always open (the default).
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysReady;

impl Gate for AlwaysReady {
    fn ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(std::future::ready(true))
    }
}

/// Worker tuning.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Lease template; `consumer_id` and `queue` are the executor identity.
    pub lease: LeaseRequest,
    /// Pause between empty leases and while the gate is closed.
    pub poll_interval: Duration,
    /// First retry delay after a queue error.
    pub retry_min: Duration,
    /// Ceiling of the exponential retry delay.
    pub retry_max: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            lease: LeaseRequest {
                consumer_id: String::new(),
                queue: String::new(),
                lease_seconds: 120,
                limit: 10,
            },
            poll_interval: Duration::from_secs(1),
            retry_min: Duration::from_secs(2),
            retry_max: Duration::from_secs(60),
        }
    }
}

/// Lease → handle → ack loop.
pub struct Worker<Q, H> {
    queue: Q,
    handler: H,
    gate: Option<Box<dyn Gate>>,
    config: WorkerConfig,
}

impl<Q, H> Worker<Q, H>
where
    Q: Queue,
    H: Handler,
{
    #[must_use]
    pub fn new(queue: Q, handler: H, config: WorkerConfig) -> Self {
        Self {
            queue,
            handler,
            gate: None,
            config,
        }
    }

    /// Install the "may take work now" predicate.
    #[must_use]
    pub fn with_gate(mut self, gate: impl Gate + 'static) -> Self {
        self.gate = Some(Box::new(gate));
        self
    }

    /// Run until `cancel` is cancelled. It never returns an error: a queue
    /// failure is retried with backoff, matching the Go executors.
    pub async fn run(&self, cancel: CancellationToken) {
        let mut retry = backoff(self.config.retry_min, self.config.retry_max);
        loop {
            if cancel.is_cancelled() {
                return;
            }

            if let Some(gate) = &self.gate {
                let ready = tokio::select! {
                    ready = gate.ready() => ready,
                    () = cancel.cancelled() => return,
                };
                if !ready {
                    if !wait(self.config.poll_interval, &cancel).await {
                        return;
                    }
                    continue;
                }
            }

            let events = match tokio::select! {
                result = self.queue.lease(self.config.lease.clone()) => result,
                () = cancel.cancelled() => return,
            } {
                Ok(events) => {
                    retry = backoff(self.config.retry_min, self.config.retry_max);
                    events
                }
                Err(_error) => {
                    if !wait_retry(&mut retry, self.config.retry_max, &cancel).await {
                        return;
                    }
                    continue;
                }
            };

            if events.is_empty() {
                if !wait(self.config.poll_interval, &cancel).await {
                    return;
                }
                continue;
            }

            let mut events = events;
            self.handler.order(&mut events);

            for event in events {
                if cancel.is_cancelled() {
                    return;
                }
                let outcome = tokio::select! {
                    outcome = self.handler.handle(&event) => outcome,
                    () = cancel.cancelled() => return,
                };
                let ack = match outcome {
                    Outcome::Delivered(detail) => Ack {
                        event_id: event.id,
                        consumer_id: self.config.lease.consumer_id.clone(),
                        outcome: AckOutcome::Delivered,
                        detail,
                    },
                    Outcome::Failed(detail) => Ack {
                        event_id: event.id,
                        consumer_id: self.config.lease.consumer_id.clone(),
                        outcome: AckOutcome::Failed,
                        detail,
                    },
                    Outcome::Skipped(detail) => Ack {
                        event_id: event.id,
                        consumer_id: self.config.lease.consumer_id.clone(),
                        outcome: AckOutcome::Skipped,
                        detail,
                    },
                    // No ack: the lease expires and the scheduler returns the
                    // event to PENDING.
                    Outcome::Retry => continue,
                };
                let acked = tokio::select! {
                    result = self.queue.ack(ack) => result,
                    () = cancel.cancelled() => return,
                };
                if acked.is_err() {
                    if !wait_retry(&mut retry, self.config.retry_max, &cancel).await {
                        return;
                    }
                    // The event may still be leased; do not spin on the rest
                    // of this batch.
                    break;
                }
            }
        }
    }
}

fn backoff(min: Duration, max: Duration) -> impl backon::Backoff {
    ExponentialBuilder::new()
        .with_min_delay(min)
        .with_max_delay(max)
        .with_factor(2.0)
        .with_jitter()
        .without_max_times()
        .build()
}

async fn wait_retry(
    retry: &mut impl backon::Backoff,
    max: Duration,
    cancel: &CancellationToken,
) -> bool {
    let delay = retry.next().unwrap_or(max);
    wait(delay, cancel).await
}

/// Sleep unless cancelled; returns `false` when cancelled.
async fn wait(delay: Duration, cancel: &CancellationToken) -> bool {
    if delay.is_zero() {
        return !cancel.is_cancelled();
    }
    tokio::select! {
        () = cancel.cancelled() => false,
        () = tokio::time::sleep(delay) => true,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::{
        Ack, AckOutcome, Action, Event, Gate, Handler, LeaseRequest, Outcome, Queue, QueueError,
        Source, Worker, WorkerConfig, backoff,
    };

    #[derive(Default)]
    struct FakeQueue {
        leases: Mutex<VecDeque<Result<Vec<Event>, QueueError>>>,
        acks: Mutex<Vec<Ack>>,
        lease_calls: AtomicUsize,
    }

    impl FakeQueue {
        fn with(leases: Vec<Result<Vec<Event>, QueueError>>) -> Arc<Self> {
            Arc::new(Self {
                leases: Mutex::new(leases.into_iter().collect()),
                ..Self::default()
            })
        }

        fn ack_count(&self) -> usize {
            self.acks.lock().expect("lock").len()
        }

        fn acked(&self) -> Vec<Ack> {
            self.acks.lock().expect("lock").clone()
        }

        fn lease_calls(&self) -> usize {
            self.lease_calls.load(Ordering::SeqCst)
        }
    }

    impl Queue for Arc<FakeQueue> {
        fn lease(
            &self,
            _request: LeaseRequest,
        ) -> impl std::future::Future<Output = Result<Vec<Event>, QueueError>> + Send {
            self.lease_calls.fetch_add(1, Ordering::SeqCst);
            let next = self.leases.lock().expect("lock").pop_front();
            async move { next.unwrap_or(Ok(Vec::new())) }
        }

        fn ack(
            &self,
            ack: Ack,
        ) -> impl std::future::Future<Output = Result<(), QueueError>> + Send {
            self.acks.lock().expect("lock").push(ack);
            std::future::ready(Ok(()))
        }
    }

    struct FakeHandler {
        outcomes: Mutex<VecDeque<Outcome>>,
        seen: Mutex<Vec<String>>,
    }

    impl FakeHandler {
        fn with(outcomes: Vec<Outcome>) -> Arc<Self> {
            Arc::new(Self {
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    impl Handler for Arc<FakeHandler> {
        fn handle(&self, event: &Event) -> impl Future<Output = Outcome> + Send {
            self.seen.lock().expect("lock").push(event.id.clone());
            let next = self
                .outcomes
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Outcome::Retry);
            std::future::ready(next)
        }
    }

    struct OrderingHandler {
        seen: Mutex<Vec<String>>,
    }

    impl OrderingHandler {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    impl Handler for Arc<OrderingHandler> {
        fn handle(&self, event: &Event) -> impl Future<Output = Outcome> + Send {
            self.seen.lock().expect("lock").push(event.id.clone());
            std::future::ready(Outcome::Delivered(String::new()))
        }

        fn order(&self, events: &mut Vec<Event>) {
            events.reverse();
        }
    }

    struct ClosedGate;

    impl Gate for ClosedGate {
        fn ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
            Box::pin(std::future::ready(false))
        }
    }

    struct OpenGate;

    impl Gate for OpenGate {
        fn ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
            Box::pin(std::future::ready(true))
        }
    }

    fn event(id: &str) -> Event {
        Event {
            id: id.to_owned(),
            source_id: "source-1".to_owned(),
            task_type: "test.ping".to_owned(),
            queue: "test".to_owned(),
            label: String::new(),
            payload: "{}".to_owned(),
            action: Action::Fire,
            source: Source::Task,
            attempts: 0,
            due_unix: 0,
            occurrence_unix: 0,
            lease_until_unix: 0,
        }
    }

    fn config() -> WorkerConfig {
        WorkerConfig {
            lease: LeaseRequest {
                consumer_id: "executor".to_owned(),
                queue: "test".to_owned(),
                lease_seconds: 120,
                limit: 1,
            },
            poll_interval: Duration::from_millis(5),
            retry_min: Duration::from_millis(5),
            retry_max: Duration::from_millis(20),
        }
    }

    async fn wait_for(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("condition timed out");
    }

    #[tokio::test]
    async fn delivered_outcome_acks_delivered() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Delivered("done".to_owned())]);
        let worker = Worker::new(queue.clone(), handler, config());
        let cancel = CancellationToken::new();

        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 1).await;
        cancel.cancel();
        run.await.expect("join");

        let acks = queue.acked();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].event_id, "ev_1");
        assert_eq!(acks[0].consumer_id, "executor");
        assert_eq!(acks[0].outcome, AckOutcome::Delivered);
        assert_eq!(acks[0].detail, "done");
    }

    #[tokio::test]
    async fn failed_outcome_acks_failed() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Failed("bad payload".to_owned())]);
        let worker = Worker::new(queue.clone(), handler, config());
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 1).await;
        cancel.cancel();
        run.await.expect("join");
        assert_eq!(queue.acked()[0].outcome, AckOutcome::Failed);
        assert_eq!(queue.acked()[0].detail, "bad payload");
    }

    #[tokio::test]
    async fn skipped_outcome_acks_skipped() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Skipped("stale".to_owned())]);
        let worker = Worker::new(queue.clone(), handler, config());
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 1).await;
        cancel.cancel();
        run.await.expect("join");
        assert_eq!(queue.acked()[0].outcome, AckOutcome::Skipped);
        assert_eq!(queue.acked()[0].detail, "stale");
    }

    #[tokio::test]
    async fn retry_is_not_acked_and_the_loop_keeps_leasing() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Retry]);
        let worker = Worker::new(queue.clone(), handler, config());
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.lease_calls() >= 2).await;
        cancel.cancel();
        run.await.expect("join");
        assert_eq!(queue.ack_count(), 0, "retry must not ack");
    }

    #[tokio::test]
    async fn a_closed_gate_never_leases() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Delivered(String::new())]);
        let worker = Worker::new(queue.clone(), handler, config()).with_gate(ClosedGate);
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        run.await.expect("join");
        assert_eq!(queue.lease_calls(), 0);
        assert_eq!(queue.ack_count(), 0);
    }

    #[tokio::test]
    async fn an_open_gate_leases_normally() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1")])]);
        let handler = FakeHandler::with(vec![Outcome::Delivered(String::new())]);
        let worker = Worker::new(queue.clone(), handler, config()).with_gate(OpenGate);
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 1).await;
        cancel.cancel();
        run.await.expect("join");
    }

    #[tokio::test]
    async fn the_order_hook_reorders_the_batch_before_handling() {
        let queue = FakeQueue::with(vec![Ok(vec![event("ev_1"), event("ev_2")])]);
        let handler = OrderingHandler::new();
        let worker = Worker::new(queue.clone(), handler.clone(), config());
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 2).await;
        cancel.cancel();
        run.await.expect("join");
        assert_eq!(
            *handler.seen.lock().expect("lock"),
            vec!["ev_2".to_owned(), "ev_1".to_owned()]
        );
        let acks = queue.acked();
        assert_eq!(acks[0].event_id, "ev_2");
        assert_eq!(acks[1].event_id, "ev_1");
    }

    #[tokio::test]
    async fn cancellation_stops_a_waiting_worker() {
        let queue = FakeQueue::with(Vec::new());
        let handler = FakeHandler::with(Vec::new());
        let mut config = config();
        config.poll_interval = Duration::from_secs(3600);
        let worker = Worker::new(queue.clone(), handler, config);
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.lease_calls() >= 1).await;
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), run)
            .await
            .expect("worker stopped on cancellation")
            .expect("join");
    }

    #[tokio::test]
    async fn queue_errors_back_off_and_the_worker_recovers() {
        let queue = FakeQueue::with(vec![
            Err(QueueError::other("dial")),
            Err(QueueError::other("dial")),
            Ok(vec![event("ev_1")]),
        ]);
        let handler = FakeHandler::with(vec![Outcome::Delivered(String::new())]);
        let worker = Worker::new(queue.clone(), handler, config());
        let cancel = CancellationToken::new();
        let run = tokio::spawn({
            let cancel = cancel.clone();
            async move { worker.run(cancel).await }
        });
        wait_for(|| queue.ack_count() == 1).await;
        cancel.cancel();
        run.await.expect("join");
        assert!(queue.lease_calls() >= 3);
    }

    #[test]
    fn backoff_starts_at_the_minimum_and_is_capped() {
        let mut retry = backoff(Duration::from_millis(10), Duration::from_millis(60));
        let delays: Vec<Duration> = std::iter::from_fn(|| retry.next()).take(6).collect();
        assert_eq!(delays.len(), 6);
        // Jitter adds (0, current); the base sequence is 10, 20, 40, 60, 60.
        assert!(delays[0] >= Duration::from_millis(10));
        assert!(delays[0] <= Duration::from_millis(20));
        assert!(delays[1] >= Duration::from_millis(20));
        assert!(delays[1] <= Duration::from_millis(40));
        assert!(delays[2] >= Duration::from_millis(40));
        assert!(delays[2] <= Duration::from_millis(80));
        assert!(
            delays
                .iter()
                .all(|delay| *delay <= Duration::from_millis(120)),
            "jitter must stay within the cap: {delays:?}"
        );
    }
}
