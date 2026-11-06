//! DNS wire format of the responder: typed records, the codec and question answering.
//! No I/O and no runtime; the packets are byte-identical to the C++ responder's
//! (tests/golden/mdns.txt).

use std::net::Ipv4Addr;

pub const MDNS_PORT: u16 = 5353;
pub const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MAX_SERVICES: usize = 7;
const SERVICES_ENUM: &str = "_services._dns-sd._udp.local";
const T_A: u16 = 1;
const T_PTR: u16 = 12;
const T_TXT: u16 = 16;
const T_SRV: u16 = 33;
const T_ANY: u16 = 255;
/// Legacy unicast (RFC 6762 6.7): a one-shot resolver gets short TTLs and no cache-flush bit.
const LEGACY_TTL: u32 = 10;
/// Compression pointers followed per name: more is a loop or garbage.
const MAX_HOPS: u32 = 32;
/// RFC 1035 3.1: a name takes at most 255 bytes on the wire (labels, length bytes, root).
const MAX_NAME: usize = 255;

/// Record selection bitmask: bit 0 = the host's A record; service i owns the four bits from
/// 1 + 4 i (`Kind`). Seven services fit.
pub const A: u32 = 1;

#[derive(Clone, Copy)]
pub enum Kind {
    Ptr = 0,
    Srv = 1,
    Txt = 2,
    /// The service type's PTR in "_services._dns-sd._udp.local".
    Enum = 3,
}

pub fn bit(svc: usize, k: Kind) -> u32 {
    1 << (1 + 4 * svc + k as usize)
}

/// How a query was answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Source port other than 5353: straight to the sender, question and id echoed.
    Legacy,
    /// QU bit: to the sender.
    Unicast,
    Multicast,
}

impl Reply {
    /// Stable name for `tracing` diagnostics of `lan_probe`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Unicast => "unicast",
            Self::Multicast => "multicast",
        }
    }
}

pub struct Service {
    /// "_esphomelib._tcp.local"
    pub type_fqdn: String,
    /// "<instance>._esphomelib._tcp.local"
    pub instance_fqdn: String,
    pub port: u16,
    pub txt: Vec<String>,
}

impl Service {
    pub fn new(service: &str, instance: &str, port: u16, txt: Vec<String>) -> Self {
        Self {
            type_fqdn: format!("{service}.local"),
            instance_fqdn: format!("{instance}.{service}.local"),
            port,
            txt,
        }
    }
}

pub enum Dest {
    Sender,
    Group,
}

pub struct Response {
    pub dest: Dest,
    pub reply: Reply,
    pub packet: Vec<u8>,
    /// (name, qtype) of the questions this packet answers; filled only when asked for.
    pub questions: Vec<(String, u16)>,
}

/// DNS record types this responder emits.
#[derive(Clone, Copy)]
enum RType {
    A,
    Ptr,
    Txt,
    Srv,
}

impl RType {
    const fn code(self) -> u16 {
        match self {
            Self::A => 1,
            Self::Ptr => 12,
            Self::Txt => 16,
            Self::Srv => 33,
        }
    }
}

/// Typed RDATA of one record.
enum RData<'a> {
    A(Ipv4Addr),
    Ptr(&'a str),
    Srv { port: u16, target: &'a str },
    Txt(&'a [String]),
}

/// One resource record before encoding.
struct Record<'a> {
    name: &'a str,
    rtype: RType,
    /// Cache-flush bit on the class field (unique records, not in legacy replies).
    unique: bool,
    ttl: u32,
    data: RData<'a>,
}

pub struct Records {
    /// "<host>.local"
    pub host_fqdn: String,
    pub services: Vec<Service>,
    /// Advertised address; UNSPECIFIED = none: no A record, no announcements.
    pub ip: Ipv4Addr,
}

fn put16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_be_bytes());
}

fn be16(p: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([p[at], p[at + 1]])
}

/// Dotted name -> DNS labels without compression; a label is cut at 63 bytes.
pub fn put_name(o: &mut Vec<u8>, dotted: &[u8]) {
    let mut start = 0;
    while start < dotted.len() {
        let dot = dotted[start..]
            .iter()
            .position(|&c| c == b'.')
            .map_or(dotted.len(), |k| start + k);
        let len = (dot - start).min(63);
        o.push(len as u8);
        o.extend_from_slice(&dotted[start..start + len]);
        start = dot + 1;
    }
    o.push(0);
}

/// Reads the possibly compressed name at `p[*pos]` into `out`, ASCII-lowercased and dotted, and
/// moves `*pos` past it. Bytes are kept as they are: names are compared byte-wise. Names longer
/// than 255 bytes fail (the C++ parser had no limit; bounds the work per question).
pub fn read_name(p: &[u8], pos: &mut usize, out: &mut Vec<u8>) -> bool {
    out.clear();
    let mut i = *pos;
    let mut jumped = false;
    let mut hops = 0;
    // The root byte.
    let mut wire_len = 1;
    while hops < MAX_HOPS {
        let Some(&l) = p.get(i) else { return false };
        if l & 0xC0 == 0xC0 {
            let Some(&lo) = p.get(i + 1) else {
                return false;
            };
            if !jumped {
                *pos = i + 2;
            }
            jumped = true;
            i = usize::from(l & 0x3f) << 8 | usize::from(lo);
            hops += 1;
            continue;
        }
        if l == 0 {
            if !jumped {
                *pos = i + 1;
            }
            return true;
        }
        let l = usize::from(l);
        wire_len += 1 + l;
        if l > 63 || i + 1 + l > p.len() || wire_len > MAX_NAME {
            return false;
        }
        if !out.is_empty() {
            out.push(b'.');
        }
        out.extend(p[i + 1..i + 1 + l].iter().map(u8::to_ascii_lowercase));
        i += 1 + l;
    }
    false
}

impl Records {
    pub fn new(host: &str) -> Self {
        Self {
            host_fqdn: format!("{host}.local"),
            services: Vec::new(),
            ip: Ipv4Addr::UNSPECIFIED,
        }
    }

    pub fn all(&self) -> u32 {
        (0..self.services.len()).fold(A, |m, i| {
            m | bit(i, Kind::Ptr) | bit(i, Kind::Srv) | bit(i, Kind::Txt) | bit(i, Kind::Enum)
        })
    }

    /// Unsolicited announcement (or goodbye: TTL 0) of every record to the group; None when
    /// there is no address to announce.
    pub fn announcement(&self, goodbye: bool) -> Option<Vec<u8>> {
        (goodbye || !self.ip.is_unspecified())
            .then(|| self.build(0, self.all(), 0, None, goodbye, &[]))
    }

    /// Response packet. `ttl_cap` is set for legacy unicast replies: TTLs capped, no cache-flush
    /// bit, `question` echoed.
    pub fn build(
        &self,
        id: u16,
        mut answers: u32,
        mut additional: u32,
        ttl_cap: Option<u32>,
        goodbye: bool,
        question: &[u8],
    ) -> Vec<u8> {
        if self.ip.is_unspecified() {
            answers &= !A;
            additional &= !A;
        }
        additional &= !answers;
        let (answers, additional) = (
            self.select_records(answers),
            self.select_records(additional),
        );
        let mut pkt = Vec::with_capacity(12 + question.len() + 512);
        put16(&mut pkt, id);
        put16(&mut pkt, 0x8400); // response, authoritative
        put16(&mut pkt, u16::from(!question.is_empty()));
        put16(&mut pkt, answers.len() as u16);
        put16(&mut pkt, 0);
        put16(&mut pkt, additional.len() as u16);
        pkt.extend_from_slice(question);
        encode_records(&mut pkt, &answers, ttl_cap, goodbye);
        encode_records(&mut pkt, &additional, ttl_cap, goodbye);
        pkt
    }

    /// Records of `mask` in the C++ order: A, then per service PTR, SRV, TXT, enumeration PTR.
    fn select_records(&self, mask: u32) -> Vec<Record<'_>> {
        let mut out = Vec::new();
        if mask & A != 0 {
            out.push(Record {
                name: &self.host_fqdn,
                rtype: RType::A,
                unique: true,
                ttl: 120,
                data: RData::A(self.ip),
            });
        }
        for (i, s) in self.services.iter().enumerate() {
            if mask & bit(i, Kind::Ptr) != 0 {
                out.push(Record {
                    name: &s.type_fqdn,
                    rtype: RType::Ptr,
                    unique: false,
                    ttl: 4500,
                    data: RData::Ptr(&s.instance_fqdn),
                });
            }
            if mask & bit(i, Kind::Srv) != 0 {
                out.push(Record {
                    name: &s.instance_fqdn,
                    rtype: RType::Srv,
                    unique: true,
                    ttl: 120,
                    data: RData::Srv {
                        port: s.port,
                        target: &self.host_fqdn,
                    },
                });
            }
            if mask & bit(i, Kind::Txt) != 0 {
                out.push(Record {
                    name: &s.instance_fqdn,
                    rtype: RType::Txt,
                    unique: true,
                    ttl: 4500,
                    data: RData::Txt(&s.txt),
                });
            }
            if mask & bit(i, Kind::Enum) != 0 {
                out.push(Record {
                    name: SERVICES_ENUM,
                    rtype: RType::Ptr,
                    unique: false,
                    ttl: 4500,
                    data: RData::Ptr(&s.type_fqdn),
                });
            }
        }
        out
    }

    /// Responses to a datagram from port `from_port`, in send order. A legacy resolver (source
    /// port other than 5353) gets one reply per question; otherwise the questions with the QU
    /// bit are answered in one unicast packet and the rest in one multicast packet. A malformed
    /// question ends the parse: only the legacy replies made before it are sent.
    pub fn answer(&self, p: &[u8], from_port: u16, trace: bool) -> Vec<Response> {
        let mut out = Vec::new();
        if p.len() < 12 {
            return out;
        }
        let id = be16(p, 0);
        let flags = be16(p, 2);
        let qd = be16(p, 4);
        if flags & 0x8000 != 0 {
            return out; // a response, not a query
        }
        let legacy = from_port != MDNS_PORT;
        let mut pos = 12;
        let (mut mc_answers, mut mc_add, mut uc_answers, mut uc_add) = (0, 0, 0, 0);
        let (mut mc_questions, mut uc_questions) = (Vec::new(), Vec::new());
        let mut name = Vec::new();
        for _ in 0..qd {
            if !read_name(p, &mut pos, &mut name) || pos + 4 > p.len() {
                return out;
            }
            let qtype = be16(p, pos);
            let qclass = be16(p, pos + 2);
            pos += 4;
            if qclass & 0x7fff != 1 && qclass & 0x7fff != 255 {
                continue;
            }
            let (a, add) = self.select(&name, qtype);
            if a == 0 {
                continue;
            }
            let traced = || {
                if trace {
                    vec![(String::from_utf8_lossy(&name).into_owned(), qtype)]
                } else {
                    Vec::new()
                }
            };
            if legacy {
                // A one-shot resolver (plain DNS client sending to 5353): reply directly,
                // echoing the question.
                let mut question = Vec::new();
                put_name(&mut question, &name);
                question.extend_from_slice(&p[pos - 4..pos - 2]);
                put16(&mut question, 1);
                out.push(Response {
                    dest: Dest::Sender,
                    reply: Reply::Legacy,
                    packet: self.build(id, a, add, Some(LEGACY_TTL), false, &question),
                    questions: traced(),
                });
            } else if qclass & 0x8000 != 0 {
                uc_answers |= a;
                uc_add |= add;
                uc_questions.extend(traced());
            } else {
                mc_answers |= a;
                mc_add |= add;
                mc_questions.extend(traced());
            }
        }
        if uc_answers != 0 {
            out.push(Response {
                dest: Dest::Sender,
                reply: Reply::Unicast,
                packet: self.build(0, uc_answers, uc_add, None, false, &[]),
                questions: uc_questions,
            });
        }
        if mc_answers != 0 {
            out.push(Response {
                dest: Dest::Group,
                reply: Reply::Multicast,
                packet: self.build(0, mc_answers, mc_add, None, false, &[]),
                questions: mc_questions,
            });
        }
        out
    }

    /// Answer and additional records for one question; `name` is lowercase.
    fn select(&self, name: &[u8], qtype: u16) -> (u32, u32) {
        let (mut a, mut add) = (0, 0);
        let any = qtype == T_ANY;
        for (i, s) in self.services.iter().enumerate() {
            if name.eq_ignore_ascii_case(s.type_fqdn.as_bytes()) && (any || qtype == T_PTR) {
                a |= bit(i, Kind::Ptr);
                add |= bit(i, Kind::Srv) | bit(i, Kind::Txt) | A;
            }
            if name == SERVICES_ENUM.as_bytes() && (any || qtype == T_PTR) {
                a |= bit(i, Kind::Enum);
            }
            if name.eq_ignore_ascii_case(s.instance_fqdn.as_bytes()) {
                if any || qtype == T_SRV {
                    a |= bit(i, Kind::Srv);
                    add |= A;
                }
                if any || qtype == T_TXT {
                    a |= bit(i, Kind::Txt);
                }
            }
        }
        if name.eq_ignore_ascii_case(self.host_fqdn.as_bytes()) && (any || qtype == T_A) {
            a |= A;
        }
        (a, add)
    }
}

/// Appends the records in order: header, then RDATA and the record itself.
fn encode_records(out: &mut Vec<u8>, records: &[Record<'_>], ttl_cap: Option<u32>, goodbye: bool) {
    let mut rd = Vec::new();
    for r in records {
        rd.clear();
        match &r.data {
            RData::A(ip) => rd.extend_from_slice(&ip.octets()),
            RData::Ptr(name) => put_name(&mut rd, name.as_bytes()),
            RData::Srv { port, target } => {
                put16(&mut rd, 0); // priority
                put16(&mut rd, 0); // weight
                put16(&mut rd, *port);
                put_name(&mut rd, target.as_bytes());
            }
            RData::Txt(txt) => {
                for t in *txt {
                    let t = &t.as_bytes()[..t.len().min(255)];
                    rd.push(t.len() as u8);
                    rd.extend_from_slice(t);
                }
                if rd.is_empty() {
                    rd.push(0);
                }
            }
        }
        put_name(out, r.name.as_bytes());
        put16(out, r.rtype.code());
        let flush = if r.unique && ttl_cap.is_none() {
            0x8000
        } else {
            0
        };
        put16(out, flush | 1);
        let ttl = if goodbye {
            0
        } else {
            ttl_cap.map_or(r.ttl, |cap| r.ttl.min(cap))
        };
        out.extend_from_slice(&ttl.to_be_bytes());
        put16(out, rd.len() as u16);
        out.extend_from_slice(&rd);
    }
}
