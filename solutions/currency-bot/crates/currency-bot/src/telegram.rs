//! Telegram adapter over the shared `home-telegram` client.

use std::time::Duration;

use home_secret::{Secret, Token};
use home_telegram::Bot;

use crate::pipeline::{BoxFuture, MessageSender};

/// The production Bot API timeout.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Delivers pipeline messages and registers the webhook.
#[derive(Debug, Clone)]
pub struct TelegramSender {
    bot: Bot,
}

impl TelegramSender {
    /// Build the client for one bot token.
    pub fn new(api_url: &str, token: Token) -> Result<Self, String> {
        let bot = Bot::new(api_url, token, TIMEOUT).map_err(|error| error.to_string())?;
        Ok(Self { bot })
    }

    /// Register the webhook with the secret token and `allowed_updates`.
    pub async fn set_webhook(
        &self,
        url: &str,
        secret: &Secret,
        allowed_updates: &[&str],
    ) -> Result<(), String> {
        self.bot
            .set_webhook(url, secret, allowed_updates)
            .await
            .map_err(|error| error.to_string())
    }
}

impl MessageSender for TelegramSender {
    fn send<'a>(&'a self, chat_id: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.bot
                .send_message(chat_id, text)
                .await
                .map_err(|error| error.to_string())
        })
    }
}
