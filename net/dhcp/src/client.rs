// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's states (RFC 2131 §4.4, figure 5) as a pure machine:
//! the caller feeds it the time and the messages that came, and does
//! what it answers ([`Out`]). Times are milliseconds since the epoch.
//!
//! Retransmission doubles from 4 seconds to 64 with up to a second of
//! jitter (§4.1); `SELECTING` takes the first `OFFER`; a kept lease
//! starts in `INIT-REBOOT` and falls back to `INIT` on a `NAK` or after
//! three unanswered requests; `RENEWING` and `REBINDING` retry at half
//! the time left, at least a minute apart (§4.4.5).

use std::net::Ipv4Addr;

use crate::lease::Lease;
use crate::packet::{Kind, Message, options::code};

/// What the client is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The link's Ethernet address.
    pub mac: [u8; 6],
    /// Option 61; `None` sends the hardware type and address (`*ADPTADR`).
    pub client_id: Option<Vec<u8>>,
    /// Option 12, when the system has a host name.
    pub host_name: Option<String>,
    /// A lease kept from before (an earlier IPL or `STRTCP`).
    pub previous: Option<Lease>,
    /// `DHCPDYNDNS(*YES)`: ask the server to update DNS for the host
    /// name (option 81 with the S flag, RFC 4702).
    pub dns_update: bool,
}

impl Config {
    /// A client with the defaults for `mac`.
    pub fn new(mac: [u8; 6]) -> Self {
        Self {
            mac,
            client_id: None,
            host_name: None,
            previous: None,
            dns_update: false,
        }
    }

    /// The identifier sent: the configured one, else type 1 and the MAC.
    pub fn id(&self) -> Vec<u8> {
        self.client_id.clone().unwrap_or_else(|| {
            let mut v = vec![1];
            v.extend(self.mac);
            v
        })
    }
}

/// The client's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)] // RFC 2131's names
pub enum State {
    Init,
    Selecting,
    Requesting,
    InitReboot,
    Rebooting,
    Bound,
    Renewing,
    Rebinding,
    Released,
}

impl State {
    /// The name the displays and logs use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Init => "INIT",
            Self::Selecting => "SELECTING",
            Self::Requesting => "REQUESTING",
            Self::InitReboot => "INIT-REBOOT",
            Self::Rebooting => "REBOOTING",
            Self::Bound => "BOUND",
            Self::Renewing => "RENEWING",
            Self::Rebinding => "REBINDING",
            Self::Released => "RELEASED",
        }
    }

    /// Whether the client holds an address in this state.
    pub fn holds(self) -> bool {
        matches!(self, Self::Bound | Self::Renewing | Self::Rebinding)
    }
}

/// Where a message goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// `255.255.255.255`.
    Broadcast,
    /// One server.
    Unicast(Ipv4Addr),
}

/// What the caller is to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // short-lived, one at a time
pub enum Out {
    /// Send a message to port 67.
    Send(Dest, Message),
    /// Put the lease's address on the link (a renewal names the same).
    Bind(Lease),
    /// Take the lease's address off the link.
    Unbind(Lease),
}

/// The parameters asked for (option 55).
const PARAMS: &[u8] = &[
    code::SUBNET_MASK,
    code::ROUTER,
    code::DNS,
    code::HOST_NAME,
    code::DOMAIN,
    code::LEASE_TIME,
    code::SERVER_ID,
    code::RENEWAL,
    code::REBINDING,
    code::BOOTFILE,
];

/// The largest message accepted (option 57).
const MAX_SIZE: u16 = 1500;
/// Unanswered requests in `REQUESTING` or `INIT-REBOOT` before `INIT`.
const TRIES: u32 = 3;

/// The client.
#[derive(Clone, Debug)]
pub struct Client {
    cfg: Config,
    state: State,
    xid: u32,
    rng: u64,
    /// When the next timeout falls.
    next: u64,
    /// Retransmissions in the current exchange.
    tries: u32,
    /// When the exchange began (for `secs`).
    began: u64,
    offer: Option<Message>,
    lease: Option<Lease>,
}

impl Client {
    /// A client at `now`; `seed` makes its transaction IDs and jitter.
    pub fn new(cfg: Config, now: u64, seed: u64) -> Self {
        let lease = cfg.previous.clone().filter(|l| !l.expired(now / 1000));
        let mut c = Self {
            cfg,
            state: if lease.is_some() { State::InitReboot } else { State::Init },
            xid: 0,
            rng: seed | 1,
            next: now,
            tries: 0,
            began: now,
            offer: None,
            lease,
        };
        c.xid = c.random();
        c
    }

    /// The state.
    pub fn state(&self) -> State {
        self.state
    }

    /// The lease held (or being rebooted into).
    pub fn lease(&self) -> Option<&Lease> {
        self.lease.as_ref().filter(|_| self.state.holds())
    }

    /// When [`Client::poll`] is next due.
    pub fn deadline(&self) -> u64 {
        if self.state == State::Released { u64::MAX } else { self.next }
    }

    fn random(&mut self) -> u32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        u32::try_from(self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32).unwrap_or(0)
    }

    /// 4, 8, 16, 32, 64 seconds, then 64, with jitter of up to a second.
    fn backoff(&mut self) -> u64 {
        let base = 4000u64 << self.tries.min(4);
        let jitter = u64::from(self.random() % 2001);
        base + jitter - 1000
    }

    fn msg(&self, kind: Kind, now: u64) -> Message {
        let mut m = Message::request(kind, self.xid, self.cfg.mac);
        m.secs = u16::try_from((now.saturating_sub(self.began)) / 1000).unwrap_or(u16::MAX);
        m.options.client_id = Some(self.cfg.id());
        if !matches!(kind, Kind::Release | Kind::Decline) {
            m.options.params = PARAMS.to_vec();
            m.options.max_size = Some(MAX_SIZE);
            m.options.host_name.clone_from(&self.cfg.host_name);
            if self.cfg.dns_update
                && let Some(h) = &self.cfg.host_name
            {
                let mut fqdn = vec![0x01, 0, 0];
                fqdn.extend(h.as_bytes());
                m.options.other.push((81, fqdn));
            }
        }
        m
    }

    fn discover(&mut self, now: u64) -> Vec<Out> {
        self.state = State::Selecting;
        self.offer = None;
        self.began = now;
        self.tries = 0;
        self.xid = self.random();
        self.send_discover(now)
    }

    fn send_discover(&mut self, now: u64) -> Vec<Out> {
        let mut m = self.msg(Kind::Discover, now);
        m.broadcast = true;
        if let Some(l) = &self.cfg.previous {
            m.options.requested_ip = Some(l.addr);
        }
        self.next = now + self.backoff();
        vec![Out::Send(Dest::Broadcast, m)]
    }

    fn send_select(&mut self, now: u64) -> Vec<Out> {
        let Some(o) = &self.offer else {
            return self.discover(now);
        };
        let mut m = self.msg(Kind::Request, now);
        m.broadcast = true;
        m.options.requested_ip = Some(o.yiaddr);
        m.options.server_id = o.options.server_id;
        self.next = now + self.backoff();
        vec![Out::Send(Dest::Broadcast, m)]
    }

    fn send_reboot(&mut self, now: u64) -> Vec<Out> {
        let Some(l) = &self.lease else {
            return self.discover(now);
        };
        let mut m = self.msg(Kind::Request, now);
        m.broadcast = true;
        m.options.requested_ip = Some(l.addr);
        self.next = now + self.backoff();
        vec![Out::Send(Dest::Broadcast, m)]
    }

    /// A request with the lease's address as ours (`RENEWING`, unicast;
    /// `REBINDING`, broadcast); the next try at half the time left
    /// before `until`, at least a minute.
    fn send_extend(&mut self, now: u64, until_ms: u64) -> Vec<Out> {
        let Some(l) = &self.lease else {
            return self.discover(now);
        };
        let mut m = self.msg(Kind::Request, now);
        m.ciaddr = l.addr;
        let dest = if self.state == State::Renewing {
            Dest::Unicast(l.server)
        } else {
            Dest::Broadcast
        };
        let half = until_ms.saturating_sub(now) / 2;
        self.next = (now + half.max(60_000)).min(until_ms);
        vec![Out::Send(dest, m)]
    }

    /// Start (or restart) at `now`.
    pub fn start(&mut self, now: u64) -> Vec<Out> {
        self.began = now;
        self.tries = 0;
        if self.state == State::InitReboot {
            self.state = State::Rebooting;
            return self.send_reboot(now);
        }
        self.discover(now)
    }

    /// The timeouts due at `now`.
    pub fn poll(&mut self, now: u64) -> Vec<Out> {
        if now < self.next {
            return Vec::new();
        }
        match self.state {
            State::Init | State::InitReboot => self.start(now),
            State::Selecting => {
                self.tries += 1;
                self.send_discover(now)
            }
            State::Requesting => {
                self.tries += 1;
                if self.tries >= TRIES {
                    self.discover(now)
                } else {
                    self.send_select(now)
                }
            }
            State::Rebooting => {
                self.tries += 1;
                if self.tries >= TRIES {
                    self.lease = None;
                    self.discover(now)
                } else {
                    self.send_reboot(now)
                }
            }
            State::Bound | State::Renewing | State::Rebinding => self.extend(now),
            State::Released => Vec::new(),
        }
    }

    fn extend(&mut self, now: u64) -> Vec<Out> {
        let Some(l) = self.lease.clone() else {
            return self.discover(now);
        };
        let ms = |s: u64| s.saturating_mul(1000);
        if now >= ms(l.expires_at()) {
            let mut out = vec![Out::Unbind(l)];
            self.lease = None;
            out.extend(self.discover(now));
            return out;
        }
        if now >= ms(l.rebind_at()) {
            self.state = State::Rebinding;
            return self.send_extend(now, ms(l.expires_at()));
        }
        if now >= ms(l.renew_at()) {
            if self.state == State::Bound {
                self.xid = self.random();
                self.began = now;
            }
            self.state = State::Renewing;
            return self.send_extend(now, ms(l.rebind_at()));
        }
        self.next = ms(l.renew_at());
        Vec::new()
    }

    /// A message that came at `now`.
    pub fn receive(&mut self, now: u64, m: &Message) -> Vec<Out> {
        if m.op != crate::packet::BOOTREPLY || m.xid != self.xid || m.chaddr != self.cfg.mac {
            return Vec::new();
        }
        match (self.state, m.kind()) {
            (State::Selecting, Some(Kind::Offer)) if !m.yiaddr.is_unspecified() => {
                self.offer = Some(m.clone());
                self.state = State::Requesting;
                self.tries = 0;
                self.send_select(now)
            }
            (State::Requesting | State::Rebooting | State::Renewing | State::Rebinding, Some(Kind::Ack)) => {
                if self.state == State::Requesting
                    && self.offer.as_ref().and_then(|o| o.options.server_id) != m.options.server_id
                    && m.options.server_id.is_some()
                {
                    return Vec::new();
                }
                let Some(l) = Lease::from_ack(m, now / 1000) else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                if let Some(old) = &self.lease
                    && old.addr != l.addr
                    && self.state.holds()
                {
                    out.push(Out::Unbind(old.clone()));
                }
                self.lease = Some(l.clone());
                self.state = State::Bound;
                self.tries = 0;
                self.offer = None;
                self.next = l.renew_at().saturating_mul(1000);
                out.push(Out::Bind(l));
                out
            }
            (State::Requesting | State::Rebooting | State::Renewing | State::Rebinding, Some(Kind::Nak)) => {
                let mut out = Vec::new();
                if self.state.holds()
                    && let Some(old) = self.lease.take()
                {
                    out.push(Out::Unbind(old));
                }
                self.lease = None;
                self.cfg.previous = None;
                // A short pause before starting again (§3.1 step 5).
                self.state = State::Init;
                self.next = now + 2000;
                out
            }
            _ => Vec::new(),
        }
    }

    /// Give the address back (`ENDTCPIFC`): a `RELEASE` to the server
    /// and the address taken off; nothing more is sent.
    pub fn release(&mut self, now: u64) -> Vec<Out> {
        let mut out = Vec::new();
        if self.state.holds()
            && let Some(l) = self.lease.take()
        {
            let mut m = self.msg(Kind::Release, now);
            m.xid = self.random();
            m.ciaddr = l.addr;
            m.options.server_id = Some(l.server);
            out.push(Out::Send(Dest::Unicast(l.server), m));
            out.push(Out::Unbind(l));
        }
        self.state = State::Released;
        out
    }

    /// Stop without giving the address back (`ENDTCP`, power down): the
    /// address is taken off, the lease kept for the next start.
    pub fn stop(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        if self.state.holds()
            && let Some(l) = &self.lease
        {
            out.push(Out::Unbind(l.clone()));
        }
        self.state = State::Released;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::BOOTREPLY;

    const MAC: [u8; 6] = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
    const SERVER: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 2);
    const ME: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 15);
    const T0: u64 = 1_793_750_000_000;

    fn sent(out: &[Out]) -> (Dest, Message) {
        out.iter()
            .find_map(|o| match o {
                Out::Send(d, m) => Some((*d, m.clone())),
                _ => None,
            })
            .expect("a message sent")
    }

    fn reply(to: &Message, kind: Kind, lease: u32) -> Message {
        let mut m = Message::request(kind, to.xid, MAC);
        m.op = BOOTREPLY;
        if kind != Kind::Nak {
            m.yiaddr = ME;
            m.options.subnet_mask = Some(Ipv4Addr::new(255, 255, 255, 0));
            m.options.routers = vec![SERVER];
            m.options.lease_secs = Some(lease);
        }
        m.options.server_id = Some(SERVER);
        m
    }

    fn bound(lease: u32) -> (Client, Lease) {
        let mut c = Client::new(Config::new(MAC), T0, 42);
        let (d, disc) = sent(&c.start(T0));
        assert_eq!(d, Dest::Broadcast);
        assert_eq!(disc.kind(), Some(Kind::Discover));
        assert!(disc.broadcast);
        assert_eq!(disc.options.client_id.as_deref(), Some(&[1, 0x52, 0x54, 0, 0x12, 0x34, 0x56][..]));
        let (_, req) = sent(&c.receive(T0 + 10, &reply(&disc, Kind::Offer, lease)));
        assert_eq!(req.kind(), Some(Kind::Request));
        assert_eq!(req.options.requested_ip, Some(ME));
        assert_eq!(req.options.server_id, Some(SERVER));
        assert_eq!(req.xid, disc.xid);
        let out = c.receive(T0 + 20, &reply(&req, Kind::Ack, lease));
        let Some(Out::Bind(l)) = out.last() else { panic!("{out:?}") };
        assert_eq!(c.state(), State::Bound);
        (c, l.clone())
    }

    #[test]
    fn discover_offer_request_ack() {
        let (c, l) = bound(3600);
        assert_eq!(l.addr, ME);
        assert_eq!(l.prefix(), 24);
        assert_eq!(c.deadline(), (T0 + 20) / 1000 * 1000 + 1_800_000);
    }

    #[test]
    fn retransmits_with_backoff() {
        let mut c = Client::new(Config::new(MAC), T0, 7);
        let (_, d1) = sent(&c.start(T0));
        let first = c.deadline() - T0;
        assert!((3000..=5000).contains(&first), "{first}");
        assert!(c.poll(T0 + 100).is_empty());
        let (_, d2) = sent(&c.poll(c.deadline()));
        assert_eq!(d1.xid, d2.xid);
        // A reply for another transaction or client is ignored.
        let mut other = reply(&d2, Kind::Offer, 60);
        other.xid ^= 1;
        assert!(c.receive(T0, &other).is_empty());
        assert_eq!(c.state(), State::Selecting);
    }

    #[test]
    fn renew_rebind_expire() {
        let (mut c, l) = bound(1000);
        let t1 = l.renew_at() * 1000;
        let (d, renew) = sent(&c.poll(t1));
        assert_eq!(d, Dest::Unicast(SERVER));
        assert_eq!(renew.ciaddr, ME);
        assert_eq!(renew.options.requested_ip, None);
        assert_eq!(c.state(), State::Renewing);
        // Renewed: bound again, a new T1.
        let out = c.receive(t1 + 5, &reply(&renew, Kind::Ack, 1000));
        assert!(matches!(out.as_slice(), [Out::Bind(_)]));
        let l2 = c.lease().unwrap().clone();
        assert!(l2.obtained > l.obtained);
        // No answer: renewing until T2, rebinding (broadcast) until expiry.
        let t2 = l2.rebind_at() * 1000;
        let _ = c.poll(l2.renew_at() * 1000);
        let (d, _) = sent(&c.poll(t2));
        assert_eq!(d, Dest::Broadcast);
        assert_eq!(c.state(), State::Rebinding);
        let out = c.poll(l2.expires_at() * 1000);
        assert!(matches!(out.first(), Some(Out::Unbind(x)) if x.addr == ME));
        assert_eq!(c.state(), State::Selecting);
    }

    #[test]
    fn nak_restarts() {
        let (mut c, l) = bound(1000);
        let (_, renew) = sent(&c.poll(l.renew_at() * 1000));
        let out = c.receive(l.renew_at() * 1000 + 1, &reply(&renew, Kind::Nak, 0));
        assert!(matches!(out.as_slice(), [Out::Unbind(_)]));
        assert_eq!(c.state(), State::Init);
        let (_, d) = sent(&c.poll(c.deadline()));
        assert_eq!(d.kind(), Some(Kind::Discover));
    }

    #[test]
    fn init_reboot_with_a_kept_lease() {
        let (_, l) = bound(86_400);
        let mut cfg = Config::new(MAC);
        cfg.previous = Some(l);
        let later = T0 + 3_600_000;
        let mut c = Client::new(cfg.clone(), later, 9);
        assert_eq!(c.state(), State::InitReboot);
        let (d, req) = sent(&c.start(later));
        assert_eq!(d, Dest::Broadcast);
        assert_eq!(req.kind(), Some(Kind::Request));
        assert_eq!(req.options.requested_ip, Some(ME));
        assert_eq!(req.options.server_id, None);
        assert_eq!(req.ciaddr, Ipv4Addr::UNSPECIFIED);
        let out = c.receive(later + 3, &reply(&req, Kind::Ack, 86_400));
        assert!(matches!(out.as_slice(), [Out::Bind(_)]));
        // Unanswered three times: DISCOVER, asking for the old address.
        let mut c = Client::new(cfg.clone(), later, 9);
        let _ = c.start(later);
        for _ in 0..2 {
            let (_, r) = sent(&c.poll(c.deadline()));
            assert_eq!(r.kind(), Some(Kind::Request));
        }
        let (_, d) = sent(&c.poll(c.deadline()));
        assert_eq!(d.kind(), Some(Kind::Discover));
        assert_eq!(d.options.requested_ip, Some(ME));
        // An expired lease is not rebooted into.
        let c = Client::new(cfg, T0 + 90_000_000_000, 9);
        assert_eq!(c.state(), State::Init);
    }

    #[test]
    fn release_and_stop() {
        let (mut c, _) = bound(3600);
        let out = c.release(T0 + 100);
        let (d, m) = sent(&out);
        assert_eq!(d, Dest::Unicast(SERVER));
        assert_eq!(m.kind(), Some(Kind::Release));
        assert_eq!(m.ciaddr, ME);
        assert_eq!(m.options.server_id, Some(SERVER));
        assert!(matches!(out.last(), Some(Out::Unbind(_))));
        assert_eq!(c.deadline(), u64::MAX);
        let (mut c, _) = bound(3600);
        let out = c.stop();
        assert!(matches!(out.as_slice(), [Out::Unbind(_)]));
    }
}
