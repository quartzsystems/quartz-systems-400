// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The names the server holds as a B node (RFC 1001 15, RFC 1002 5.1.1),
//! as a state machine without sockets: [`NameService`] says what to send
//! for each packet heard and each moment passed.
//!
//! - Registration: each name is claimed with three broadcast NAME
//!   REGISTRATION REQUESTs 250 ms apart; when no node objects, a NAME
//!   OVERWRITE DEMAND makes it held. An objection (a negative registration
//!   response) or a NAME CONFLICT DEMAND puts the name in conflict: it is
//!   neither answered for nor defended.
//! - Answers: a NAME QUERY REQUEST for a held name gets a positive
//!   response; a unicast one for another name a negative one; a broadcast
//!   one for another name nothing. A NODE STATUS REQUEST for `*` or a held
//!   name gets the held names.
//! - Defence: another node's registration of a unique name the server
//!   holds gets a negative registration response (`ACT_ERR`).
//! - Refresh: a B node has no name server to refresh with; at half the
//!   TTL each held name is claimed again with a broadcast NAME REFRESH
//!   REQUEST.
//! - Release: at the end, a broadcast NAME RELEASE REQUEST for each held
//!   name.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use super::packet::{
    Header, NAME_ACTIVE, NAME_GROUP, NB_GROUP, OP_MULTIHOMED, OP_QUERY, OP_REFRESH,
    OP_REGISTRATION, OP_RELEASE, Packet, Question, RCODE_ACT_ERR, RCODE_CFT_ERR, RCODE_NAM_ERR,
    Resource, TYPE_NB, TYPE_NBSTAT, TYPE_NULL, node_status,
};
use crate::nbss::{NbName, SUFFIX_SERVER, SUFFIX_WORKSTATION};

/// `BCAST_REQ_RETRY_TIMEOUT` (RFC 1002 6).
pub const RETRY: Duration = Duration::from_millis(250);
/// `BCAST_REQ_RETRY_COUNT` (RFC 1002 6).
pub const RETRIES: u8 = 3;
/// `DEFAULT_TTL` (RFC 1002 6): 3.5 days.
pub const DEFAULT_TTL: u32 = 302_400;

/// Where a packet goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    /// The broadcast address on UDP 137.
    Broadcast,
    /// One node.
    To(SocketAddrV4),
}

/// A packet to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Out {
    /// Where.
    pub dest: Dest,
    /// The packet.
    pub packet: Packet,
}

/// What the server registers.
#[derive(Debug, Clone)]
pub struct NameConfig {
    /// The server name: `<00>` and `<20>`, unique.
    pub server: String,
    /// The workgroup (the domain attribute): `<00>`, group; none when
    /// empty or the server's own name.
    pub workgroup: String,
    /// The TTL given in registrations and answers, in seconds.
    pub ttl: u32,
    /// The first transaction ID used.
    pub first_id: u16,
}

impl NameConfig {
    /// The names of `server` in `workgroup` with the RFC's TTL.
    #[must_use]
    pub fn new(server: &str, workgroup: &str) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        Self {
            server: server.to_owned(),
            workgroup: workgroup.to_owned(),
            ttl: DEFAULT_TTL,
            first_id: ((seed ^ std::process::id()) & 0xFFFF) as u16,
        }
    }
}

/// Where one name stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameState {
    /// Being claimed: requests sent, the transaction, the next step.
    Registering {
        /// Requests sent.
        sent: u8,
        /// Their transaction ID.
        id: u16,
        /// When the next is due.
        next: Instant,
    },
    /// Held; claimed again at `refresh`.
    Held {
        /// When.
        refresh: Instant,
    },
    /// Another node has it.
    Conflict,
    /// Released.
    Released,
}

/// One name of the server's.
#[derive(Debug, Clone)]
struct Owned {
    name: NbName,
    group: bool,
    state: NameState,
}

impl Owned {
    fn held(&self) -> bool {
        matches!(self.state, NameState::Held { .. })
    }

    fn nb_flags(&self) -> u16 {
        if self.group { NB_GROUP } else { 0 }
    }
}

/// The server's names on the network.
#[derive(Debug)]
pub struct NameService {
    names: Vec<Owned>,
    ttl: u32,
    next_id: u16,
    /// Transactions this service sent, to know its own broadcasts.
    sent: VecDeque<u16>,
}

impl NameService {
    /// The service for `cfg`, nothing sent yet.
    #[must_use]
    pub fn new(cfg: &NameConfig) -> Self {
        let mut names = Vec::new();
        let start = NameState::Released;
        for suffix in [SUFFIX_WORKSTATION, SUFFIX_SERVER] {
            names.push(Owned {
                name: NbName::new(&cfg.server, suffix),
                group: false,
                state: start,
            });
        }
        if !cfg.workgroup.is_empty() && !cfg.workgroup.eq_ignore_ascii_case(&cfg.server) {
            names.push(Owned {
                name: NbName::new(&cfg.workgroup, SUFFIX_WORKSTATION),
                group: true,
                state: start,
            });
        }
        Self {
            names,
            ttl: cfg.ttl,
            next_id: cfg.first_id,
            sent: VecDeque::new(),
        }
    }

    fn id(&mut self) -> u16 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.sent.push_back(id);
        if self.sent.len() > 64 {
            self.sent.pop_front();
        }
        id
    }

    /// Each name and where it stands.
    #[must_use]
    pub fn names(&self) -> Vec<(NbName, NameState)> {
        self.names
            .iter()
            .map(|o| (o.name.clone(), o.state))
            .collect()
    }

    /// A broadcast claim (`opcode`, with `RD` for a request) of name `i`.
    fn claim(&mut self, i: usize, opcode: u8, rd: bool, me: Ipv4Addr, id: u16) -> Out {
        let o = &self.names[i];
        Out {
            dest: Dest::Broadcast,
            packet: Packet {
                header: Header {
                    id,
                    opcode,
                    rd,
                    broadcast: true,
                    ..Header::default()
                },
                questions: vec![Question {
                    name: o.name.clone(),
                    qtype: TYPE_NB,
                }],
                additional: vec![Resource::nb(o.name.clone(), self.ttl, o.nb_flags(), me)],
                ..Packet::default()
            },
        }
    }

    /// Starts claiming every name as `me`.
    pub fn start(&mut self, now: Instant, me: Ipv4Addr) -> Vec<Out> {
        let mut out = Vec::new();
        for i in 0..self.names.len() {
            let id = self.id();
            self.names[i].state = NameState::Registering {
                sent: 1,
                id,
                next: now + RETRY,
            };
            out.push(self.claim(i, OP_REGISTRATION, true, me, id));
        }
        out
    }

    /// When [`NameService::tick`] next has something to do.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        self.names
            .iter()
            .filter_map(|o| match o.state {
                NameState::Registering { next, .. } => Some(next),
                NameState::Held { refresh } => Some(refresh),
                _ => None,
            })
            .min()
    }

    /// What is due at `now`: the next registration request, the overwrite
    /// demand that makes a name held, a refresh.
    pub fn tick(&mut self, now: Instant, me: Ipv4Addr) -> Vec<Out> {
        let mut out = Vec::new();
        let half = Duration::from_secs(u64::from(self.ttl / 2).max(1));
        for i in 0..self.names.len() {
            match self.names[i].state {
                NameState::Registering { sent, id, next } if next <= now => {
                    if sent < RETRIES {
                        self.names[i].state = NameState::Registering {
                            sent: sent + 1,
                            id,
                            next: now + RETRY,
                        };
                        out.push(self.claim(i, OP_REGISTRATION, true, me, id));
                    } else {
                        let id = self.id();
                        out.push(self.claim(i, OP_REGISTRATION, false, me, id));
                        self.names[i].state = NameState::Held {
                            refresh: now + half,
                        };
                    }
                }
                NameState::Held { refresh } if refresh <= now => {
                    let id = self.id();
                    out.push(self.claim(i, OP_REFRESH, false, me, id));
                    self.names[i].state = NameState::Held {
                        refresh: now + half,
                    };
                }
                _ => {}
            }
        }
        out
    }

    /// Releases every held name.
    pub fn stop(&mut self, me: Ipv4Addr) -> Vec<Out> {
        let mut out = Vec::new();
        for i in 0..self.names.len() {
            if self.names[i].held() {
                let id = self.id();
                out.push(self.claim(i, OP_RELEASE, false, me, id));
            }
            self.names[i].state = NameState::Released;
        }
        out
    }

    /// The held name `n` (with no scope), if any.
    fn held(&self, n: &NbName) -> Option<&Owned> {
        if !n.scope.is_empty() {
            return None;
        }
        self.names
            .iter()
            .find(|o| o.held() && o.name.is(&n.name, n.suffix))
    }

    /// What to answer `bytes` heard from `from`; `me` is this node's
    /// address towards it.
    pub fn handle(&mut self, from: SocketAddrV4, bytes: &[u8], me: Ipv4Addr) -> Vec<Out> {
        let Ok(p) = Packet::parse(bytes) else {
            return Vec::new();
        };
        let h = p.header;
        if h.response {
            if h.opcode == OP_REGISTRATION && h.rcode != 0 {
                self.objection(&p);
            }
            return Vec::new();
        }
        // This service's own broadcasts come back to it.
        if self.sent.contains(&h.id) {
            return Vec::new();
        }
        let Some(q) = p.questions.first() else {
            return Vec::new();
        };
        match h.opcode {
            OP_QUERY if q.qtype == TYPE_NB => self.query(from, &h, q, me),
            OP_QUERY if q.qtype == TYPE_NBSTAT => self.status(from, &h, q),
            OP_REGISTRATION | OP_MULTIHOMED => self.defend(from, &h, q, me),
            _ => Vec::new(),
        }
    }

    /// A negative registration response or a conflict demand.
    fn objection(&mut self, p: &Packet) {
        let id = p.header.id;
        let conflict = p.header.rcode == RCODE_CFT_ERR;
        let named = p.answers.first().map(|r| &r.name);
        for o in &mut self.names {
            let ours = match o.state {
                NameState::Registering { id: mine, .. } => mine == id,
                NameState::Held { .. } => conflict && named == Some(&o.name),
                _ => false,
            };
            if ours && named.is_none_or(|n| o.name.is(&n.name, n.suffix)) {
                o.state = NameState::Conflict;
            }
        }
    }

    fn reply(h: &Header, rcode: u8, answer: Resource) -> Packet {
        Packet {
            header: Header {
                id: h.id,
                response: true,
                opcode: h.opcode,
                aa: true,
                rd: h.rd,
                rcode,
                ..Header::default()
            },
            answers: vec![answer],
            ..Packet::default()
        }
    }

    fn query(&self, from: SocketAddrV4, h: &Header, q: &Question, me: Ipv4Addr) -> Vec<Out> {
        let packet = if let Some(o) = self.held(&q.name) {
            Self::reply(
                h,
                0,
                Resource::nb(q.name.clone(), self.ttl, o.nb_flags(), me),
            )
        } else if !h.broadcast {
            let none = Resource {
                name: q.name.clone(),
                rtype: TYPE_NULL,
                ttl: 0,
                rdata: Vec::new(),
            };
            Self::reply(h, RCODE_NAM_ERR, none)
        } else {
            return Vec::new();
        };
        vec![Out {
            dest: Dest::To(from),
            packet,
        }]
    }

    fn status(&self, from: SocketAddrV4, h: &Header, q: &Question) -> Vec<Out> {
        let wildcard = q.name.name == "*" && q.name.scope.is_empty();
        if !wildcard && self.held(&q.name).is_none() {
            return Vec::new();
        }
        let names: Vec<(NbName, u16)> = self
            .names
            .iter()
            .filter(|o| o.held())
            .map(|o| {
                let g = if o.group { NAME_GROUP } else { 0 };
                (o.name.clone(), g | NAME_ACTIVE)
            })
            .collect();
        let mut packet = Self::reply(
            h,
            0,
            Resource {
                name: q.name.clone(),
                rtype: TYPE_NBSTAT,
                ttl: 0,
                rdata: node_status(&names, [0; 6]),
            },
        );
        packet.header.rd = false;
        vec![Out {
            dest: Dest::To(from),
            packet,
        }]
    }

    fn defend(&self, from: SocketAddrV4, h: &Header, q: &Question, me: Ipv4Addr) -> Vec<Out> {
        let Some(o) = self.held(&q.name).filter(|o| !o.group) else {
            return Vec::new();
        };
        let mut packet = Self::reply(
            h,
            RCODE_ACT_ERR,
            Resource::nb(q.name.clone(), 0, o.nb_flags(), me),
        );
        packet.header.opcode = OP_REGISTRATION;
        packet.header.rd = true;
        packet.header.ra = true;
        vec![Out {
            dest: Dest::To(from),
            packet,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 15);
    const PEER: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(10, 0, 2, 2), 137);

    fn service(now: Instant) -> NameService {
        let mut cfg = NameConfig::new("QSTEST", "WORKGROUP");
        cfg.first_id = 100;
        cfg.ttl = 60;
        let mut s = NameService::new(&cfg);
        let out = s.start(now, ME);
        assert_eq!(out.len(), 3);
        s
    }

    /// Ticks through registration; the claims sent.
    fn register(s: &mut NameService, now: Instant) -> Vec<Out> {
        let mut all = Vec::new();
        for k in 1..=3 {
            all.extend(s.tick(now + RETRY * k, ME));
        }
        all
    }

    fn query(name: &str, suffix: u8, qtype: u16, broadcast: bool) -> Vec<u8> {
        Packet {
            header: Header {
                id: 7,
                opcode: OP_QUERY,
                rd: true,
                broadcast,
                ..Header::default()
            },
            questions: vec![Question {
                name: NbName::new(name, suffix),
                qtype,
            }],
            ..Packet::default()
        }
        .encode()
    }

    #[test]
    fn names_are_claimed_three_times_then_demanded() {
        let now = Instant::now();
        let mut s = service(now);
        let out = register(&mut s, now);
        // Two more requests each, then an overwrite demand (RD clear).
        assert_eq!(out.len(), 9);
        let demands: Vec<_> = out.iter().filter(|o| !o.packet.header.rd).collect();
        assert_eq!(demands.len(), 3);
        for o in &out {
            assert_eq!(o.dest, Dest::Broadcast);
            assert!(o.packet.header.broadcast);
            assert_eq!(o.packet.additional[0].addresses()[0].1, ME);
        }
        let group = out
            .iter()
            .find(|o| o.packet.questions[0].name.name == "WORKGROUP")
            .unwrap();
        assert_eq!(group.packet.additional[0].addresses()[0].0, NB_GROUP);
        assert!(
            s.names()
                .iter()
                .all(|(_, st)| matches!(st, NameState::Held { .. }))
        );
    }

    #[test]
    fn queries_and_node_status_are_answered() {
        let now = Instant::now();
        let mut s = service(now);
        // Not answered while registering.
        assert!(
            s.handle(PEER, &query("QSTEST", 0x20, TYPE_NB, true), ME)
                .is_empty()
        );
        register(&mut s, now);
        let out = s.handle(PEER, &query("qstest", 0x20, TYPE_NB, true), ME);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].dest, Dest::To(PEER));
        let h = out[0].packet.header;
        assert!(h.response && h.aa && h.rd && h.rcode == 0 && h.id == 7);
        assert_eq!(out[0].packet.answers[0].addresses(), [(0, ME)]);
        assert_eq!(out[0].packet.answers[0].ttl, 60);
        // Another name: nothing to a broadcast, NAM_ERR to a unicast.
        assert!(
            s.handle(PEER, &query("OTHER", 0x20, TYPE_NB, true), ME)
                .is_empty()
        );
        let out = s.handle(PEER, &query("OTHER", 0x20, TYPE_NB, false), ME);
        assert_eq!(out[0].packet.header.rcode, RCODE_NAM_ERR);
        assert_eq!(out[0].packet.answers[0].rtype, TYPE_NULL);
        // Node status of `*`.
        let out = s.handle(PEER, &query("*", 0, TYPE_NBSTAT, false), ME);
        let r = &out[0].packet.answers[0];
        assert_eq!(r.rtype, TYPE_NBSTAT);
        assert_eq!(r.name.name, "*");
        assert_eq!(r.rdata[0], 3);
        assert_eq!(&r.rdata[1..17], b"QSTEST         \x00");
        assert_eq!(&r.rdata[17..19], [0x04, 0x00]);
        assert_eq!(&r.rdata[37..53], b"WORKGROUP      \x00");
        assert_eq!(&r.rdata[53..55], [0x84, 0x00]);
        assert_eq!(r.rdata.len(), 1 + 3 * 18 + 46);
    }

    #[test]
    fn unique_names_are_defended_and_objections_heeded() {
        let now = Instant::now();
        let mut s = service(now);
        register(&mut s, now);
        let mut reg = Packet::parse(&query("QSTEST", 0x20, TYPE_NB, true)).unwrap();
        reg.header.opcode = OP_REGISTRATION;
        reg.additional = vec![Resource::nb(NbName::new("QSTEST", 0x20), 60, 0, *PEER.ip())];
        let out = s.handle(PEER, &reg.encode(), ME);
        assert_eq!(out.len(), 1);
        let h = out[0].packet.header;
        assert_eq!(
            (h.response, h.opcode, h.rcode),
            (true, OP_REGISTRATION, RCODE_ACT_ERR)
        );
        assert_eq!(out[0].packet.answers[0].addresses(), [(0, ME)]);
        // The group name is not defended.
        reg.questions[0].name = NbName::new("WORKGROUP", 0);
        assert!(s.handle(PEER, &reg.encode(), ME).is_empty());
        // A conflict demand for QSTEST<20> takes it away.
        let demand = Packet {
            header: Header {
                id: 9,
                response: true,
                opcode: OP_REGISTRATION,
                rcode: RCODE_CFT_ERR,
                ..Header::default()
            },
            answers: vec![Resource::nb(NbName::new("QSTEST", 0x20), 0, 0, ME)],
            ..Packet::default()
        };
        assert!(s.handle(PEER, &demand.encode(), ME).is_empty());
        assert!(
            s.handle(PEER, &query("QSTEST", 0x20, TYPE_NB, true), ME)
                .is_empty()
        );
        assert_eq!(
            s.handle(PEER, &query("QSTEST", 0, TYPE_NB, true), ME).len(),
            1
        );
    }

    #[test]
    fn an_objection_while_registering_stops_the_claim() {
        let now = Instant::now();
        let mut s = service(now);
        // The first claim (QSTEST<00>) used transaction 100.
        let neg = Packet {
            header: Header {
                id: 100,
                response: true,
                opcode: OP_REGISTRATION,
                rcode: RCODE_ACT_ERR,
                ..Header::default()
            },
            answers: vec![Resource::nb(NbName::new("QSTEST", 0), 0, 0, *PEER.ip())],
            ..Packet::default()
        };
        s.handle(PEER, &neg.encode(), ME);
        assert_eq!(s.names()[0].1, NameState::Conflict);
        assert_eq!(register(&mut s, now).len(), 6);
    }

    #[test]
    fn own_broadcasts_refresh_and_release() {
        let now = Instant::now();
        let mut s = service(now);
        let out = register(&mut s, now);
        // A claim of its own heard back is not defended against.
        assert!(s.handle(PEER, &out[8].packet.encode(), ME).is_empty());
        assert_eq!(
            s.next_wake(),
            Some(now + RETRY * 3 + Duration::from_secs(30))
        );
        let refresh = s.tick(now + Duration::from_secs(31), ME);
        assert_eq!(refresh.len(), 3);
        assert!(refresh.iter().all(|o| o.packet.header.opcode == OP_REFRESH));
        let rel = s.stop(ME);
        assert_eq!(rel.len(), 3);
        assert!(
            rel.iter()
                .all(|o| o.packet.header.opcode == OP_RELEASE && o.dest == Dest::Broadcast)
        );
        assert!(s.next_wake().is_none());
    }
}
