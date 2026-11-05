//! Runtime plumbing shared by first-party daemons: a thin [`Supervisor`] over
//! [`CancellationToken`] and [`TaskTracker`], plus a Unix shutdown signal.
//!
//! The supervisor keeps a name per task so shutdown can report which actors
//! refused to stop. It does not impose a runtime flavour: [`Supervisor::spawn`]
//! takes `Send` futures for multi-threaded servers, [`Supervisor::spawn_local`]
//! takes `!Send` ones inside a [`tokio::task::LocalSet`] (the Pulsar daemon).

use std::future::Future;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// One tracked task: its supervisor-assigned name and the join handle used to
/// report unfinished tasks at shutdown.
#[derive(Debug)]
struct Named {
    name: &'static str,
    handle: JoinHandle<()>,
}

/// Long-lived task registry with one cancellation token.
#[derive(Clone, Debug, Default)]
pub struct Supervisor {
    token: CancellationToken,
    tracker: TaskTracker,
    /// Shared by clones; tasks are registered by every clone on the same tracker.
    names: std::sync::Arc<Mutex<Vec<Named>>>,
}

impl Supervisor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Spawn a `Send` task (the server flavour) and track it for [`Self::shutdown`].
    pub fn spawn<F>(&self, name: &'static str, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.register(name, self.tracker.spawn(future));
    }

    /// Spawn a `!Send` task on the current `LocalSet` and track it for shutdown.
    ///
    /// Must be called from inside a [`tokio::task::LocalSet`]; that is how the
    /// single-threaded station runtime runs every actor.
    pub fn spawn_local<F>(&self, name: &'static str, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.register(name, self.tracker.spawn_local(future));
    }

    fn register(&self, name: &'static str, handle: JoinHandle<()>) {
        let mut names = self.names.lock().unwrap_or_else(PoisonError::into_inner);
        // Servers spawn one task per connection: finished handles must not accumulate.
        names.retain(|named| !named.handle.is_finished());
        names.push(Named { name, handle });
    }

    /// Cancel every task and wait up to `timeout` for all of them to return.
    ///
    /// Returns the names of tasks that were still running when the timeout
    /// expired (empty when every task stopped). Panics are reported by the
    /// process's panic hook, so the registry only keeps handles for names.
    pub async fn shutdown(&self, timeout: Duration) -> Vec<&'static str> {
        self.token.cancel();
        self.tracker.close();
        if tokio::time::timeout(timeout, self.tracker.wait())
            .await
            .is_err()
        {
            return self.unfinished();
        }
        Vec::new()
    }

    /// Names of tasks that have not finished yet.
    fn unfinished(&self) -> Vec<&'static str> {
        let mut names = self.names.lock().unwrap_or_else(PoisonError::into_inner);
        names.retain(|named| !named.handle.is_finished());
        names.iter().map(|named| named.name).collect()
    }
}

/// Resolve on `SIGTERM` or `SIGINT`.
#[cfg(unix)]
pub async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::error!(%error, "cannot install SIGTERM handler");
            return;
        }
    };
    let mut interrupt = match signal(SignalKind::interrupt()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::error!(%error, "cannot install SIGINT handler");
            return;
        }
    };
    tokio::select! {
        _ = terminate.recv() => {}
        _ = interrupt.recv() => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn shutdown_cancels_and_waits_for_tasks() {
        let supervisor = Supervisor::new();
        let finished = Arc::new(AtomicBool::new(false));
        let task_token = supervisor.token();
        let task_finished = finished.clone();
        supervisor.spawn("worker", async move {
            task_token.cancelled().await;
            task_finished.store(true, Ordering::SeqCst);
        });
        tokio::task::yield_now().await;
        assert!(!finished.load(Ordering::SeqCst));
        let stuck = tokio::time::timeout(
            Duration::from_secs(1),
            supervisor.shutdown(Duration::from_secs(1)),
        )
        .await
        .expect("shutdown must not hang");
        assert!(stuck.is_empty());
        assert!(finished.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn token_cancels_independently_of_the_tracker() {
        let supervisor = Supervisor::new();
        let waiter = supervisor.token();
        let observer = tokio::spawn(async move { waiter.cancelled().await });
        supervisor.shutdown(Duration::from_secs(1)).await;
        tokio::time::timeout(Duration::from_secs(1), observer)
            .await
            .expect("observer should finish")
            .expect("observer task");
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_reports_tasks_that_never_stop() {
        let supervisor = Supervisor::new();
        supervisor.spawn("stuck", std::future::pending());
        tokio::task::yield_now().await;
        let names = supervisor.shutdown(Duration::from_secs(1)).await;
        assert_eq!(names, ["stuck"]);
    }

    #[tokio::test(start_paused = true)]
    async fn finished_tasks_are_pruned_from_the_registry() {
        let supervisor = Supervisor::new();
        supervisor.spawn("done", async {});
        tokio::task::yield_now().await;
        supervisor.spawn("pending", std::future::pending());
        let names = supervisor.names.lock().unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].name, "pending");
    }

    #[tokio::test(start_paused = true)]
    async fn local_tasks_run_on_the_local_set() {
        let supervisor = Supervisor::new();
        let local = tokio::task::LocalSet::new();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        local
            .run_until(async {
                supervisor.spawn_local("local", async move {
                    // `Rc` makes this future `!Send`, which only `spawn_local` accepts.
                    let _marker = std::rc::Rc::new(());
                    flag.store(true, Ordering::SeqCst);
                });
                tokio::task::yield_now().await;
                assert!(supervisor.shutdown(Duration::from_secs(1)).await.is_empty());
            })
            .await;
        assert!(done.load(Ordering::SeqCst));
    }
}
