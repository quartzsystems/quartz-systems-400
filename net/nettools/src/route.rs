// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `TRACEROUTE` and `TRCTCPRTE`: probes with a TTL (hop limit) counting
//! up from `RANGE`'s first value, `PROBES` per hop, each answered by the
//! router where the TTL ran out (time exceeded), or by the destination
//! (an echo reply to an ICMP probe, port unreachable to a UDP one). The
//! messages follow the oracle: `TCP3260` first, then one `TCP3261` per
//! hop with the hop number, the node (its name and address in
//! parentheses, or the address alone) and each probe's time in
//! milliseconds to three significant digits, `*` for no answer.

use std::fmt::Write as _;
use std::net::IpAddr;
use std::time::Duration;

use crate::msg::{self, Kind, Msg};

/// `RANGE`, `PROBES`, `WAITTIME`, `NAMELOOKUP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The first TTL.
    pub first: u8,
    /// The last TTL.
    pub max: u8,
    /// Probes per hop.
    pub probes: u32,
    /// How long a probe waits.
    pub wait: Duration,
    /// Look the nodes' names up.
    pub lookup: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            first: 1,
            max: 30,
            probes: 3,
            wait: Duration::from_secs(3),
            lookup: true,
        }
    }
}

/// One probe's answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    /// Nothing within the wait.
    Timeout,
    /// A node answered.
    From {
        /// The node.
        addr: IpAddr,
        /// The round trip.
        rtt: Duration,
        /// The destination itself answered.
        done: bool,
        /// An unreachable mark (`H`, `N`, `P`, `X`, `!`).
        mark: Option<char>,
    },
}

/// What sends a probe and reads its answer.
pub trait Prober {
    /// Probe number `n` with TTL `ttl`, waiting up to `wait`.
    fn probe(&mut self, ttl: u8, n: u32, wait: Duration) -> Result<Answer, String>;
}

/// A time in milliseconds to three significant digits: `0.439`, `5.42`,
/// `17.3`, `123`.
pub fn millis(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms >= 100.0 {
        format!("{ms:.0}")
    } else if ms >= 10.0 {
        format!("{ms:.1}")
    } else if ms >= 1.0 {
        format!("{ms:.2}")
    } else {
        format!("{ms:.3}")
    }
}

/// The first message: where the probes go and from which interface.
pub fn probing(dest: IpAddr, name: Option<&str>, local: Option<IpAddr>, max: u8) -> Msg {
    let to = match name {
        Some(n) => format!("{n} ({dest})"),
        None => dest.to_string(),
    };
    let from = local.map_or_else(|| "*ANY".to_string(), |a| a.to_string());
    Msg::new(
        msg::PROBING,
        Kind::Info,
        format!("Tracing the route to {to} from {from} interface, at most {max} hops."),
    )
}

/// Trace the route: the hop messages, the run ending at the destination,
/// an unreachable mark or the last TTL. `names` looks a node's name up.
pub fn run(o: &Options, p: &mut dyn Prober, names: &mut dyn FnMut(IpAddr) -> Option<String>) -> Vec<Msg> {
    if o.first > o.max {
        return vec![Msg::new(
            msg::BAD_RANGE,
            Kind::Escape,
            format!("The first hop {} is past the last hop {}.", o.first, o.max),
        )];
    }
    let mut out = Vec::new();
    let mut n = 0;
    for ttl in o.first..=o.max {
        let mut line = ttl.to_string();
        let mut last: Option<IpAddr> = None;
        let mut stop = false;
        for _ in 0..o.probes {
            n += 1;
            match p.probe(ttl, n, o.wait) {
                Ok(Answer::Timeout) => line.push_str(" *"),
                Ok(Answer::From { addr, rtt, done, mark }) => {
                    if last != Some(addr) {
                        let name = if o.lookup { names(addr) } else { None };
                        match name {
                            Some(nm) => {
                                let _ = write!(line, " {nm} ({addr})");
                            }
                            None => {
                                let _ = write!(line, " {addr}");
                            }
                        }
                        last = Some(addr);
                    }
                    let _ = write!(line, " {}", millis(rtt));
                    if let Some(m) = mark {
                        let _ = write!(line, " !{m}");
                    }
                    stop |= done || mark.is_some();
                }
                Err(text) => {
                    out.push(Msg::new(msg::SOCKET_FAILED, Kind::Escape, text));
                    return out;
                }
            }
        }
        out.push(Msg::new(msg::HOP, Kind::Info, line));
        if stop {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path of routers then the destination; `None` hops never answer.
    struct Path(Vec<Option<&'static str>>);

    impl Prober for Path {
        fn probe(&mut self, ttl: u8, _: u32, _: Duration) -> Result<Answer, String> {
            let i = usize::from(ttl) - 1;
            Ok(match self.0.get(i) {
                Some(Some(a)) => Answer::From {
                    addr: a.parse().unwrap(),
                    rtt: Duration::from_micros(439),
                    done: i + 1 == self.0.len(),
                    mark: None,
                },
                _ => Answer::Timeout,
            })
        }
    }

    #[test]
    fn three_significant_digits() {
        assert_eq!(millis(Duration::from_micros(439)), "0.439");
        assert_eq!(millis(Duration::from_micros(5420)), "5.42");
        assert_eq!(millis(Duration::from_micros(17_349)), "17.3");
        assert_eq!(millis(Duration::from_millis(123)), "123");
    }

    #[test]
    fn loopback_in_one_hop() {
        let mut p = Path(vec![Some("127.0.0.1")]);
        let m = run(&Options::default(), &mut p, &mut |_| Some("LOOPBACK".into()));
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, "TCP3261");
        assert_eq!(m[0].text, "1 LOOPBACK (127.0.0.1) 0.439 0.439 0.439");
    }

    #[test]
    fn silent_hops_and_the_last_ttl() {
        let mut p = Path(vec![Some("10.0.2.2"), None, Some("192.0.2.9"), Some("192.0.2.1")]);
        let o = Options { max: 3, probes: 2, lookup: false, ..Options::default() };
        let m = run(&o, &mut p, &mut |_| None);
        let texts: Vec<&str> = m.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["1 10.0.2.2 0.439 0.439", "2 * *", "3 192.0.2.9 0.439 0.439"]);
    }

    #[test]
    fn range_checked() {
        let o = Options { first: 5, max: 2, ..Options::default() };
        let m = run(&o, &mut Path(vec![]), &mut |_| None);
        assert_eq!(m[0].id, "TCP3252");
        assert_eq!(m[0].kind, Kind::Escape);
    }

    #[test]
    fn first_message() {
        let m = probing("127.0.0.1".parse().unwrap(), None, None, 30);
        assert_eq!(m.id, "TCP3260");
        assert!(m.text.contains("127.0.0.1 from *ANY"));
    }
}
