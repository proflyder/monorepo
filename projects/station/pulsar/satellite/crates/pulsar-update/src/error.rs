//! Ошибка обновления с признаком `fatal`: фатальный релиз (неверная контрольная сумма,
//! бинарь не запускается) автоматическое обновление больше не ставит, сетевая ошибка
//! заслуживает повтора. Текст ошибки видит Home Assistant, поэтому он сохраняется дословно.

/// Почему проверка или установка не удалась.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    /// Релиз непригоден: повтор не поможет, `update_attempt` остаётся маркером отката.
    #[error("{0}")]
    Fatal(String),
    /// Сеть, файловая система или отмена: следующий запуск может пройти.
    #[error("{0}")]
    Retry(String),
    /// Демон завершается, установка не доведена до подмены.
    #[error("cancelled")]
    Cancelled,
}

impl UpdateError {
    /// Фатальность для контроллера: после неё маркер `update_attempt` не стирается.
    #[must_use]
    pub fn fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }

    pub(crate) fn fatal_text(text: impl Into<String>) -> Self {
        Self::Fatal(text.into())
    }

    pub(crate) fn retry(text: impl Into<String>) -> Self {
        Self::Retry(text.into())
    }
}
