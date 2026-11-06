//! Протокольное состояние одного клиента: фрейминг, Noise handshake и data-кадры. Чистое:
//! вызывающий подаёт байты и записывает возвращённые байты наружу.

use crate::frame::{self, EncryptError, HandshakeError, Noise, NoiseFrame, Plain, Psk};

pub use crate::frame::EncryptError as EncodeError;

/// Конфигурация одной сессии; `allow_plaintext` = false, когда действует ключ шифрования.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub name: String,
    pub mac: String,
    pub psk: Psk,
    pub allow_plaintext: bool,
    /// Файл ключа испорчен, и случайный ключ получить не удалось: любой Noise handshake
    /// отвергается как неверный ключ, чтобы станция не приняла клиента с известным ключом.
    pub reject_all: bool,
}

impl SessionConfig {
    pub fn new(
        name: impl Into<String>,
        mac: impl Into<String>,
        psk: Psk,
        allow_plaintext: bool,
    ) -> Self {
        Self {
            name: name.into(),
            mac: mac.into(),
            psk,
            allow_plaintext,
            reject_all: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Proto {
    Unknown,
    Plain,
    Noise,
}

/// Декодированный protobuf-payload; id — это `option (id)` сообщения.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded {
    pub msg_type: u32,
    pub payload: Vec<u8>,
}

/// Что вызывающий должен сделать после обработки сообщений [`Feed`].
#[derive(Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// Кадр отказа Noise уже в выходном буфере: сбросить его, затем закрыть клиента.
    Reject(&'static str),
    /// Закрыть клиента (причина — для лога).
    Close(&'static str),
}

/// Результат [`Session::feed`]: сколько байт разобрано, декодированные сообщения и необязательное
/// терминальное действие. `consumed < input.len()` — остаток является неполным кадром.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Feed {
    pub consumed: usize,
    pub messages: Vec<Decoded>,
    pub event: Option<SessionEvent>,
}

pub struct Session {
    cfg: SessionConfig,
    proto: Proto,
    noise: Noise,
    /// Открытый текст Noise-кадра, который сейчас расшифровывается.
    rx: Vec<u8>,
}

impl Session {
    pub fn new(cfg: SessionConfig) -> Self {
        Self {
            cfg,
            proto: Proto::Unknown,
            noise: Noise::ClientHello,
            rx: Vec::new(),
        }
    }

    pub fn proto(&self) -> &Proto {
        &self.proto
    }

    pub fn handshake_done(&self) -> bool {
        self.proto == Proto::Noise && !self.noise.in_handshake()
    }

    /// Разбирает все целые кадры в `input` и дописывает протокольный вывод (server hello,
    /// ответ handshake, отказ) в `out`.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Feed {
        let mut feed = Feed::default();
        let mut pos = 0;
        while pos < input.len() {
            if self.proto == Proto::Unknown {
                match input[pos] {
                    0x01 => self.proto = Proto::Noise,
                    0x00 if self.cfg.allow_plaintext => self.proto = Proto::Plain,
                    0x00 => {
                        self.reject(out, "Bad indicator byte");
                        feed.event = Some(SessionEvent::Reject("Bad indicator byte"));
                        break;
                    }
                    _ => {
                        feed.event = Some(SessionEvent::Close("bad frame preamble"));
                        break;
                    }
                }
            }
            let used = match self.proto {
                Proto::Plain => self.plain_frame(&input[pos..], &mut feed),
                Proto::Noise => self.noise_frame(&input[pos..], out, &mut feed),
                Proto::Unknown => unreachable!("assigned above"),
            };
            match used {
                Some(used) => pos += used,
                None => break,
            }
        }
        feed.consumed = pos;
        feed
    }

    /// Дописывает одно сообщение во фрейминге сессии. До завершения Noise handshake отправлять
    /// нечего: [`EncodeError::NotReady`], в буфер ничего не пишется.
    pub fn encode(
        &mut self,
        msg_type: u32,
        payload: &[u8],
        out: &mut Vec<u8>,
        scratch: &mut Vec<u8>,
    ) -> Result<(), EncodeError> {
        match self.proto {
            Proto::Plain => {
                frame::append_plain(out, msg_type, payload);
                Ok(())
            }
            Proto::Noise => self.noise.append_encrypted(out, msg_type, payload, scratch),
            Proto::Unknown => Err(EncryptError::NotReady),
        }
    }

    fn plain_frame(&mut self, p: &[u8], feed: &mut Feed) -> Option<usize> {
        match frame::parse_plain(p) {
            Plain::Incomplete => None,
            Plain::BadPreamble => {
                feed.event = Some(SessionEvent::Close("bad frame preamble"));
                None
            }
            Plain::MalformedHeader => {
                feed.event = Some(SessionEvent::Close("malformed frame header"));
                None
            }
            Plain::TooLarge => {
                feed.event = Some(SessionEvent::Close("frame too large"));
                None
            }
            Plain::Frame {
                msg_type,
                msg,
                used,
            } => {
                feed.messages.push(Decoded {
                    msg_type,
                    payload: msg.to_vec(),
                });
                Some(used)
            }
        }
    }

    fn noise_frame(&mut self, p: &[u8], out: &mut Vec<u8>, feed: &mut Feed) -> Option<usize> {
        let in_handshake = self.noise.in_handshake();
        let client_hello = self.noise.is_client_hello();
        match frame::parse_noise(p, in_handshake) {
            NoiseFrame::Incomplete => None,
            NoiseFrame::BadIndicator => {
                self.reject(out, "Bad indicator byte");
                feed.event = Some(SessionEvent::Reject("Bad indicator byte"));
                None
            }
            NoiseFrame::BadHandshakeLen => {
                self.reject(out, "Bad handshake packet len");
                feed.event = Some(SessionEvent::Reject("Bad handshake packet len"));
                None
            }
            NoiseFrame::Frame { body, used } => {
                if client_hello {
                    if self.cfg.reject_all {
                        // Как при неверном ключе: server hello, затем MAC failure.
                        frame::append_noise_raw(
                            out,
                            &[
                                b"\x01",
                                self.cfg.name.as_bytes(),
                                b"\0",
                                self.cfg.mac.as_bytes(),
                                b"\0",
                            ],
                        );
                        self.reject(out, "Handshake MAC failure");
                        feed.event = Some(SessionEvent::Reject("Handshake MAC failure"));
                        return Some(used);
                    }
                    if let Err(e) =
                        self.noise
                            .start(body, &self.cfg.psk, &self.cfg.name, &self.cfg.mac, out)
                    {
                        let _ = e;
                        feed.event = Some(SessionEvent::Close("noise setup failed"));
                        return Some(used);
                    }
                } else if in_handshake {
                    match self.noise.handshake(body, out) {
                        Ok(()) => {}
                        Err(HandshakeError::Reject(reason)) => {
                            self.reject(out, reason);
                            feed.event = Some(SessionEvent::Reject(reason));
                        }
                        Err(HandshakeError::ReadFailed { mac }) => {
                            let reason = if mac {
                                "Handshake MAC failure"
                            } else {
                                "Handshake error"
                            };
                            self.reject(out, reason);
                            feed.event = Some(SessionEvent::Reject(reason));
                        }
                        Err(HandshakeError::SplitFailed) => {
                            feed.event = Some(SessionEvent::Close("noise split failed"));
                        }
                    }
                } else {
                    let mut buf = std::mem::take(&mut self.rx);
                    match self.noise.decrypt(body, &mut buf) {
                        Ok((msg_type, range)) => feed.messages.push(Decoded {
                            msg_type,
                            payload: buf[range].to_vec(),
                        }),
                        Err(why) => feed.event = Some(SessionEvent::Close(why)),
                    }
                    self.rx = buf;
                }
                Some(used)
            }
        }
    }

    fn reject(&mut self, out: &mut Vec<u8>, reason: &str) {
        frame::append_noise_raw(out, &[b"\x01", reason.as_bytes()]);
    }
}
