use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::Purpose;

/// At most two concurrent waits per machine, across purposes.
pub const MAX_WAITERS_PER_MACHINE: usize = 2;

#[derive(Debug, Default)]
struct Inner {
    counts: HashMap<String, usize>,
    queues: HashMap<(String, Purpose), Arc<Notify>>,
}

/// Wakes long-poll claims when a matching grant is created.
///
/// Notifications are per `(machine, purpose)`: an unlock waiter must not wake
/// for a logon grant and vice versa. The number of concurrent waits is capped
/// per machine across purposes so one machine cannot pin unbounded HTTP
/// requests.
#[derive(Debug, Default)]
pub struct Waiters {
    inner: Mutex<Inner>,
}

/// One registered wait. Dropping it releases the per-machine slot.
pub struct Waiter {
    waiters: Arc<Waiters>,
    machine: String,
    notify: Arc<Notify>,
}

impl Waiters {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a waiter, or returns `None` when the machine already has
    /// [`MAX_WAITERS_PER_MACHINE`] active waits.
    #[allow(clippy::significant_drop_tightening)]
    pub fn acquire(self: &Arc<Self>, machine: &str, purpose: Purpose) -> Option<Waiter> {
        let mut inner = self.inner.lock().expect("waiter lock");
        let count = inner.counts.get(machine).copied().unwrap_or(0);
        if count >= MAX_WAITERS_PER_MACHINE {
            return None;
        }
        inner.counts.insert(machine.to_owned(), count + 1);
        let notify = Arc::clone(
            inner
                .queues
                .entry((machine.to_owned(), purpose))
                .or_insert_with(|| Arc::new(Notify::new())),
        );
        Some(Waiter {
            waiters: Arc::clone(self),
            machine: machine.to_owned(),
            notify,
        })
    }

    /// Wakes every waiter for `(machine, purpose)` after a grant is committed.
    pub fn broadcast(&self, machine: &str, purpose: Purpose) {
        let inner = self.inner.lock().expect("waiter lock");
        if let Some(notify) = inner.queues.get(&(machine.to_owned(), purpose)) {
            notify.notify_waiters();
        }
    }

    #[cfg(test)]
    pub(crate) fn counts_for(&self, machine: &str) -> usize {
        self.inner
            .lock()
            .expect("waiter lock")
            .counts
            .get(machine)
            .copied()
            .unwrap_or(0)
    }
}

impl Waiter {
    /// A future that resolves on the next broadcast. Call
    /// [`tokio::sync::futures::Notified::enable`] before re-checking the store
    /// so a broadcast between the check and the wait cannot be lost.
    pub fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.notify.notified()
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        let mut inner = self.waiters.inner.lock().expect("waiter lock");
        if let Some(count) = inner.counts.get_mut(&self.machine) {
            *count -= 1;
            if *count == 0 {
                inner.counts.remove(&self.machine);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waiters_are_capped_per_machine_and_woken_per_purpose() {
        let waiters = Arc::new(Waiters::new());
        let first = waiters.acquire("komp", Purpose::Unlock).expect("first");
        let _second = waiters.acquire("komp", Purpose::Unlock).expect("second");
        assert!(waiters.acquire("komp", Purpose::Logon).is_none());
        assert_eq!(waiters.counts_for("komp"), 2);

        {
            let notified = first.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            // A logon broadcast must not wake an unlock waiter.
            waiters.broadcast("komp", Purpose::Logon);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), &mut notified)
                    .await
                    .is_err()
            );
            waiters.broadcast("komp", Purpose::Unlock);
            tokio::time::timeout(std::time::Duration::from_secs(1), notified)
                .await
                .expect("woken");
        }
        drop(first);
        assert_eq!(waiters.counts_for("komp"), 1);
    }
}
