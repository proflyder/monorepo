//! Runtime plumbing shared by first-party daemons: a thin [`Supervisor`] over
//! [`CancellationToken`] and [`TaskTracker`], plus a Unix shutdown signal.

use std::future::Future;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Long-lived task registry with one cancellation token.
#[derive(Clone, Debug, Default)]
pub struct Supervisor {
    token: CancellationToken,
    tracker: TaskTracker,
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

    /// Spawn a task that is awaited by [`Supervisor::shutdown`].
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tracker.spawn(future)
    }

    /// Cancel every task and wait until all of them have returned.
    pub async fn shutdown(&self) {
        self.token.cancel();
        self.tracker.close();
        self.tracker.wait().await;
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
        supervisor.spawn(async move {
            task_token.cancelled().await;
            task_finished.store(true, Ordering::SeqCst);
        });
        tokio::task::yield_now().await;
        assert!(!finished.load(Ordering::SeqCst));
        tokio::time::timeout(Duration::from_secs(1), supervisor.shutdown())
            .await
            .expect("shutdown must not hang");
        assert!(finished.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn token_cancels_independently_of_the_tracker() {
        let supervisor = Supervisor::new();
        let waiter = supervisor.token();
        let observer = tokio::spawn(async move { waiter.cancelled().await });
        supervisor.shutdown().await;
        tokio::time::timeout(Duration::from_secs(1), observer)
            .await
            .expect("observer should finish")
            .expect("observer task");
    }
}
