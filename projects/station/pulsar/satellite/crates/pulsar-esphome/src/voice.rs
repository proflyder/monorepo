//! Владение голосовым пайплайном: чистые правила, без I/O.
//!
//! Владелец — самый свежий клиент после hello, всё ещё подписанный на голосового ассистента.
//! HA, переподключившаяся после обрыва Wi-Fi, сразу забирает пайплайн у мёртвого сокета; проба
//! или вторая HA, подписавшаяся и ушедшая, возвращает его предыдущему. Новый клиент при
//! заполненном лимите может вытеснить установленного, но никогда — владельца.

use tokio::time::Instant;

/// Что выбор владельца и ранг вытеснения видят о клиенте.
#[derive(Clone, Copy, Debug)]
pub struct Standing {
    pub id: u64,
    pub hello: bool,
    pub voice: bool,
    pub voice_wanted: u64,
    pub last_rx: Instant,
}

/// Владелец пайплайна.
pub fn voice_choice(clients: impl Iterator<Item = Standing>) -> Option<u64> {
    let mut want: Option<Standing> = None;
    for c in clients {
        if c.hello && c.voice_wanted != 0 && want.is_none_or(|w| c.voice_wanted > w.voice_wanted) {
            want = Some(c);
        }
    }
    want.map(|w| w.id)
}

/// Клиент, завершивший hello при заполненном лимите, заменяет установленного: сначала без
/// интереса к голосу, никогда — владельца, иначе того, от кого дольше всего не было слышно
/// (переподключающаяся HA, чей старый сокет ещё не отвалился, попадает внутрь, а старый уходит).
pub fn eviction_victim(clients: impl Iterator<Item = Standing>) -> (Option<u64>, usize) {
    let rank = |c: &Standing| {
        if c.voice {
            2
        } else if c.voice_wanted != 0 {
            1
        } else {
            0
        }
    };
    let mut victim: Option<Standing> = None;
    let mut established = 0;
    for c in clients.filter(|c| c.hello) {
        established += 1;
        if victim
            .is_none_or(|v| rank(&c) < rank(&v) || (rank(&c) == rank(&v) && c.last_rx < v.last_rx))
        {
            victim = Some(c);
        }
    }
    (victim.map(|v| v.id), established)
}

/// Состояние владения и «устаревших» команд предыдущего run.
#[derive(Debug, Default)]
pub struct VoiceOwnership {
    current: Option<u64>,
    stale: bool,
}

impl VoiceOwnership {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<u64> {
        self.current
    }

    /// Меняет владельца; возвращает `true`, если он изменился.
    pub fn set_current(&mut self, owner: Option<u64>) -> bool {
        if self.current == owner {
            return false;
        }
        self.current = owner;
        // Audio/stop, ещё стоящие в очереди от прошлого run, не должны дойти до нового
        // владельца (в C++ контроллер завершал run внутри hook, до отправки остального).
        self.stale = true;
        true
    }

    /// Новый run: команды снова относятся к текущему владельцу.
    pub fn start_run(&mut self) {
        self.stale = false;
    }

    /// Команды аудио/останова этого run уже не актуальны.
    pub fn is_stale(&self) -> bool {
        self.stale
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn stand(id: u64, hello: bool, voice: bool, wanted: u64, age_s: u64) -> Standing {
        Standing {
            id,
            hello,
            voice,
            voice_wanted: wanted,
            last_rx: Instant::now() - Duration::from_secs(age_s),
        }
    }

    #[test]
    fn owner_is_the_latest_subscriber_past_hello() {
        assert_eq!(voice_choice([].into_iter()), None);
        assert_eq!(
            voice_choice([stand(1, true, false, 0, 0), stand(2, true, false, 0, 0)].into_iter()),
            None
        );
        assert_eq!(
            voice_choice(
                [
                    stand(1, true, false, 3, 0),
                    stand(2, true, false, 5, 0),
                    stand(3, true, false, 4, 0)
                ]
                .into_iter()
            ),
            Some(2)
        );
        // Подписка до hello не считается.
        assert_eq!(
            voice_choice([stand(1, true, false, 3, 0), stand(2, false, false, 5, 0)].into_iter()),
            Some(1)
        );
    }

    #[test]
    fn eviction_prefers_the_least_useful_and_never_the_owner() {
        // Ожидающие соединения не считаются и не уходят.
        assert_eq!(
            eviction_victim([stand(1, false, false, 0, 0)].into_iter()),
            (None, 0)
        );
        // Сначала без интереса к голосу, затем дольше всех молчащие.
        assert_eq!(
            eviction_victim(
                [
                    stand(1, true, true, 4, 0),
                    stand(2, true, false, 2, 10),
                    stand(3, true, false, 0, 80),
                    stand(4, true, false, 0, 70),
                    stand(5, false, false, 0, 0),
                ]
                .into_iter()
            ),
            (Some(3), 4)
        );
        // Затем подписчики, не владеющие пайплайном; владелец — последним.
        assert_eq!(
            eviction_victim(
                [
                    stand(1, true, true, 4, 0),
                    stand(2, true, false, 2, 80),
                    stand(3, true, false, 3, 60)
                ]
                .into_iter()
            ),
            (Some(2), 3)
        );
        assert_eq!(
            eviction_victim([stand(1, true, true, 4, 0)].into_iter()),
            (Some(1), 1)
        );
        // При равенстве — более раннее соединение.
        assert_eq!(
            eviction_victim([stand(1, true, false, 0, 5), stand(2, true, false, 0, 5)].into_iter()),
            (Some(1), 2)
        );
    }

    #[test]
    fn ownership_marks_commands_stale_only_on_handover() {
        let mut v = VoiceOwnership::new();
        assert!(!v.is_stale());
        assert!(v.set_current(Some(1)));
        assert!(v.is_stale());
        v.start_run();
        assert!(!v.is_stale());
        assert!(v.set_current(Some(2)));
        assert!(v.is_stale());
        v.start_run();
        assert!(!v.set_current(Some(2)));
        assert!(!v.is_stale());
        assert!(v.set_current(None));
        assert!(v.is_stale());
    }
}
