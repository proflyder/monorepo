use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::Purpose;

pub const MAX_WAITERS_PER_MACHINE: usize = 2;

#[derive(Debug, Default)]
struct Inner {
    counts: HashMap<String, usize>,
    queues: HashMap<(String, Purpose), Arc<Notify>>,
}

#[derive(Debug, Default)]
pub struct Waiters {
    inner: Mutex<Inner>,
}

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
