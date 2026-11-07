//! Finding stations on the network: an mDNS query for Yandex announcements and the
//! `--scan` /24 adb-port probe.
//!
//! Only answers that announce a Yandex Station get an `adb connect`: stock firmware as
//! `_yandexio._tcp`, converted ones as ESPHome devices named `yandex-*`. Everything else
//! (including other Android devices on the network) is left alone.

use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

/// Whether an mDNS response announces a Yandex Station: an answer PTR for
/// `_yandexio._tcp.local.` (stock firmware), or for `_esphomelib._tcp.local.` whose
/// instance label starts with "yandex-". Names compared case-insensitively; unparsable
/// packets -> false.
pub fn station_announce(pkt: &[u8]) -> bool {
    if pkt.len() < 12 {
        return false;
    }
    let flags = u16::from_be_bytes([pkt[2], pkt[3]]);
    if flags & 0x8000 == 0 {
        return false; // not a response
    }
    let qd = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let an = u16::from_be_bytes([pkt[6], pkt[7]]) as usize;
    let mut pos = 12;
    for _ in 0..qd {
        let Ok((_, next)) = parse_name(pkt, pos) else {
            return false;
        };
        pos = next + 4;
        if pos > pkt.len() {
            return false;
        }
    }
    for _ in 0..an {
        let Ok((name, next)) = parse_name(pkt, pos) else {
            return false;
        };
        pos = next;
        if pos + 10 > pkt.len() {
            return false;
        }
        let rtype = u16::from_be_bytes([pkt[pos], pkt[pos + 1]]);
        let rdlen = u16::from_be_bytes([pkt[pos + 8], pkt[pos + 9]]) as usize;
        let rdata = pos + 10;
        if rdata + rdlen > pkt.len() {
            return false;
        }
        pos = rdata + rdlen;
        if rtype != 12 {
            continue; // PTR only
        }
        let Ok((target, _)) = parse_name(pkt, rdata) else {
            return false;
        };
        let owner = name.to_ascii_lowercase();
        let target = target.to_ascii_lowercase();
        if owner == "_yandexio._tcp.local." {
            return true;
        }
        if owner == "_esphomelib._tcp.local." && target.starts_with("yandex-") {
            return true;
        }
    }
    false
}

/// Parses a (possibly compressed) DNS name at `pos`; returns the name and the position just
/// after its top-level encoding.
fn parse_name(msg: &[u8], start: usize) -> Result<(String, usize), ()> {
    let mut out = String::new();
    let mut pos = start;
    let mut after: Option<usize> = None;
    let mut jumps = 0;
    loop {
        let Some(&len) = msg.get(pos) else {
            return Err(());
        };
        if len & 0xC0 == 0xC0 {
            let Some(&lo) = msg.get(pos + 1) else {
                return Err(());
            };
            let ptr = (usize::from(len & 0x3F) << 8) | usize::from(lo);
            if jumps > 16 || ptr >= msg.len() {
                return Err(());
            }
            jumps += 1;
            if after.is_none() {
                after = Some(pos + 2);
            }
            pos = ptr;
            continue;
        }
        if len == 0 {
            let end = after.unwrap_or(pos + 1);
            return Ok((out, end));
        }
        let len = usize::from(len);
        if len > 63 || pos + 1 + len > msg.len() {
            return Err(());
        }
        let label = &msg[pos + 1..pos + 1 + len];
        out.push_str(&String::from_utf8_lossy(label));
        out.push('.');
        pos += 1 + len;
    }
}

/// A UDP socket bound to `local` with the mDNS interface and TTL set, like Go's
/// `SetMulticastInterface`/`SetMulticastTTL`.
fn mdns_socket(local: Ipv4Addr) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.bind(&SocketAddrV4::new(local, 0).into())?;
    socket.set_multicast_if_v4(&local)?;
    socket.set_multicast_ttl_v4(255)?;
    Ok(socket.into())
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

pub fn mdns_query(service: &str) -> Vec<u8> {
    let mut pkt = vec![0u8; 12];
    pkt[5] = 1; // qdcount = 1
    for label in service.trim_end_matches('.').split('.') {
        pkt.push(label.len() as u8);
        pkt.extend_from_slice(label.as_bytes());
    }
    pkt.push(0);
    pkt.extend_from_slice(&12u16.to_be_bytes()); // PTR
    pkt.extend_from_slice(&1u16.to_be_bytes()); // IN
    pkt
}

/// The local address of the default-route interface (no packet is sent).
pub fn default_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(a) if !a.ip().is_loopback() && !a.ip().is_unspecified() => Some(*a.ip()),
        _ => None,
    }
}

pub fn local_ipv4(err: &mut dyn Write) -> Option<Ipv4Addr> {
    let ip = default_ipv4();
    if ip.is_none() {
        let _ = writeln!(
            err,
            "no default-route IPv4 interface found: pass the station's address (pulsar-install <ip>)"
        );
    }
    ip
}

pub fn sort_addrs(addrs: &mut [String]) {
    addrs.sort_by_key(|a| {
        a.rsplit_once(':')
            .and_then(|(ip, port)| Some((ip.parse::<Ipv4Addr>().ok()?, port.parse::<u16>().ok()?)))
            .unwrap_or((Ipv4Addr::UNSPECIFIED, 0))
    });
}

/// `ip:port` for the hosts prefix.1 … prefix.254 accepting TCP on `port`.
pub fn scan_net(prefix: &str, port: u16) -> Vec<String> {
    let found = Arc::new(Mutex::new(Vec::new()));
    // Go's semaphore: 254 goroutines, at most 64 dialling at once.
    let slots: Arc<(Mutex<usize>, Condvar)> = Arc::new((Mutex::new(0), Condvar::new()));
    let mut handles = Vec::with_capacity(254);
    for i in 1..=254u8 {
        let addr = format!("{prefix}.{i}:{port}");
        let found = Arc::clone(&found);
        let slots = Arc::clone(&slots);
        handles.push(thread::spawn(move || {
            {
                let (lock, cv) = &*slots;
                let mut active = lock.lock().expect("scan slots");
                while *active >= 64 {
                    active = cv.wait(active).expect("scan slots");
                }
                *active += 1;
            }
            if TcpStream::connect_timeout(
                &addr.parse::<SocketAddr>().expect("literal address"),
                Duration::from_millis(500),
            )
            .is_ok()
            {
                found.lock().expect("scan mutex").push(addr);
            }
            {
                let (lock, cv) = &*slots;
                let mut active = lock.lock().expect("scan slots");
                *active -= 1;
                drop(active);
                cv.notify_one();
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    let mut found = Arc::try_unwrap(found)
        .expect("scan threads joined")
        .into_inner()
        .expect("scan mutex");
    sort_addrs(&mut found);
    found
}

/// Finds open adb ports in the /24 of the default-route interface. May reach other Android
/// devices, which can show an adb authorization prompt.
pub fn scan(err: &mut dyn Write) -> Vec<String> {
    let Some(ip) = local_ipv4(err) else {
        return Vec::new();
    };
    let o = ip.octets();
    let prefix = format!("{}.{}.{}", o[0], o[1], o[2]);
    let _ = writeln!(err, "scanning {prefix}.0/24 for adb...");
    scan_net(&prefix, 5555)
}

/// Stations announced over mDNS: `ip:5555`. Queries go out from an ephemeral port ("legacy
/// unicast"): responders answer to that port directly, so port 5353 need not be free.
pub fn mdns(err: &mut dyn Write) -> Vec<String> {
    let _ = writeln!(err, "looking for stations via mDNS...");
    let Some(local) = local_ipv4(err) else {
        return Vec::new();
    };
    let sock = match mdns_socket(local) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "mDNS: {e}");
            return Vec::new();
        }
    };
    let group = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
    let queries = [
        mdns_query("_yandexio._tcp.local."),
        mdns_query("_esphomelib._tcp.local."),
    ];
    let send = || {
        for q in &queries {
            let _ = sock.send_to(q, group);
        }
    };
    send();
    let start = Instant::now();
    let mut resent = false;
    let mut seen = Vec::new();
    let mut buf = [0u8; 9000];
    loop {
        let deadline = start
            + if resent {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(1)
            };
        // Go sets the absolute deadline even when it is already past: the read then times
        // out immediately and the retransmit/stop logic below still runs.
        let wait = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .unwrap_or(Duration::from_nanos(1));
        if sock.set_read_timeout(Some(wait)).is_err() {
            break;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                if let SocketAddr::V4(from) = from
                    && station_announce(&buf[..n])
                {
                    let a = format!("{}:5555", from.ip());
                    if !seen.contains(&a) {
                        seen.push(a);
                    }
                }
            }
            // Retransmit once, only after a timeout; any other read error stops the search.
            Err(e) if is_timeout(&e) && !resent => {
                send();
                resent = true;
            }
            Err(_) => break,
        }
    }
    sort_addrs(&mut seen);
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn announce_matches_go_golden() {
        let cases: Value = serde_json::from_str(include_str!("../testdata/discover.json")).unwrap();
        for c in cases["packets"].as_array().unwrap() {
            let raw = c["packet_hex"].as_str().unwrap();
            assert_eq!(
                station_announce(&hex(raw)),
                c["want"].as_bool().unwrap(),
                "{raw}"
            );
        }
    }

    #[test]
    fn query_matches_go_golden() {
        let f: Value = serde_json::from_str(include_str!("../testdata/discover.json")).unwrap();
        assert_eq!(
            mdns_query("_yandexio._tcp.local."),
            hex(f["query_yandexio_hex"].as_str().unwrap())
        );
        assert_eq!(
            mdns_query("_esphomelib._tcp.local."),
            hex(f["query_esphomelib_hex"].as_str().unwrap())
        );
    }

    #[test]
    fn sorts_numerically() {
        let mut a = vec![
            "192.0.2.10:5555".to_string(),
            "192.0.2.2:5555".to_string(),
            "192.0.2.1:5555".to_string(),
        ];
        sort_addrs(&mut a);
        assert_eq!(a, ["192.0.2.1:5555", "192.0.2.2:5555", "192.0.2.10:5555"]);
    }
}
