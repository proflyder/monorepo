//! User-visible fixed texts.
//!
//! The `/help` text intentionally promises an hourly broadcast that the hourly
//! job does not send; it is compatibility text, not a bug to fix here.

/// `GET /` compatibility body.
pub const ROOT_BODY: &str = "Hello World!";

/// `/start` reply.
pub const START_MESSAGE: &str = "👋 Привет! Я бот для отслеживания курсов валют.

Доступные команды:
/trigger - Обновить курсы валют
/help - Показать справку";

/// `/help` (and `/helpme`) reply.
pub const HELP_MESSAGE: &str = "📖 Справка по командам бота:

/trigger - Принудительно обновить и получить актуальные курсы USD→KZT и RUB→KZT
/start - Показать приветственное сообщение
/help - Показать эту справку

ℹ️ Бот автоматически отправляет курсы валют каждый час.";

/// The `/trigger` failure reply. The original interpolates the unescaped error
/// into HTML; that user-visible behaviour is kept.
#[must_use]
pub fn trigger_failure(cause: &str) -> String {
    format!("❌ Не удалось обновить курсы: {cause}")
}
