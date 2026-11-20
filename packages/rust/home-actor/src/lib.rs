use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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
    pub fn send(&self, command: C) -> bool {
        self.commands.send(command).is_ok()
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }
}

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
