use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::TcpStream;

use super::media::Pcm;
use crate::tools::BoxFuture;

pub const CHUNK_BYTES: usize = 6400;
pub const STT_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_HEADER_BYTES: usize = 64 << 10;
pub const LANGUAGE: &str = "ru";

pub trait SpeechToText: Send + Sync {
    fn transcribe<'a>(&'a self, pcm: &'a Pcm) -> BoxFuture<'a, Result<String, String>>;
}

pub struct Wyoming {
    addresses: Vec<String>,
    timeout: Duration,
}

impl Wyoming {
    #[must_use]
    pub fn new(addresses: Vec<String>, timeout: Duration) -> Self {
        Self { addresses, timeout }
    }

    async fn first_answer(&self, pcm: &Pcm) -> Result<String, String> {
        let mut last = String::from("no speech-to-text endpoint configured");
        for address in &self.addresses {
            match tokio::time::timeout(self.timeout, transcribe_at(address, pcm)).await {
                Ok(Ok(text)) => return Ok(text),
                Ok(Err(error)) => last = format!("{address}: {error}"),
                Err(_) => last = format!("{address}: timed out"),
            }
        }
        Err(last)
    }
}

impl SpeechToText for Wyoming {
    fn transcribe<'a>(&'a self, pcm: &'a Pcm) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(self.first_answer(pcm))
    }
}

async fn transcribe_at(address: &str, pcm: &Pcm) -> Result<String, String> {
    let stream = TcpStream::connect(address)
        .await
        .map_err(|error| error.to_string())?;
    let (read, mut write) = stream.into_split();
    let format = json!({"rate": pcm.rate, "width": 2, "channels": 1});
    write_event(
        &mut write,
        "transcribe",
        &json!({"language": LANGUAGE}),
        &[],
    )
    .await?;
    write_event(&mut write, "audio-start", &format, &[]).await?;
    for chunk in pcm.samples.chunks(CHUNK_BYTES) {
        write_event(&mut write, "audio-chunk", &format, chunk).await?;
    }
    write_event(&mut write, "audio-stop", &json!({}), &[]).await?;
    let mut reader = BufReader::new(read);
    loop {
        let (kind, data) = read_event(&mut reader).await?;
        if kind == "transcript" {
            return Ok(data["text"].as_str().unwrap_or_default().trim().to_owned());
        }
        if kind == "error" {
            return Err(data["text"]
                .as_str()
                .unwrap_or("speech-to-text error")
                .to_owned());
        }
    }
}

pub async fn write_event<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    kind: &str,
    data: &Value,
    payload: &[u8],
) -> Result<(), String> {
    let data_bytes = serde_json::to_vec(data).map_err(|error| error.to_string())?;
    let mut header = json!({"type": kind, "version": "1.0.0", "data_length": data_bytes.len()});
    if !payload.is_empty() {
        header["payload_length"] = json!(payload.len());
    }
    let mut frame = serde_json::to_vec(&header).map_err(|error| error.to_string())?;
    frame.push(b'\n');
    frame.extend_from_slice(&data_bytes);
    frame.extend_from_slice(payload);
    writer
        .write_all(&frame)
        .await
        .map_err(|error| error.to_string())
}

pub async fn read_event<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<(String, Value), String> {
    let mut line = Vec::new();
    let read = (&mut *reader)
        .take(MAX_HEADER_BYTES as u64)
        .read_until(b'\n', &mut line)
        .await
        .map_err(|error| error.to_string())?;
    if read == 0 {
        return Err("speech-to-text closed the connection".to_owned());
    }
    let header: Value = serde_json::from_slice(&line).map_err(|error| error.to_string())?;
    let mut data = header.get("data").cloned().unwrap_or(Value::Null);
    let data_length = length(&header, "data_length");
    if data_length > 0 {
        let mut raw = vec![0; data_length];
        reader
            .read_exact(&mut raw)
            .await
            .map_err(|error| error.to_string())?;
        data = serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
    }
    let payload_length = length(&header, "payload_length");
    if payload_length > 0 {
        let mut skip = vec![0; payload_length];
        reader
            .read_exact(&mut skip)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok((header["type"].as_str().unwrap_or_default().to_owned(), data))
}

fn length(header: &Value, key: &str) -> usize {
    header
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
        .min(MAX_HEADER_BYTES)
}
