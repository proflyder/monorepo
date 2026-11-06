//! The subset of Server-Sent Events the voice proxy needs: an incremental
//! reader that joins multiline data, tolerates CRLF and forwards comment lines
//! as keepalives.
//!
//! The reader accepts both `\n` and `\r\n`; the protocol itself only requires
//! one line ending.

use std::collections::VecDeque;

/// One decoded SSE record.
///
/// `name` is the `event:` field (default `message`). A comment line becomes
/// [`KEEPALIVE`] with empty data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub data: String,
}

/// The pseudo event name for a comment line.
pub const KEEPALIVE: &str = "keepalive";

/// A record that is not valid UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sse: data is not valid utf-8")]
pub struct InvalidUtf8;

/// Incremental SSE decoder.
#[derive(Debug)]
pub struct Reader {
    buffer: Vec<u8>,
    event: String,
    data: Vec<String>,
    pending: VecDeque<Event>,
    finished: bool,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            event: "message".to_owned(),
            data: Vec::new(),
            pending: VecDeque::new(),
            finished: false,
        }
    }

    /// Feeds one network chunk.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidUtf8`] when a complete line is not valid UTF-8.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), InvalidUtf8> {
        self.buffer.extend_from_slice(chunk);
        self.consume(false)
    }

    /// Declares the stream finished, flushing a final record that has no
    /// trailing blank line.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidUtf8`] when a complete line is not valid UTF-8.
    pub fn finish(&mut self) -> Result<(), InvalidUtf8> {
        self.finished = true;
        self.consume(true)
    }

    /// Pops the next decoded record.
    pub fn next_event(&mut self) -> Option<Event> {
        self.pending.pop_front()
    }

    /// Whether the reader has produced every record it can.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.pending.is_empty() && self.finished && self.buffer.is_empty()
    }

    fn consume(&mut self, final_chunk: bool) -> Result<(), InvalidUtf8> {
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=index).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.line(&line)?;
        }
        if final_chunk && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.line(&line)?;
        }
        if final_chunk {
            self.dispatch();
        }
        Ok(())
    }

    fn line(&mut self, raw: &[u8]) -> Result<(), InvalidUtf8> {
        let line = std::str::from_utf8(raw).map_err(|_| InvalidUtf8)?;
        if line.is_empty() {
            self.dispatch();
        } else if let Some(value) = line.strip_prefix("event:") {
            self.event.clear();
            self.event.push_str(value.trim());
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data
                .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        } else if line.starts_with(':') {
            self.pending.push_back(Event {
                name: KEEPALIVE.to_owned(),
                data: String::new(),
            });
        }
        Ok(())
    }

    fn dispatch(&mut self) {
        if self.data.is_empty() {
            self.event.clear();
            self.event.push_str("message");
            return;
        }
        self.pending.push_back(Event {
            name: std::mem::replace(&mut self.event, "message".to_owned()),
            data: self.data.join("\n"),
        });
        self.data.clear();
    }
}

/// One SSE data frame, terminated by a blank line.
#[must_use]
pub fn data_frame(payload: &str) -> String {
    format!("data: {payload}\n\n")
}

/// The keepalive comment frame.
pub const KEEPALIVE_FRAME: &str = ": keepalive\n\n";

#[cfg(test)]
mod tests {
    use super::{Event, KEEPALIVE, Reader, data_frame};

    fn read_all(chunks: &[&[u8]]) -> Vec<Event> {
        let mut reader = Reader::new();
        for chunk in chunks {
            reader.push(chunk).expect("push");
        }
        reader.finish().expect("finish");
        std::iter::from_fn(|| reader.next_event()).collect()
    }

    #[test]
    fn joins_multiline_data_and_defaults_the_event_name() {
        let events = read_all(&[b"data: a\ndata: b\n\ndata: c\n\n"]);
        assert_eq!(
            events,
            vec![
                Event {
                    name: "message".to_owned(),
                    data: "a\nb".to_owned()
                },
                Event {
                    name: "message".to_owned(),
                    data: "c".to_owned()
                },
            ]
        );
    }

    #[test]
    fn accepts_crlf_and_an_unnamed_final_record() {
        let events = read_all(&[b"event: x\r\ndata: y\r\n\r\ndata: tail"]);
        assert_eq!(
            events,
            vec![
                Event {
                    name: "x".to_owned(),
                    data: "y".to_owned()
                },
                Event {
                    name: "message".to_owned(),
                    data: "tail".to_owned()
                },
            ]
        );
    }

    #[test]
    fn comment_lines_become_keepalives() {
        let events = read_all(&[b": ping\n\ndata: x\n\n"]);
        assert_eq!(events[0].name, KEEPALIVE);
        assert_eq!(events[1].data, "x");
    }

    #[test]
    fn splitting_anywhere_yields_the_same_events() {
        let text: &[u8] =
            b"event: hermes.tool.progress\ndata: {\"tool\":\"x\"}\n\ndata: [DONE]\n\n";
        let expected = read_all(&[text]);
        for split in 0..=text.len() {
            let events = read_all(&[&text[..split], &text[split..]]);
            assert_eq!(events, expected, "split {split}");
        }
    }

    #[test]
    fn invalid_utf8_is_an_error() {
        let mut reader = Reader::new();
        assert!(reader.push(b"data: \xff\n\n").is_err());
    }

    #[test]
    fn data_frames_are_one_frame_per_call() {
        assert_eq!(data_frame("[DONE]"), "data: [DONE]\n\n");
    }
}
