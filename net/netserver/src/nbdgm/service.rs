// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server's host announcements ([MS-BRWS] 3.2), as a state machine
//! without sockets: [`Announcer`] says what to send at each moment and
//! for each datagram heard.
//!
//! - A `HostAnnouncement` goes to the workgroup's local master browser
//!   (`WORKGROUP<1D>`, a direct group datagram broadcast on the subnet)
//!   when the server starts, then after 1, 2, 4 and 8 minutes, the
//!   interval doubling up to the browse interval attribute (12 minutes
//!   by default); its periodicity says when the next one comes, so the
//!   browser knows when to drop the server from its list.
//! - An `AnnouncementRequest` from a browser brings the next announcement
//!   forward to a random moment within 30 seconds, so that the servers of
//!   a subnet do not all answer at once.
//! - When the server ends, a last announcement with no server type bits
//!   says it is gone.
//!
//! The server is never a browser itself: it takes no part in elections
//! and keeps no browse list (a browse list is served over SMB1, which is
//! refused).

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use super::packet::{
    Announcement, BROWSE, DIRECT_GROUP, Datagram, SV_TYPE_PRINTQ_SERVER, SV_TYPE_SERVER,
    SV_TYPE_SERVER_NT, SV_TYPE_WORKSTATION, is_announcement_request, mailslot_write,
    parse_mailslot,
};
use crate::nbss::{NbName, SUFFIX_WORKSTATION};

/// The datagram service port.
pub const PORT: u16 = 138;
/// The first interval between announcements.
pub const FIRST_INTERVAL: Duration = Duration::from_mins(1);
/// The longest wait before answering an `AnnouncementRequest`.
pub const REQUEST_DELAY: Duration = Duration::from_secs(30);
/// The local master browser's name suffix.
pub const SUFFIX_MASTER_BROWSER: u8 = 0x1D;
/// The operating system version announced.
const OS_VERSION: (u8, u8) = (6, 1);

/// What the server announces.
#[derive(Debug, Clone)]
pub struct AnnounceConfig {
    /// The server's `NetBIOS` name.
    pub server: String,
    /// The workgroup (the domain attribute); nothing is announced without one.
    pub workgroup: String,
    /// The server's text.
    pub comment: String,
    /// Whether it has print shares.
    pub printing: bool,
    /// The longest interval between announcements (the browse interval
    /// attribute); zero announces nothing.
    pub interval: Duration,
}

impl AnnounceConfig {
    /// Whether anything is announced.
    #[must_use]
    pub fn announces(&self) -> bool {
        !self.interval.is_zero() && !self.workgroup.trim().is_empty() && !self.server.is_empty()
    }

    /// The `SV_TYPE_*` bits announced while the server runs.
    #[must_use]
    pub fn server_type(&self) -> u32 {
        let mut t = SV_TYPE_WORKSTATION | SV_TYPE_SERVER | SV_TYPE_SERVER_NT;
        if self.printing {
            t |= SV_TYPE_PRINTQ_SERVER;
        }
        t
    }
}

/// A datagram to broadcast on UDP 138.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Out {
    /// The datagram.
    pub datagram: Datagram,
}

/// The announcements of one server.
#[derive(Debug)]
pub struct Announcer {
    cfg: AnnounceConfig,
    /// When the next announcement is due; `None` before the start and
    /// when nothing is announced.
    next: Option<Instant>,
    /// The interval that follows the next announcement.
    interval: Duration,
    id: u16,
    seed: u64,
}

impl Announcer {
    /// The announcer of `cfg`; nothing is sent until [`Announcer::start`].
    #[must_use]
    pub fn new(cfg: AnnounceConfig) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| u64::from(d.subsec_nanos()) | 1)
            ^ (u64::from(std::process::id()) << 32);
        Self {
            interval: FIRST_INTERVAL.min(cfg.interval),
            cfg,
            next: None,
            id: (seed & 0xFFFF) as u16,
            seed,
        }
    }

    /// When [`Announcer::tick`] next has something to do.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        self.next
    }

    /// The server started at `now` with the address `ip`: the first
    /// announcement.
    pub fn start(&mut self, now: Instant, ip: Ipv4Addr) -> Vec<Out> {
        if !self.cfg.announces() {
            return Vec::new();
        }
        self.next = Some(now);
        self.tick(now, ip)
    }

    /// `now` passed: an announcement when one is due.
    pub fn tick(&mut self, now: Instant, ip: Ipv4Addr) -> Vec<Out> {
        match self.next {
            Some(at) if at <= now => {}
            _ => return Vec::new(),
        }
        let gap = self.interval;
        self.next = Some(now + gap);
        self.interval = (gap * 2).min(self.cfg.interval);
        vec![self.announcement(ip, gap, self.cfg.server_type())]
    }

    /// A datagram from `from` arrived at `now`: an `AnnouncementRequest`
    /// to the browser mailslot brings the next announcement forward.
    pub fn handle(&mut self, now: Instant, from: SocketAddrV4, bytes: &[u8]) {
        let _ = from;
        let Some(next) = self.next else {
            return;
        };
        let Ok(d) = Datagram::parse(bytes) else {
            return;
        };
        let Ok((slot, frame)) = parse_mailslot(&d.data) else {
            return;
        };
        if !slot.eq_ignore_ascii_case(BROWSE) || !is_announcement_request(frame) {
            return;
        }
        let at = now + self.delay();
        if at < next {
            self.next = Some(at);
        }
    }

    /// The server ends: a last announcement with no type bits.
    pub fn stop(&mut self, ip: Ipv4Addr) -> Vec<Out> {
        if self.next.take().is_none() {
            return Vec::new();
        }
        vec![self.announcement(ip, Duration::ZERO, 0)]
    }

    /// A random wait within [`REQUEST_DELAY`] (xorshift).
    fn delay(&mut self) -> Duration {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        let ms = u64::try_from(REQUEST_DELAY.as_millis()).unwrap_or(30_000);
        Duration::from_millis(x % ms)
    }

    fn announcement(&mut self, ip: Ipv4Addr, gap: Duration, server_type: u32) -> Out {
        let frame = Announcement {
            periodicity: u32::try_from(gap.as_millis()).unwrap_or(u32::MAX),
            server: self.cfg.server.clone(),
            os_version: OS_VERSION,
            server_type,
            comment: self.cfg.comment.clone(),
        }
        .encode();
        self.id = self.id.wrapping_add(1);
        Out {
            datagram: Datagram {
                kind: DIRECT_GROUP,
                id: self.id,
                source_ip: ip,
                source_port: PORT,
                source: NbName::new(&self.cfg.server, SUFFIX_WORKSTATION),
                dest: NbName::new(&self.cfg.workgroup, SUFFIX_MASTER_BROWSER),
                data: mailslot_write(BROWSE, &frame),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::packet::{ANNOUNCEMENT_REQUEST, Announcement, BROADCAST};
    use super::*;

    const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 40);

    fn cfg(interval: Duration) -> AnnounceConfig {
        AnnounceConfig {
            server: "QS400".into(),
            workgroup: "WORKGROUP".into(),
            comment: "Quartz Systems".into(),
            printing: true,
            interval,
        }
    }

    fn frame(o: &Out) -> Announcement {
        let (slot, f) = parse_mailslot(&o.datagram.data).unwrap();
        assert_eq!(slot, BROWSE);
        Announcement::parse(f).unwrap()
    }

    fn request() -> Vec<u8> {
        Datagram {
            kind: BROADCAST,
            id: 9,
            source_ip: Ipv4Addr::new(192, 168, 1, 2),
            source_port: PORT,
            source: NbName::new("MASTER", 0),
            dest: NbName::new("WORKGROUP", 0),
            data: mailslot_write(BROWSE, &[ANNOUNCEMENT_REQUEST, 0, b'M', 0]),
        }
        .encode()
    }

    #[test]
    fn announces_at_start_then_doubling_up_to_the_interval() {
        let mut a = Announcer::new(cfg(Duration::from_mins(12)));
        let t0 = Instant::now();
        assert!(a.tick(t0, IP).is_empty(), "nothing before the start");
        let out = a.start(t0, IP);
        assert_eq!(out.len(), 1);
        let d = &out[0].datagram;
        assert_eq!(d.kind, DIRECT_GROUP);
        assert!(d.dest.is("WORKGROUP", SUFFIX_MASTER_BROWSER));
        assert!(d.source.is("QS400", 0));
        assert_eq!((d.source_ip, d.source_port), (IP, PORT));
        let f = frame(&out[0]);
        assert_eq!(f.server, "QS400");
        assert_eq!(f.comment, "Quartz Systems");
        assert_eq!(
            f.server_type,
            SV_TYPE_WORKSTATION | SV_TYPE_SERVER | SV_TYPE_SERVER_NT | SV_TYPE_PRINTQ_SERVER
        );
        assert_eq!(f.periodicity, 60_000);
        let mut at = t0;
        let mut gaps = Vec::new();
        for _ in 0..6 {
            let next = a.next_wake().unwrap();
            assert!(
                a.tick(next.checked_sub(Duration::from_millis(1)).unwrap(), IP)
                    .is_empty()
            );
            gaps.push((next - at).as_secs() / 60);
            at = next;
            assert_eq!(a.tick(at, IP).len(), 1);
        }
        assert_eq!(gaps, [1, 2, 4, 8, 12, 12]);
    }

    #[test]
    fn a_short_interval_caps_the_first() {
        let mut a = Announcer::new(cfg(Duration::from_secs(20)));
        let t0 = Instant::now();
        assert_eq!(frame(&a.start(t0, IP)[0]).periodicity, 20_000);
        assert_eq!(a.next_wake(), Some(t0 + Duration::from_secs(20)));
    }

    #[test]
    fn nothing_without_an_interval_or_a_workgroup() {
        let mut a = Announcer::new(cfg(Duration::ZERO));
        assert!(a.start(Instant::now(), IP).is_empty());
        assert!(a.next_wake().is_none());
        assert!(a.stop(IP).is_empty());
        let mut c = cfg(Duration::from_mins(12));
        c.workgroup = " ".into();
        let mut a = Announcer::new(c);
        assert!(a.start(Instant::now(), IP).is_empty());
    }

    #[test]
    fn a_request_brings_the_next_one_within_thirty_seconds() {
        let mut a = Announcer::new(cfg(Duration::from_mins(12)));
        let t0 = Instant::now();
        let from = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 2), PORT);
        // Before the start it is ignored.
        a.handle(t0, from, &request());
        a.start(t0, IP);
        let now = t0 + Duration::from_secs(5);
        a.handle(now, from, &request());
        let next = a.next_wake().unwrap();
        assert!(
            next >= now && next <= now + REQUEST_DELAY,
            "{:?}",
            next - now
        );
        // Anything else changes nothing.
        let before = a.next_wake();
        a.handle(now, from, &[0x11, 0x02, 0, 0]);
        let mut other = request();
        let n = other.len();
        other[n - 4] = 0x01;
        a.handle(now, from, &other);
        assert_eq!(a.next_wake(), before);
    }

    #[test]
    fn the_last_announcement_has_no_type() {
        let mut a = Announcer::new(cfg(Duration::from_mins(12)));
        a.start(Instant::now(), IP);
        let out = a.stop(IP);
        assert_eq!(frame(&out[0]).server_type, 0);
        assert!(a.next_wake().is_none());
        assert!(a.stop(IP).is_empty());
    }
}
