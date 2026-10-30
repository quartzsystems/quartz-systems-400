// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `PRTCMNTRC` listing (132 columns): the trace's description as
//! labelled values, then a line per record (number, sent or received,
//! length, time, MAC addresses, frame format) followed by its decoded
//! ARP, IP and TCP/UDP/ICMP headers and the data in hex with its
//! characters. `FMTTCP(*YES)` keeps the IP frames (and ARP) only,
//! narrowed by `TCPIPADR` and `SLTPORT`; `FMTBCST(*NO)` drops
//! broadcasts. The layout is IBM's in shape; the words are ours
//! (`docs/NET_OPEN_QUESTIONS.md`, "Network tools").

use std::fmt::Write as _;
use std::net::IpAddr;

use super::frame::{self, Frame, L4};
use super::{Record, Trace};

/// `CODE`: how the data's characters are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// `*CALC`: ASCII for IP frames, which is all an Ethernet line carries here.
    Calc,
    /// `*ASCII`.
    Ascii,
    /// `*EBCDIC` (CCSID 37).
    Ebcdic,
}

/// What `PRTCMNTRC` selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    /// `FMTTCP(*YES)`: IP and ARP frames only.
    pub tcp_only: bool,
    /// `TCPIPADR`: frames between these (the second `None` is `*ALL`).
    pub addrs: Option<(IpAddr, Option<IpAddr>)>,
    /// `SLTPORT`: frames to or from this port.
    pub port: Option<u16>,
    /// `FMTBCST`: broadcast and multicast frames printed.
    pub broadcast: bool,
    /// `CODE`.
    pub code: Code,
}

impl Default for Select {
    fn default() -> Self {
        Self {
            tcp_only: true,
            addrs: None,
            port: None,
            broadcast: true,
            code: Code::Calc,
        }
    }
}

impl Select {
    /// Whether a decoded frame is printed.
    pub fn keeps(&self, f: Option<&Frame>) -> bool {
        let Some(f) = f else {
            return !self.tcp_only;
        };
        if !self.broadcast && f.broadcast() {
            return false;
        }
        if self.tcp_only && f.ip.is_none() && f.arp.is_none() {
            return false;
        }
        if let Some((a, b)) = self.addrs {
            let pair: Option<(IpAddr, IpAddr)> = match (&f.ip, &f.arp) {
                (Some(ip), _) => Some((ip.src, ip.dst)),
                (None, Some(arp)) => Some((IpAddr::V4(arp.spa), IpAddr::V4(arp.tpa))),
                _ => None,
            };
            let Some((s, d)) = pair else {
                return false;
            };
            let hit = match b {
                Some(b) => (s == a && d == b) || (s == b && d == a),
                None => s == a || d == a,
            };
            if !hit {
                return false;
            }
        }
        if let Some(p) = self.port
            && !f.ports().is_some_and(|(s, d)| s == p || d == p)
        {
            return false;
        }
        true
    }
}

/// A civil date and time from microseconds since the epoch, shifted by
/// `offset` seconds: `("MM/DD/YY", "HH:MM:SS.uuuuuu")`.
pub fn civil(us: u64, offset: i64) -> (String, String) {
    let secs = i64::try_from(us / 1_000_000).unwrap_or(i64::MAX).saturating_add(offset);
    let micros = us % 1_000_000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        format!("{m:02}/{d:02}/{:02}", y.rem_euclid(100)),
        format!("{:02}:{:02}:{:02}.{micros:06}", rem / 3600, rem / 60 % 60, rem % 60),
    )
}

fn hexs(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

fn label(text: &str, value: &str) -> String {
    // Dots on even columns to 28, the colon on 30, the value on 34.
    let mut l = format!(" {text} ");
    l.truncate(30);
    while l.len() < 30 {
        l.push(if l.len() % 2 == 0 { '.' } else { ' ' });
    }
    format!("{l}:   {value}")
}

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

/// A byte as a character in `code`, `.` when it has no picture.
fn shown(b: u8, code: Code) -> char {
    match code {
        Code::Calc | Code::Ascii => {
            if (0x20..0x7f).contains(&b) {
                char::from(b)
            } else {
                '.'
            }
        }
        Code::Ebcdic => ccsid::from_ebcdic(&[b])
            .chars()
            .next()
            .filter(|c| !c.is_control() && *c != '\u{fffd}')
            .unwrap_or('.'),
    }
}

/// The data dump: 16 bytes a line, in four words, with the characters.
fn dump(out: &mut Vec<String>, tag: &str, b: &[u8], code: Code) {
    for (i, chunk) in b.chunks(16).enumerate() {
        let mut words = String::new();
        for (j, w) in chunk.chunks(4).enumerate() {
            if j > 0 {
                words.push(' ');
            }
            words.push_str(&hexs(w));
        }
        let chars: String = chunk.iter().map(|x| shown(*x, code)).collect();
        let head = if i == 0 { tag } else { "" };
        out.push(format!("{:22}{head:<7}{:04X}  {words:<35}  *{chars}*", "", i * 16));
    }
}

fn proto_name(p: u8) -> String {
    match p {
        1 => "ICMP".into(),
        6 => "TCP".into(),
        17 => "UDP".into(),
        58 => "ICMPV6".into(),
        n => n.to_string(),
    }
}

fn record(out: &mut Vec<String>, n: u64, r: &Record, sel: &Select, offset: i64) {
    let f = frame::parse(&r.data);
    let (_, time) = civil(r.time, offset);
    let (dst, src, fmt) = match &f {
        Some(f) => (
            hexs(&f.dst),
            hexs(&f.src),
            if f.ethertype >= 0x0600 {
                format!("ETHV2   Type: {:04X}", f.ethertype)
            } else {
                "802.3".into()
            },
        ),
        None => (String::new(), String::new(), String::new()),
    };
    out.push(
        format!(
            "{n:>7}  {:^3}  {:>6}  {time}  {dst:12}  {src:12}  {fmt}",
            if r.sent { "S" } else { "R" },
            r.len
        )
        .trim_end()
        .to_string(),
    );
    let pad = " ".repeat(22);
    if let Some(f) = &f {
        if let Some(v) = f.vlan {
            out.push(format!("{pad}VLAN   Identifier: {v}"));
        }
        if let Some(a) = &f.arp {
            out.push(format!(
                "{pad}ARP    {}  Sender: {} {}  Target: {} {}",
                if a.op == 1 { "Request" } else { "Reply" },
                a.spa,
                hexs(&a.sha),
                a.tpa,
                hexs(&a.tha)
            ));
        }
        if let Some(ip) = &f.ip {
            let (df, mf, off) = ip.frag;
            let mut flags = Vec::new();
            if df {
                flags.push("DF");
            }
            if mf {
                flags.push("MF");
            }
            out.push(format!(
                "{pad}IP     ToS: {:02X}  Length: {}  Protocol: {}  ID: {:04X}  TTL: {}  Flags: {}  Offset: {off}",
                ip.tos,
                ip.len,
                proto_name(ip.proto),
                ip.id,
                ip.ttl,
                if flags.is_empty() { "NONE".into() } else { flags.join(",") }
            ));
            out.push(format!("{pad}       Source: {}  Destination: {}", ip.src, ip.dst));
            out.push(format!("{pad}       Header: {}", hexs(&ip.header)));
        }
        match &f.l4 {
            Some(L4::Tcp { src, dst, seq, ack, flags, window, header }) => {
                out.push(format!(
                    "{pad}TCP    Source port: {src}  Destination port: {dst}  Sequence: {seq}  Acknowledgment: {ack}  Window: {window}"
                ));
                out.push(format!("{pad}       Flags: {}", frame::tcp_flags(*flags).join(" ")));
                out.push(format!("{pad}       Header: {}", hexs(header)));
            }
            Some(L4::Udp { src, dst, len }) => {
                out.push(format!("{pad}UDP    Source port: {src}  Destination port: {dst}  Length: {len}"));
            }
            Some(L4::Icmp { kind, code }) => {
                out.push(format!("{pad}ICMP   Type: {kind}  Code: {code}"));
            }
            None => {}
        }
    }
    let start = f.as_ref().map_or(0, |f| f.payload.min(r.data.len()));
    let data = &r.data[start..];
    if !data.is_empty() {
        let code = if sel.code == Code::Calc { Code::Ascii } else { sel.code };
        dump(out, "Data", data, code);
    }
}

/// A heading row in the record lines' columns.
fn heading(c: [&str; 7]) -> String {
    format!("{:>7}  {:^3}  {:>6}  {:<15}  {:<12}  {:<12}  {}", c[0], c[1], c[2], c[3], c[4], c[5], c[6])
        .trim_end()
        .to_string()
}

/// The listing of trace `t`; `offset` shifts times to local time.
pub fn listing(t: &Trace, sel: &Select, offset: i64) -> Vec<String> {
    let h = &t.header;
    let (sd, st) = civil(h.started, offset);
    let end = h.ended.map_or_else(|| "*ACTIVE".to_string(), |e| {
        let (d, t) = civil(e, offset);
        format!("{d} {t}")
    });
    let addr = |a: Option<IpAddr>| a.map_or_else(|| "*ALL".to_string(), |x| x.to_string());
    let mut out = vec![
        format!("Communications trace of {} type {}", h.cfgobj, h.cfgtype.name()),
        String::new(),
        label("Configuration object", &h.cfgobj),
        label("Type", &format!("{:<13}1=Line, 2=Network interface, 3=Network server", h.cfgtype.number())),
        label("Object protocol", &h.protocol),
        label("Kernel link", &h.ifname),
        label("Description", &h.text),
        label("Start date/time", &format!("{sd} {st}")),
        label("End date/time", &end),
        label("Status", h.status.name()),
        label("Bytes collected", &t.collected.to_string()),
        label("Buffer size", &h.maxstg.to_string()),
        label(
            "Data direction",
            &format!(
                "{:<13}1=Sent, 2=Received, 3=Both",
                match h.dir {
                    super::Direction::Sent => 1,
                    super::Direction::Received => 2,
                    super::Direction::Both => 3,
                }
            ),
        ),
        label("Stop on buffer full", &format!("{:<13}Y=Yes, N=No", yn(!h.wrap))),
        label("Bytes kept per frame", &h.usrdta.map_or_else(|| "*CALC".into(), |n| n.to_string())),
        label("Format TCP/IP data only", &format!("{:<13}Y=Yes, N=No", yn(sel.tcp_only))),
        label("  IP address", &addr(sel.addrs.map(|a| a.0))),
        label("  IP address", &addr(sel.addrs.and_then(|a| a.1))),
        label("  IP port", &sel.port.map_or_else(|| "*ALL".into(), |p| p.to_string())),
        label("Format broadcast data", &format!("{:<13}Y=Yes, N=No", yn(sel.broadcast))),
        String::new(),
        heading(["Record", "", "Data", "", "Destination", "Source", ""]),
        heading(["Number", "S/R", "Length", "Timer", "MAC address", "MAC address", "Format"]),
        heading(["------", "---", "------", "---------------", "------------", "------------", "------"]),
    ];
    let mut printed = 0;
    let first = t.first_number();
    for (i, r) in t.records.iter().enumerate() {
        let f = frame::parse(&r.data);
        if !sel.keeps(f.as_ref()) {
            continue;
        }
        printed += 1;
        record(&mut out, first + i as u64, r, sel, offset);
    }
    out.push(String::new());
    out.push(label("Records in the trace", &t.records.len().to_string()));
    out.push(label("Records printed", &printed.to_string()));
    out.push(" End of the communications trace listing.".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmntrc::frame::sample;
    use crate::cmntrc::tests::header;

    #[test]
    fn civil_time() {
        // 2026-10-30 12:34:56.000789 UTC.
        let us = 1_793_363_696 * 1_000_000 + 789;
        assert_eq!(civil(us, 0), ("10/30/26".to_string(), "12:34:56.000789".to_string()));
        assert_eq!(civil(us, -3600).1, "11:34:56.000789");
        assert_eq!(civil(0, 0).0, "01/01/70");
    }

    #[test]
    fn listing_selects_and_decodes() {
        let mut t = Trace::new(header());
        t.push(1_793_363_696_000_000, true, &sample(6, [10, 0, 2, 15], [10, 0, 2, 2], 23, 40000));
        t.push(1_793_363_696_000_100, false, &sample(17, [10, 0, 2, 3], [10, 0, 2, 15], 53, 5353));
        let mut junk = vec![0xffu8; 12];
        junk.extend_from_slice(&[0x88, 0xcc, 1, 2, 3]);
        t.push(1_793_363_696_000_200, false, &junk);
        let all = listing(&t, &Select { tcp_only: false, ..Select::default() }, 0);
        let text = all.join("\n");
        assert!(text.contains(" Configuration object . . . . :   ETHLINE"), "{text}");
        assert!(text.contains("      1   S       58  12:34:56.000000  52550A000202  525400123456  ETHV2   Type: 0800"), "{text}");
        assert!(text.contains("TCP    Source port: 23  Destination port: 40000"));
        assert!(text.contains("Flags: PSH ACK"));
        assert!(text.contains("UDP    Source port: 53  Destination port: 5353  Length: 12"));
        assert!(text.contains("Data   0000  70696E67"), "{text}");
        assert!(text.contains("*ping*"));
        assert!(text.contains("Records printed  . . . . . . :   3"), "{text}");
        let tcp = listing(&t, &Select::default(), 0).join("\n");
        assert!(tcp.contains("Records printed  . . . . . . :   2"), "{tcp}");
        let port = Select { port: Some(53), ..Select::default() };
        assert!(listing(&t, &port, 0).join("\n").contains("Records printed  . . . . . . :   1"));
        let addr = Select { addrs: Some(("10.0.2.2".parse().unwrap(), None)), ..Select::default() };
        assert!(listing(&t, &addr, 0).join("\n").contains("Records printed  . . . . . . :   1"));
        assert!(all.iter().all(|l| l.len() <= 132), "132 columns");
    }

    #[test]
    fn labels_line_up() {
        assert_eq!(label("Type", "1"), " Type . . . . . . . . . . . . :   1");
        assert_eq!(label("Configuration object", "X"), " Configuration object . . . . :   X");
        assert_eq!(label("  IP port", "*ALL"), "   IP port  . . . . . . . . . :   *ALL");
    }

    #[test]
    fn ebcdic_characters() {
        assert_eq!(shown(0xc1, Code::Ebcdic), 'A');
        assert_eq!(shown(0xf5, Code::Ebcdic), '5');
        assert_eq!(shown(0x41, Code::Ascii), 'A');
        assert_eq!(shown(0x00, Code::Ascii), '.');
    }
}
