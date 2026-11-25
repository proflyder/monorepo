use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::ErrorKey;

pub const MAX_VOICE_BYTES: usize = 4 << 20;
pub const MAX_IMAGE_BYTES: usize = 6 << 20;
pub const MAX_FILE_BYTES: usize = 256 << 10;
pub const MAX_UPLOAD_BYTES: usize = MAX_IMAGE_BYTES + (64 << 10);
pub const VOICE_RATE: u32 = 16_000;
pub const MAX_NAME_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Voice,
    Image,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub kind: AttachmentKind,
    pub mime: String,
    pub size: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm {
    pub samples: Vec<u8>,
    pub rate: u32,
}

pub struct MediaStore {
    dir: PathBuf,
}

impl MediaStore {
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    pub fn save(
        &self,
        id: &str,
        mime: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Attachment, ErrorKey> {
        let attachment = classify(id, mime, name, bytes)?;
        let meta = serde_json::to_vec(&attachment).map_err(|_| ErrorKey::ChatStorageFailed)?;
        std::fs::write(self.blob(id), bytes).map_err(|_| ErrorKey::ChatStorageFailed)?;
        std::fs::write(self.meta(id), meta).map_err(|_| ErrorKey::ChatStorageFailed)?;
        Ok(attachment)
    }

    pub fn get(&self, id: &str) -> Result<Attachment, ErrorKey> {
        valid_id(id)?;
        let raw = std::fs::read(self.meta(id)).map_err(|_| ErrorKey::ChatAttachmentNotFound)?;
        serde_json::from_slice(&raw).map_err(|_| ErrorKey::ChatAttachmentNotFound)
    }

    pub fn bytes(&self, id: &str) -> Result<Vec<u8>, ErrorKey> {
        valid_id(id)?;
        std::fs::read(self.blob(id)).map_err(|_| ErrorKey::ChatAttachmentNotFound)
    }

    pub fn remove(&self, id: &str) {
        if valid_id(id).is_err() {
            return;
        }
        let _blob = std::fs::remove_file(self.blob(id));
        let _meta = std::fs::remove_file(self.meta(id));
    }

    fn blob(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.bin"))
    }

    fn meta(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

fn valid_id(id: &str) -> Result<(), ErrorKey> {
    let valid = id.starts_with("f_")
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(()).ok_or(ErrorKey::ChatAttachmentNotFound)
}

pub fn classify(id: &str, mime: &str, name: &str, bytes: &[u8]) -> Result<Attachment, ErrorKey> {
    let mime = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    let (kind, limit, duration_ms) = match mime.as_str() {
        "audio/wav" | "audio/x-wav" | "audio/wave" => {
            let pcm = parse_wav(bytes)?;
            (AttachmentKind::Voice, MAX_VOICE_BYTES, Some(duration(&pcm)))
        }
        "image/jpeg" | "image/png" | "image/webp" => {
            image_magic(&mime, bytes)?;
            (AttachmentKind::Image, MAX_IMAGE_BYTES, None)
        }
        "text/plain" | "text/markdown" | "text/csv" | "application/json" | "text/x-log" => {
            std::str::from_utf8(bytes).map_err(|_| ErrorKey::ChatAttachmentUnsupported)?;
            (AttachmentKind::File, MAX_FILE_BYTES, None)
        }
        _ => return Err(ErrorKey::ChatAttachmentUnsupported),
    };
    if bytes.is_empty() || bytes.len() > limit {
        return Err(ErrorKey::ChatAttachmentTooLarge);
    }
    Ok(Attachment {
        id: id.to_owned(),
        kind,
        mime,
        size: bytes.len(),
        name,
        duration_ms,
    })
}

fn image_magic(mime: &str, bytes: &[u8]) -> Result<(), ErrorKey> {
    let ok = match mime {
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        _ => bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
    };
    ok.then_some(()).ok_or(ErrorKey::ChatAttachmentUnsupported)
}

fn duration(pcm: &Pcm) -> u64 {
    let samples = u64::try_from(pcm.samples.len() / 2).unwrap_or(u64::MAX);
    samples.saturating_mul(1000) / u64::from(pcm.rate.max(1))
}

pub fn parse_wav(bytes: &[u8]) -> Result<Pcm, ErrorKey> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(ErrorKey::ChatVoiceFormat);
    }
    let mut offset = 12;
    let mut format: Option<(u16, u16, u32, u16)> = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = le_u32(&bytes[offset + 4..offset + 8]) as usize;
        let body = offset + 8;
        let end = body.saturating_add(size).min(bytes.len());
        if id == b"fmt " && end >= body + 16 {
            format = Some((
                le_u16(&bytes[body..body + 2]),
                le_u16(&bytes[body + 2..body + 4]),
                le_u32(&bytes[body + 4..body + 8]),
                le_u16(&bytes[body + 14..body + 16]),
            ));
        }
        if id == b"data" {
            return pcm_from(format, &bytes[body..end]);
        }
        offset = body.saturating_add(size + (size & 1));
    }
    Err(ErrorKey::ChatVoiceFormat)
}

fn pcm_from(format: Option<(u16, u16, u32, u16)>, data: &[u8]) -> Result<Pcm, ErrorKey> {
    match format {
        Some((1, 1, VOICE_RATE, 16)) if !data.is_empty() => Ok(Pcm {
            samples: data.to_vec(),
            rate: VOICE_RATE,
        }),
        _ => Err(ErrorKey::ChatVoiceFormat),
    }
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[must_use]
pub fn wav(samples: &[u8], rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(samples.len() + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(samples);
    out
}
