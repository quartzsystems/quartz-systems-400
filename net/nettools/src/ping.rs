// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `PING` and `VFYTCPCNN`: echo requests one after the other, each
//! waiting up to `WAITTIME` for its reply, then the round-trip and
//! success summaries. The messages follow the oracle's sequence
//! (`TCP3203` or `TCP3204`, one `TCP3215` or `TCP3206` per packet,
//! `TCP3211`, `TCP3210`); `MSGMODE(*QUIET)` keeps the summaries only
//! and `MSGMODE(* *ESCAPE)` makes the summary the escape when a packet
//! went unanswered.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::msg::{self, Kind, Msg};

/// Whom to ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// `RMTSYS` as typed (`*INTNETADR` when the address was given).
    pub rmtsys: String,
    /// The name resolved to the address, when one was looked up.
    pub name: Option<String>,
    /// The address.
    pub addr: IpAddr,
}

/// `NBRPKT`, `PKTLEN`, `WAITTIME`, `MSGMODE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Packets to send.
    pub count: u32,
    /// Data bytes per packet.
    pub len: usize,
    /// How long each waits for its reply.
    pub wait: Duration,
    /// `*VERBOSE`: a message per packet.
    pub verbose: bool,
    /// `*ESCAPE`: the summary is the escape when a packet failed.
    pub escape: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            count: 5,
            len: 256,
            wait: Duration::from_secs(1),
            verbose: true,
            escape: false,
        }
    }
}

/// One reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Its sequence number.
    pub seq: u16,
    /// Who sent it.
    pub from: IpAddr,
    /// The TTL or hop limit it arrived with.
    pub ttl: Option<u8>,
    /// Its data bytes.
    pub len: usize,
}

/// What sends the requests and reads the replies.
pub trait Echoer {
    /// Send request `seq` with `len` data bytes.
    fn send(&mut self, seq: u16, len: usize) -> Result<(), String>;
    /// The next reply, waiting until `deadline`; `None` when none came.
    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, String>;
}

/// What the run produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The messages, in order; the last is the summary.
    pub msgs: Vec<Msg>,
    /// Replies received.
    pub ok: u32,
}

impl Outcome {
    /// The escape, when the run ended with one.
    pub fn escape(&self) -> Option<&Msg> {
        self.msgs.last().filter(|m| m.kind == Kind::Escape)
    }
}

fn first(t: &Target) -> Msg {
    match &t.name {
        Some(n) => Msg::new(
            msg::VERIFY_AT,
            Kind::Info,
            format!("Checking the connection to host {n} at {}.", t.addr),
        ),
        None => Msg::new(msg::VERIFY, Kind::Info, format!("Checking the connection to host {}.", t.rmtsys)),
    }
}

fn reply_msg(n: u32, ms: u128, r: &Reply, dup: bool) -> Msg {
    let ttl = r.ttl.map_or_else(|| "?".to_string(), |t| t.to_string());
    let (id, limit) = match (r.from.is_ipv6(), dup) {
        (false, false) => (msg::REPLY, "TTL"),
        (false, true) => (msg::REPLY_DUP, "TTL"),
        (true, false) => (msg::REPLY6, "hop limit"),
        (true, true) => (msg::REPLY6_DUP, "hop limit"),
    };
    let tail = if dup { " (a duplicate)" } else { "" };
    Msg::new(
        id,
        Kind::Info,
        format!("Reply {n} from {} in {ms} ms: {} bytes, {limit} {ttl}.{tail}", r.from, r.len),
    )
}

/// Ping `t` through `e`; `clock` reads the time (a fake one in tests).
pub fn run(t: &Target, o: &Options, e: &mut dyn Echoer, clock: &mut dyn FnMut() -> Instant) -> Outcome {
    let mut msgs = vec![first(t)];
    let mut times: Vec<u128> = Vec::new();
    // When each packet went, and whether it was answered, by number.
    let mut sent: Vec<(Instant, bool)> = Vec::new();
    for n in 1..=o.count {
        let seq = u16::try_from(n).unwrap_or(u16::MAX);
        let start = clock();
        sent.push((start, false));
        if let Err(text) = e.send(seq, o.len) {
            msgs.push(Msg::new(msg::SOCKET_FAILED, Kind::Diag, text));
            continue;
        }
        let deadline = start + o.wait;
        loop {
            match e.recv(deadline) {
                Ok(Some(r)) => {
                    let Some(slot) = sent.get_mut(usize::from(r.seq).wrapping_sub(1)) else {
                        continue;
                    };
                    let ms = clock().saturating_duration_since(slot.0).as_millis();
                    if slot.1 {
                        if o.verbose {
                            msgs.push(reply_msg(u32::from(r.seq), ms, &r, true));
                        }
                        continue;
                    }
                    if r.seq != seq {
                        // A late reply to a packet already given up on.
                        continue;
                    }
                    slot.1 = true;
                    times.push(ms);
                    if o.verbose {
                        msgs.push(reply_msg(n, ms, &r, false));
                    }
                    break;
                }
                Ok(None) => break,
                Err(text) => {
                    msgs.push(Msg::new(msg::SOCKET_FAILED, Kind::Diag, text));
                    break;
                }
            }
        }
        if !sent[sent.len() - 1].1 && o.verbose {
            msgs.push(Msg::new(
                msg::NO_REPLY,
                Kind::Diag,
                format!("No reply within {} seconds to packet {n}.", o.wait.as_secs().max(1)),
            ));
        }
    }
    let ok = u32::try_from(times.len()).unwrap_or(u32::MAX);
    if let (Some(min), Some(max)) = (times.iter().min(), times.iter().max()) {
        let avg = times.iter().sum::<u128>() / times.len() as u128;
        msgs.push(Msg::new(
            msg::ROUND_TRIP,
            Kind::Info,
            format!("Round trip in milliseconds, least/average/most: {min}/{avg}/{max}."),
        ));
    }
    let pct = (ok * 100).checked_div(o.count).unwrap_or(0);
    let kind = if o.escape && ok < o.count { Kind::Escape } else { Kind::Comp };
    msgs.push(Msg::new(
        msg::STATS,
        kind,
        format!("{ok} of {} packets answered ({pct} %).", o.count),
    ));
    Outcome { msgs, ok }
}

/// `PKTLEN`'s data bytes for an address family: the IP and ICMP
/// headers must fit in 65535.
pub fn max_len(v6: bool) -> usize {
    if v6 { 65_535 - 40 - 8 } else { 65_535 - 20 - 8 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Replies scripted per packet: `Some(ms)` answers after that long;
    /// with `dup` the first reply comes twice.
    struct Fake {
        plan: VecDeque<Option<u64>>,
        queue: VecDeque<(u16, u64)>,
        now: std::rc::Rc<std::cell::Cell<Instant>>,
        dup: bool,
    }

    impl Echoer for Fake {
        fn send(&mut self, seq: u16, _: usize) -> Result<(), String> {
            if let Some(ms) = self.plan.pop_front().flatten() {
                self.queue.push_back((seq, ms));
                if std::mem::take(&mut self.dup) {
                    self.queue.push_back((seq, 0));
                }
            }
            Ok(())
        }
        fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, String> {
            if let Some((seq, ms)) = self.queue.pop_front() {
                self.now.set(self.now.get() + Duration::from_millis(ms));
                Ok(Some(Reply { seq, from: "127.0.0.1".parse().unwrap(), ttl: Some(64), len: 256 }))
            } else {
                self.now.set(deadline);
                Ok(None)
            }
        }
    }

    fn ping(plan: &[Option<u64>], o: Options, dup: bool) -> Outcome {
        let now = std::rc::Rc::new(std::cell::Cell::new(Instant::now()));
        let mut f = Fake { plan: plan.iter().copied().collect(), queue: VecDeque::new(), now: now.clone(), dup };
        let t = Target { rmtsys: "*INTNETADR".into(), name: None, addr: "127.0.0.1".parse().unwrap() };
        run(&t, &o, &mut f, &mut || now.get())
    }

    fn ids(o: &Outcome) -> Vec<&str> {
        o.msgs.iter().map(|m| m.id).collect()
    }

    #[test]
    fn all_answered() {
        let o = ping(&[Some(0), Some(2), Some(4)], Options { count: 3, ..Options::default() }, false);
        assert_eq!(ids(&o), ["TCP3204", "TCP3215", "TCP3215", "TCP3215", "TCP3211", "TCP3210"]);
        assert_eq!(o.ok, 3);
        assert!(o.msgs[4].text.ends_with("0/2/4."), "{}", o.msgs[4].text);
        assert!(o.msgs[5].text.starts_with("3 of 3 packets answered (100 %)"));
        assert_eq!(o.escape(), None);
        assert!(o.msgs[2].text.contains("Reply 2 from 127.0.0.1 in 2 ms: 256 bytes, TTL 64"));
    }

    #[test]
    fn timeouts_as_the_oracle_sends_them() {
        let o = ping(&[None, None], Options { count: 2, ..Options::default() }, false);
        assert_eq!(ids(&o), ["TCP3204", "TCP3206", "TCP3206", "TCP3210"]);
        assert_eq!(o.msgs[3].kind, Kind::Comp);
        let o = ping(&[None], Options { count: 1, verbose: false, escape: true, ..Options::default() }, false);
        assert_eq!(ids(&o), ["TCP3204", "TCP3210"]);
        assert_eq!(o.escape().unwrap().text, "0 of 1 packets answered (0 %).");
    }

    #[test]
    fn quiet_keeps_the_summaries() {
        let o = ping(&[Some(1), Some(1)], Options { count: 2, verbose: false, ..Options::default() }, false);
        assert_eq!(ids(&o), ["TCP3204", "TCP3211", "TCP3210"]);
    }

    #[test]
    fn a_duplicate_is_marked() {
        let o = ping(&[Some(1), Some(1)], Options { count: 2, ..Options::default() }, true);
        assert_eq!(ids(&o), ["TCP3204", "TCP3215", "TCP3216", "TCP3215", "TCP3211", "TCP3210"]);
    }

    #[test]
    fn named_target() {
        let t = Target { rmtsys: "LOOPBACK".into(), name: Some("LOOPBACK".into()), addr: "127.0.0.1".parse().unwrap() };
        let m = first(&t);
        assert_eq!(m.id, "TCP3203");
        assert!(m.text.contains("LOOPBACK at 127.0.0.1"));
    }
}
