//! Minimal actor primitive shared by first-party daemons.
//!
//! An actor owns its state inside one task and receives commands through an
//! unbounded channel. [`Handle`] is the non-blocking, `Clone` side: every method
//! takes `&self` and never awaits. The actor ends when every handle is dropped
//! (the command channel closes).
//!
//! The handle itself is generic over `Send`; the actor task is spawned by the
//! caller through [`ricardo_runtime::Supervisor::spawn`] or
//! [`ricardo_runtime::Supervisor::spawn_local`].

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Non-blocking command side of an actor.
pub struct Handle<C> {
    commands: UnboundedSender<C>,
}

impl<C> Clone for Handle<C> {
    fn clone(&self) -> Self {
        Self {
            commands: self.commands.clone(),
        }
    }
}

impl<C> Handle<C> {
    /// Queues `command`; false when the actor has already ended.
    pub fn send(&self, command: C) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Whether the actor side is gone (no more commands will be handled).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }
}

/// Creates the command channel of one actor: the handle stays with the owner,
/// the receiver moves into the actor task.
pub fn channel<C>() -> (Handle<C>, UnboundedReceiver<C>) {
    let (commands, commands_rx) = unbounded_channel();
    (Handle { commands }, commands_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn channel_delivers_until_the_last_handle_drops() {
        let (handle, mut commands) = channel::<u8>();
        let clone = handle.clone();
        assert!(handle.send(7));
        assert_eq!(commands.recv().await, Some(7));
        drop(handle);
        assert!(!clone.is_closed());
        drop(clone);
        assert_eq!(commands.recv().await, None);
    }
}
