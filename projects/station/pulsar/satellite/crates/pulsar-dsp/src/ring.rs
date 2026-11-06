//! Один круговой буфер на все очереди аудио: FIFO захват → обработка, кольцо пре-ролла и
//! кольцо записи wake-событий. Ёмкость и размер чанка задаются в элементах; старые элементы
//! вытесняются, поэтому память ограничена независимо от темпа читателя.

/// Круговой буфер фиксированной ёмкости с чанками фиксированной длины.
///
/// Буфер хранит элементы в плоском `Vec` и не выделяет память после [`Ring::new`]. `head` —
/// первый (самый старый) элемент. Пока буфер не полон, `len` растёт; после — каждый новый
/// элемент вытесняет самый старый.
#[derive(Clone, Debug)]
pub struct Ring<T> {
    buf: Vec<T>,
    head: usize,
    len: usize,
    chunk: usize,
}

impl<T: Copy + Default> Ring<T> {
    /// `chunk` элементов в чанке, `capacity` элементов в буфере (не меньше одного чанка).
    /// `chunk == 0` даёт пустой буфер: `push_chunk` всегда сбрасывает, `take_chunk` — false.
    pub fn new(chunk: usize, capacity: usize) -> Self {
        let capacity = if chunk == 0 { 0 } else { capacity.max(chunk) };
        Self {
            buf: vec![T::default(); capacity],
            head: 0,
            len: 0,
            chunk,
        }
    }

    pub fn chunk(&self) -> usize {
        self.chunk
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Элементов сейчас в буфере.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Сколько целых чанков помещается в буфер.
    pub fn chunks(&self) -> usize {
        self.buf.len().checked_div(self.chunk).unwrap_or(0)
    }

    /// Сколько целых чанков сейчас лежит.
    pub fn filled_chunks(&self) -> usize {
        self.len.checked_div(self.chunk).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Дописывает элементы, вытесняя самые старые; Some(n) — сколько элементов вытеснено.
    pub fn push(&mut self, items: &[T]) -> Option<usize> {
        if self.buf.is_empty() || items.is_empty() {
            return None;
        }
        let cap = self.buf.len();
        let dropped = (self.len + items.len()).saturating_sub(cap);
        for &v in items {
            let at = (self.head + self.len) % cap;
            self.buf[at] = v;
            if self.len < cap {
                self.len += 1;
            } else {
                self.head = (self.head + 1) % cap;
            }
        }
        (dropped > 0).then_some(dropped)
    }

    /// Дописывает ровно один чанк. Чанк другой длины рвёт временную шкалу: буфер очищается, и
    /// ничего не записывается (false).
    pub fn push_chunk(&mut self, chunk: &[T]) -> bool {
        if self.chunk == 0 || chunk.len() != self.chunk || self.buf.is_empty() {
            self.clear();
            return false;
        }
        self.push(chunk);
        true
    }

    pub fn has_chunk(&self) -> bool {
        self.len >= self.chunk && self.chunk > 0
    }

    /// Забирает ровно один чанк с начала в `out` (его длина); false, если чанка нет или размер
    /// `out` не совпадает.
    pub fn take_chunk(&mut self, out: &mut [T]) -> bool {
        if self.chunk == 0 || out.len() != self.chunk || !self.has_chunk() {
            return false;
        }
        self.read_at(0, out);
        self.head = (self.head + self.chunk) % self.buf.len();
        self.len -= self.chunk;
        true
    }

    /// После ошибки захвата: неполный чанк в хвосте содержит разрыв. Целые чанки остаются.
    pub fn drop_partial(&mut self) {
        if self.chunk > 0 {
            self.len -= self.len % self.chunk;
        }
    }

    /// Последние `n` чанков (не больше, чем лежит), от старых к новым.
    pub fn tail(&self, n: usize) -> Vec<T> {
        let filled = self.filled_chunks();
        let n = n.min(filled);
        let mut out = Vec::with_capacity(n * self.chunk);
        for k in (filled - n)..filled {
            let end = out.len() + self.chunk;
            out.resize(end, T::default());
            self.read_at(k * self.chunk, &mut out[end - self.chunk..]);
        }
        out
    }

    /// Копирует `len` элементов с логической позиции `at` (по модулю буфера).
    fn read_at(&self, at: usize, out: &mut [T]) {
        let cap = self.buf.len();
        let start = (self.head + at) % cap;
        let len = out.len();
        let first = len.min(cap - start);
        out[..first].copy_from_slice(&self.buf[start..start + first]);
        out[first..].copy_from_slice(&self.buf[..len - first]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIFO захвата: 2 канала, чанк 3 кадра (6 элементов), не больше 10 кадров (20 элементов).
    #[test]
    fn takes_whole_chunks_in_order() {
        let mut f = Ring::new(6, 20);
        let mut out = [0i16; 6];
        assert!(!f.take_chunk(&mut out));
        assert_eq!(f.push(&[1, 1, 2, 2]), None);
        assert!(!f.has_chunk());
        assert_eq!(f.push(&[3, 3, 4, 4]), None);
        assert_eq!(f.len(), 8);
        assert_eq!(f.filled_chunks(), 1);
        assert!(f.take_chunk(&mut out));
        assert_eq!(out, [1, 1, 2, 2, 3, 3]);
        assert_eq!(f.len(), 2);
        assert_eq!(f.push(&[5, 5, 6, 6]), None);
        // Неверный размер выхода: ничего не забрано.
        assert!(!f.take_chunk(&mut [0; 4]));
        assert!(f.take_chunk(&mut out));
        assert_eq!(out, [4, 4, 5, 5, 6, 6]);
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn drops_the_oldest_past_the_capacity() {
        let mut f = Ring::new(6, 20);
        let block: Vec<i16> = (0..16).collect();
        assert_eq!(f.push(&block), None);
        // 8 + 4 кадра: 2 кадра сверх предела, самые старые уходят (12 элементов).
        let block: Vec<i16> = (16..24).collect();
        assert_eq!(f.push(&block), Some(4));
        assert_eq!(f.len(), 20);
        let mut out = [0i16; 6];
        assert!(f.take_chunk(&mut out));
        assert_eq!(out, [4, 5, 6, 7, 8, 9]);
        // Ровно на пределе: ничего не выброшено.
        let mut f = Ring::new(6, 20);
        assert_eq!(f.push(&[0; 20]), None);
    }

    #[test]
    fn drops_the_partial_chunk_after_an_error() {
        let mut f = Ring::new(4, 100);
        f.push(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        f.drop_partial();
        assert_eq!(f.len(), 8);
        f.drop_partial();
        assert_eq!(f.len(), 8);
        let mut out = [0i16; 4];
        assert!(f.take_chunk(&mut out));
        assert!(f.take_chunk(&mut out));
        assert_eq!(out, [5, 6, 7, 8]);
        f.push(&[11, 12]);
        f.drop_partial();
        assert_eq!(f.len(), 0);
    }

    /// Кольцо пре-ролла: чанки по 2 элемента, 3 чанка.
    #[test]
    fn keeps_the_last_chunks_oldest_first() {
        let mut r = Ring::new(2, 6);
        let chunk = [5i16; 2];
        assert!(r.tail(2).is_empty());
        assert!(r.push_chunk(&chunk));
        // Просят больше, чем лежит: сколько есть.
        assert_eq!(r.tail(3), [5, 5]);
        for k in 2..=5i16 {
            r.push_chunk(&[k, k]);
        }
        // Обернулись дважды: остались чанки 3, 4, 5.
        assert_eq!(r.tail(3), [3, 3, 4, 4, 5, 5]);
        assert_eq!(r.tail(2), [4, 4, 5, 5]);
        assert_eq!(r.tail(9), [3, 3, 4, 4, 5, 5]);
        assert!(r.tail(0).is_empty());
        // Чанк другой длины рвёт временную шкалу: ничего старого не выживает.
        assert!(!r.push_chunk(&[0; 3]));
        assert!(r.tail(3).is_empty());
        r.push_chunk(&[6, 6]);
        assert_eq!(r.tail(3), [6, 6]);
    }

    #[test]
    fn wrapping_chunks_are_read_correctly() {
        // Чанк 3, ёмкость 2 чанка; после вытеснения чанк пересекает конец буфера.
        let mut r = Ring::new(3, 6);
        for k in 0..4i16 {
            assert!(r.push_chunk(&[k; 3]));
        }
        assert_eq!(r.tail(2), [2, 2, 2, 3, 3, 3]);
        let mut out = [0i16; 3];
        assert!(r.take_chunk(&mut out));
        assert_eq!(out, [2, 2, 2]);
    }
}
