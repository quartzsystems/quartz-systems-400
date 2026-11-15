// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service port's protocol from the system's side, without I/O:
//! events in (the peer connected or went away, its bytes, `slic`'s
//! notices, the clock ticking), whole frames out, each to be written to
//! the port in one write.

use qsipart::service::{
    Ack, FromSystem, Hello, Keylock, PeerHello, Pong, PowerDown, Refcode, Refused, State, SysState,
    ToSystem, msgid, panel,
};
use qsipart::{Frame, Reader, caps};

use super::{Notice, text};
use crate::status::Status;

/// How long `slic` has to answer a panel function.
pub const ANSWER_MS: u64 = 10_000;
/// Before this (2020-01-01) the system's clock was never set.
const CLOCK_UNSET_MS: u64 = 1_577_836_800_000;

/// What the protocol asks of the system.
pub trait Host {
    /// The IP addresses as `address/prefix`, loopback left out.
    fn addresses(&mut self) -> Vec<String>;
    /// The clock, Unix milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Start `PWRDWNSYS` with these options.
    ///
    /// # Errors
    /// It could not be asked for.
    fn power_down(&mut self, p: &PowerDown) -> Result<(), String>;
    /// Ask `slic` for function 21; its answer comes as a
    /// [`Notice::Answer`].
    fn ask_dst(&mut self, id: u32);
    /// Set the system's clock.
    ///
    /// # Errors
    /// It could not be set.
    fn set_clock(&mut self, unix_ns: i64) -> Result<(), String>;
    /// A line for the serial log.
    fn log(&mut self, line: &str);
}

/// The service port's state.
pub struct Service<H: Host> {
    /// The system.
    pub host: H,
    status: Status,
    reader: Reader,
    connected: bool,
    peer: Option<PeerHello>,
    /// The state last sent this connection.
    sent: Option<State>,
    /// Panel requests `slic` has not answered: id and when asked.
    pending: Vec<(u32, u64)>,
}

fn sys_state(s: &str) -> SysState {
    match s {
        "complete" => SysState::Complete,
        "ending" => SysState::Ending,
        "down" => SysState::Down,
        _ => SysState::Ipl,
    }
}

fn frame(m: &FromSystem) -> Vec<u8> {
    // Every message the system sends is far below a frame's limit.
    m.encode().unwrap_or_default()
}

fn refused(id: u32, msgid: &str, text: impl Into<String>) -> Vec<u8> {
    frame(&FromSystem::Refused(Refused {
        id,
        msgid: msgid.into(),
        text: text.into(),
    }))
}

impl<H: Host> Service<H> {
    /// With `slic`'s state as the file holds it now.
    pub fn new(host: H, status: Status) -> Self {
        Self {
            host,
            status,
            reader: Reader::new(),
            connected: false,
            peer: None,
            sent: None,
            pending: Vec::new(),
        }
    }

    /// The state as last pushed.
    #[must_use]
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Whether a peer is connected.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The peer's hello, once it said it.
    #[must_use]
    pub fn peer(&self) -> Option<&PeerHello> {
        self.peer.as_ref()
    }

    fn hello(&self) -> FromSystem {
        let s = &self.status;
        FromSystem::Hello(Hello {
            version: qsipart::VERSION,
            caps: caps::SERVICE_V1,
            system: s.system.clone(),
            release: s.release.clone(),
            serial: s.serial.clone(),
            slot: s.slot.clone(),
            ipl_type: s.ipl_type.clone(),
            keylock: match s.mode.as_str() {
                "manual" => Keylock::Manual,
                _ => Keylock::Normal,
            },
        })
    }

    fn refcode(&self, current: bool) -> Option<Vec<u8>> {
        let s = &self.status;
        (!s.src.is_empty()).then(|| {
            frame(&FromSystem::Refcode(Refcode {
                code: s.src.clone(),
                words: Vec::new(),
                text: s.src_text.clone(),
                time_ms: s.src_time,
                current,
            }))
        })
    }

    fn state_now(&mut self) -> State {
        let state = sys_state(&self.status.state);
        // Addresses mean something once the IPL has made them.
        let addresses = if matches!(state, SysState::Complete | SysState::Ending) {
            self.host.addresses()
        } else {
            Vec::new()
        };
        State {
            state,
            fix_level: self.status.fix_level.clone(),
            addresses,
        }
    }

    /// The state if it differs from the one last sent.
    fn state_if_changed(&mut self) -> Option<Vec<u8>> {
        let now = self.state_now();
        if self.sent.as_ref() == Some(&now) {
            return None;
        }
        let f = frame(&FromSystem::State(now.clone()));
        self.sent = Some(now);
        Some(f)
    }

    /// Take `slic`'s new state; the hello again when what it says
    /// changed (the keylock and the serial number once the LIC record is
    /// read, the system name once the system values are).
    fn adopt(&mut self, status: &str) -> Option<Vec<u8>> {
        let before = self.hello();
        self.status = Status::parse(status);
        let now = self.hello();
        (now != before).then(|| frame(&now))
    }

    /// A peer connected: the hello, the current code, the state.
    pub fn connect(&mut self) -> Vec<Vec<u8>> {
        self.connected = true;
        self.reader.reset();
        self.peer = None;
        self.sent = None;
        let mut out = vec![frame(&self.hello())];
        out.extend(self.refcode(true));
        out.extend(self.state_if_changed());
        out
    }

    /// The peer went away. A request it made is still carried out; its
    /// answer is not sent.
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.reader.reset();
        self.peer = None;
    }

    /// What `slic` said.
    pub fn notice(&mut self, n: Notice) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match n {
            Notice::Posted { status } => {
                out.extend(self.adopt(&status));
                out.extend(self.refcode(false));
                out.extend(self.state_if_changed());
            }
            Notice::Changed { status } => {
                out.extend(self.adopt(&status));
                out.extend(self.state_if_changed());
            }
            Notice::Answer { id, refusal } => {
                let Some(at) = self.pending.iter().position(|(p, _)| *p == id) else {
                    return out;
                };
                self.pending.remove(at);
                out.push(match refusal {
                    None => frame(&FromSystem::Ack(Ack { id })),
                    Some((m, t)) => refused(id, &m, t),
                });
            }
            // The caller answers once what is queued is written.
            Notice::Flush { .. } => {}
        }
        if self.connected { out } else { Vec::new() }
    }

    /// Bytes from the peer.
    pub fn input(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.reader.push(bytes);
        let mut out = Vec::new();
        while let Some(f) = self.reader.next_frame() {
            out.extend(self.request(&f));
        }
        out
    }

    /// Once a second or so: a state whose addresses changed, panel
    /// requests `slic` never answered.
    pub fn tick(&mut self) -> Vec<Vec<u8>> {
        let now = self.host.now_ms();
        let mut out = Vec::new();
        let (late, waiting): (Vec<_>, Vec<_>) = self
            .pending
            .iter()
            .partition(|(_, at)| now.saturating_sub(*at) > ANSWER_MS);
        self.pending = waiting;
        for (id, _) in late {
            out.push(refused(id, msgid::DST_BUSY, text::DST_NO_ANSWER));
        }
        if self.connected {
            out.extend(self.state_if_changed());
            out
        } else {
            Vec::new()
        }
    }

    fn request(&mut self, f: &Frame) -> Vec<Vec<u8>> {
        let m = match ToSystem::decode(f) {
            Ok(Some(m)) => m,
            Ok(None) => {
                // A peer's kind (0x01nn) this version does not know.
                return if f.kind & 0xFF00 == 0x0100 {
                    vec![refused(
                        ToSystem::request_id(f),
                        msgid::UNKNOWN,
                        text::unknown(f.kind),
                    )]
                } else {
                    Vec::new()
                };
            }
            Err(e) => {
                return vec![refused(
                    ToSystem::request_id(f),
                    msgid::BAD,
                    text::bad(&e.to_string()),
                )];
            }
        };
        match m {
            ToSystem::Hello(h) => {
                self.host.log(&format!(
                    "qsid: peer {} (version {}, capabilities {:#x})",
                    h.name, h.version, h.caps
                ));
                let wants_time = h.caps & caps::TIME != 0;
                self.peer = Some(h);
                if wants_time && self.host.now_ms() < CLOCK_UNSET_MS {
                    return vec![frame(&FromSystem::TimeRequest)];
                }
                Vec::new()
            }
            ToSystem::PowerDown(p) => vec![self.power_down(&p)],
            ToSystem::Panel(p) => {
                if p.function != panel::DST {
                    return vec![refused(p.id, msgid::FUNCTION, text::function(p.function))];
                }
                // slic answers again; these two it would only come to
                // once the IPL let it.
                if self.status.mode != "manual" {
                    return vec![refused(p.id, msgid::NOT_MANUAL, text::NOT_MANUAL)];
                }
                if self.status.state != "complete" {
                    return vec![refused(p.id, msgid::DST_BUSY, text::DST_IPL)];
                }
                let now = self.host.now_ms();
                self.pending.push((p.id, now));
                self.host.ask_dst(p.id);
                Vec::new()
            }
            ToSystem::Ping(p) => {
                let time_ms = self.host.now_ms();
                vec![frame(&FromSystem::Pong(Pong { id: p.id, time_ms }))]
            }
            ToSystem::Time(t) => {
                let ms = u64::try_from(t.unix_ns / 1_000_000).unwrap_or(0);
                let mine = self.host.now_ms();
                if mine < CLOCK_UNSET_MS && ms >= CLOCK_UNSET_MS {
                    match self.host.set_clock(t.unix_ns) {
                        Ok(()) => self
                            .host
                            .log(&format!("qsid: clock set from the hypervisor to {ms} ms")),
                        Err(e) => self.host.log(&format!("qsid: clock not set: {e}")),
                    }
                } else {
                    let off = i128::from(ms) - i128::from(mine);
                    self.host.log(&format!(
                        "qsid: the hypervisor's clock is {off} ms from ours"
                    ));
                }
                Vec::new()
            }
        }
    }

    fn power_down(&mut self, p: &PowerDown) -> Vec<u8> {
        match self.status.state.as_str() {
            "complete" => {}
            "ending" | "down" => return refused(p.id, msgid::ENDING, text::ENDING),
            _ => return refused(p.id, msgid::NOT_READY, text::NOT_READY),
        }
        match self.host.power_down(p) {
            Ok(()) => {
                self.host.log(&format!(
                    "qsid: power down asked: {}, delay {}, restart {}",
                    if p.immediate { "*IMMED" } else { "*CNTRLD" },
                    p.delay,
                    if p.restart { "*YES" } else { "*NO" }
                ));
                // Asked once: a second request is refused until slic's
                // codes say ending.
                self.status.state = "ending".into();
                frame(&FromSystem::Ack(Ack { id: p.id }))
            }
            Err(e) => refused(p.id, msgid::POWER_DOWN_FAILED, text::power_down_failed(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsipart::service::{Panel, Ping, Time};

    #[derive(Default)]
    struct Mem {
        now: u64,
        addrs: Vec<String>,
        powered: Vec<PowerDown>,
        asked: Vec<u32>,
        clock: Option<i64>,
        logs: Vec<String>,
    }

    impl Host for Mem {
        fn addresses(&mut self) -> Vec<String> {
            self.addrs.clone()
        }
        fn now_ms(&mut self) -> u64 {
            self.now
        }
        fn power_down(&mut self, p: &PowerDown) -> Result<(), String> {
            self.powered.push(*p);
            Ok(())
        }
        fn ask_dst(&mut self, id: u32) {
            self.asked.push(id);
        }
        fn set_clock(&mut self, unix_ns: i64) -> Result<(), String> {
            self.clock = Some(unix_ns);
            Ok(())
        }
        fn log(&mut self, line: &str) {
            self.logs.push(line.into());
        }
    }

    fn decode(frames: &[Vec<u8>]) -> Vec<FromSystem> {
        let mut r = Reader::new();
        for f in frames {
            r.push(f);
        }
        r.frames()
            .iter()
            .map(|f| FromSystem::decode(f).unwrap().unwrap())
            .collect()
    }

    fn status(state: &str, src: &str, mode: &str) -> String {
        Status {
            system: "QSHQ400".into(),
            release: "V1R2M0".into(),
            state: state.into(),
            mode: mode.into(),
            ipl_type: "B".into(),
            slot: "A".into(),
            src: src.into(),
            src_text: "text".into(),
            src_time: 1_793_000_000_000,
            serial: "10ABCDE".into(),
            ..Status::default()
        }
        .render()
    }

    fn service() -> Service<Mem> {
        let host = Mem {
            now: 1_793_000_000_000,
            addrs: vec!["10.0.2.15/24".into()],
            ..Mem::default()
        };
        Service::new(host, Status::parse(&status("ipl", "C1001000", "manual")))
    }

    #[test]
    fn hello_current_code_and_state_on_each_connection() {
        let mut s = service();
        let got = decode(&s.connect());
        let FromSystem::Hello(h) = &got[0] else {
            panic!("{got:?}")
        };
        assert_eq!(
            (h.system.as_str(), h.serial.as_str(), h.keylock),
            ("QSHQ400", "10ABCDE", Keylock::Manual)
        );
        let FromSystem::Refcode(r) = &got[1] else {
            panic!()
        };
        assert!(r.current && r.code == "C1001000");
        assert!(
            matches!(&got[2], FromSystem::State(st) if st.state == SysState::Ipl && st.addresses.is_empty())
        );
        // Codes as posted; the state only when it changes.
        let got = decode(&s.notice(Notice::Posted {
            status: status("ipl", "C1001010", "manual"),
        }));
        assert_eq!(got.len(), 1);
        assert!(matches!(&got[0], FromSystem::Refcode(r) if !r.current && r.code == "C1001010"));
        let got = decode(&s.notice(Notice::Posted {
            status: status("complete", "C6001090", "manual"),
        }));
        assert!(
            matches!(&got[1], FromSystem::State(st) if st.state == SysState::Complete && st.addresses == ["10.0.2.15/24"])
        );
        // An address that comes later is a change.
        s.host.addrs.push("fd00::15/64".into());
        assert_eq!(decode(&s.tick()).len(), 1);
        assert!(s.tick().is_empty());
        // Away and back: hello again, nothing sent while away.
        s.disconnect();
        assert!(
            s.notice(Notice::Posted {
                status: status("complete", "C6001090", "manual")
            })
            .is_empty()
        );
        let got = decode(&s.connect());
        assert!(matches!(got[0], FromSystem::Hello(_)));
        assert_eq!(got.len(), 3);
    }

    fn ask(s: &mut Service<Mem>, m: &ToSystem) -> Vec<FromSystem> {
        decode(&s.input(&m.encode().unwrap()))
    }

    #[test]
    fn power_down_when_complete_once() {
        let mut s = service();
        s.connect();
        let pd = ToSystem::PowerDown(PowerDown {
            id: 5,
            immediate: false,
            delay: 30,
            restart: false,
        });
        assert!(
            matches!(&ask(&mut s, &pd)[0], FromSystem::Refused(r) if r.id == 5 && r.msgid == msgid::NOT_READY)
        );
        s.notice(Notice::Posted {
            status: status("complete", "C6001090", "normal"),
        });
        assert_eq!(ask(&mut s, &pd), vec![FromSystem::Ack(Ack { id: 5 })]);
        assert_eq!(s.host.powered.len(), 1);
        assert_eq!(s.host.powered[0].delay, 30);
        assert!(matches!(&ask(&mut s, &pd)[0], FromSystem::Refused(r) if r.msgid == msgid::ENDING));
    }

    #[test]
    fn panel_21_goes_to_slic_and_others_are_refused() {
        let mut s = service();
        s.connect();
        let p = ToSystem::Panel(Panel {
            id: 9,
            function: panel::DST,
        });
        // During the IPL: refused here, slic is not asked.
        assert!(
            matches!(&ask(&mut s, &p)[0], FromSystem::Refused(r) if r.msgid == msgid::DST_BUSY)
        );
        s.notice(Notice::Posted {
            status: status("complete", "C6001090", "normal"),
        });
        assert!(
            matches!(&ask(&mut s, &p)[0], FromSystem::Refused(r) if r.msgid == msgid::NOT_MANUAL)
        );
        assert!(s.host.asked.is_empty());
        s.notice(Notice::Posted {
            status: status("complete", "C6001090", "manual"),
        });
        assert!(ask(&mut s, &p).is_empty());
        assert_eq!(s.host.asked, [9]);
        let got = decode(&s.notice(Notice::Answer {
            id: 9,
            refusal: Some((msgid::NOT_MANUAL.into(), text::NOT_MANUAL.into())),
        }));
        assert!(
            matches!(&got[0], FromSystem::Refused(r) if r.id == 9 && r.msgid == msgid::NOT_MANUAL)
        );
        // An answer nobody waits for is dropped.
        assert!(
            s.notice(Notice::Answer {
                id: 9,
                refusal: None
            })
            .is_empty()
        );
        let other = ToSystem::Panel(Panel {
            id: 10,
            function: 3,
        });
        assert!(
            matches!(&ask(&mut s, &other)[0], FromSystem::Refused(r) if r.msgid == msgid::FUNCTION)
        );
        // slic never answering is a refusal too.
        ask(&mut s, &p);
        s.host.now += ANSWER_MS + 1;
        let got = decode(&s.tick());
        assert!(matches!(&got[0], FromSystem::Refused(r) if r.msgid == msgid::DST_BUSY));
    }

    #[test]
    fn ping_time_unknown_and_junk() {
        let mut s = service();
        s.connect();
        assert_eq!(
            ask(&mut s, &ToSystem::Ping(Ping { id: 3 })),
            vec![FromSystem::Pong(Pong {
                id: 3,
                time_ms: 1_793_000_000_000
            })]
        );
        // Junk, then an unknown request kind, then a short body.
        let mut bytes = b"garbage".to_vec();
        bytes.extend(qsipart::frame::encode(0x0177, &7u32.to_le_bytes()).unwrap());
        bytes.extend(qsipart::frame::encode(0x0102, &[1]).unwrap());
        bytes.extend(qsipart::frame::encode(0x0999, &[]).unwrap());
        let got = decode(&s.input(&bytes));
        assert!(
            matches!(&got[0], FromSystem::Refused(r) if r.id == 7 && r.msgid == msgid::UNKNOWN)
        );
        assert!(matches!(&got[1], FromSystem::Refused(r) if r.msgid == msgid::BAD));
        assert_eq!(got.len(), 2);
        // A clock never set is set from the hypervisor's, once asked.
        s.host.now = 1000;
        let hello = ToSystem::Hello(PeerHello {
            version: 1,
            caps: caps::SERVICE_V1,
            name: "test".into(),
        });
        assert_eq!(ask(&mut s, &hello), vec![FromSystem::TimeRequest]);
        ask(
            &mut s,
            &ToSystem::Time(Time {
                unix_ns: 1_793_000_000_000_000_000,
            }),
        );
        assert_eq!(s.host.clock, Some(1_793_000_000_000_000_000));
    }
}
