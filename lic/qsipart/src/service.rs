// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service port's messages. The system speaks first: on each
//! connection of the peer it sends [`Hello`], the current
//! [`Refcode`] (with `current` set) and its [`State`]; then each code
//! as it is posted and each change of state. The hypervisor sends its
//! [`PeerHello`] and requests: [`PowerDown`], [`Panel`] and [`Ping`]
//! carry an `id` that the answer ([`Ack`], [`Refused`], [`Pong`])
//! repeats. [`FromSystem::TimeRequest`] asks for the hypervisor's
//! clock, which comes back as [`Time`].

use ipc::{Wire, wire_struct};

use crate::Error;
use crate::frame::Frame;
use crate::wire::{body, frame_of, open_enum};

/// Message kinds on the service port.
pub mod kind {
    /// The system's hello.
    pub const HELLO: u16 = 0x0001;
    /// A reference code.
    pub const REFCODE: u16 = 0x0002;
    /// The system's state.
    pub const STATE: u16 = 0x0003;
    /// A request was accepted.
    pub const ACK: u16 = 0x0004;
    /// A request was refused.
    pub const REFUSED: u16 = 0x0005;
    /// The answer to a ping.
    pub const PONG: u16 = 0x0006;
    /// The system asks for the hypervisor's clock.
    pub const TIME_REQUEST: u16 = 0x0007;
    /// The hypervisor's hello.
    pub const PEER_HELLO: u16 = 0x0101;
    /// Power down.
    pub const POWER_DOWN: u16 = 0x0102;
    /// A control panel function.
    pub const PANEL: u16 = 0x0103;
    /// A ping.
    pub const PING: u16 = 0x0104;
    /// The hypervisor's clock.
    pub const TIME: u16 = 0x0105;
}

/// Control panel functions.
pub mod panel {
    /// Function 21: dedicated service tools on the console.
    pub const DST: u8 = 21;
}

/// The message identifiers of a refusal.
pub mod msgid {
    /// A request of a kind the system does not know.
    pub const UNKNOWN: &str = "QSI0001";
    /// A request whose body could not be read.
    pub const BAD: &str = "QSI0002";
    /// Power down before the IPL is complete.
    pub const NOT_READY: &str = "QSI0010";
    /// Power down while one is under way.
    pub const ENDING: &str = "QSI0011";
    /// The power down could not be started.
    pub const POWER_DOWN_FAILED: &str = "QSI0012";
    /// A panel function this system does not carry out.
    pub const FUNCTION: &str = "QSI0020";
    /// Function 21 with the keylock not in manual mode.
    pub const NOT_MANUAL: &str = "QSI0021";
    /// Function 21 before the IPL is complete, or with service tools
    /// already on the console.
    pub const DST_BUSY: &str = "QSI0022";
}

open_enum!(
    /// The keylock's mode as the system read it at IPL.
    Keylock {
        /// Normal.
        Normal = 1,
        /// Manual.
        Manual = 2,
    }
);

open_enum!(
    /// What the system is doing.
    SysState {
        /// An IPL is under way.
        Ipl = 1,
        /// The IPL is complete.
        Complete = 2,
        /// The system is ending: a power down under way.
        Ending = 3,
        /// The system has ended: the next thing is the power off, the
        /// restart or a halt.
        Down = 4,
    }
);

/// The system's hello.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// Its protocol version.
    pub version: u16,
    /// Its capabilities (`crate::caps`).
    pub caps: u64,
    /// The system name.
    pub system: String,
    /// The release (`V1R2M0`).
    pub release: String,
    /// The serial number.
    pub serial: String,
    /// The slot of this IPL (`A`, `B`).
    pub slot: String,
    /// The IPL type (`A`, `B`, `D`).
    pub ipl_type: String,
    /// The keylock's mode.
    pub keylock: Keylock,
}
wire_struct!(Hello {
    version,
    caps,
    system,
    release,
    serial,
    slot,
    ipl_type,
    keylock
});

/// A reference code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refcode {
    /// The code, eight characters (`C1001000`).
    pub code: String,
    /// Its words 2 to 9 as hexadecimal text, where it has them.
    pub words: Vec<String>,
    /// Its text.
    pub text: String,
    /// When it was posted, milliseconds since 1970-01-01 UTC.
    pub time_ms: u64,
    /// Sent on a connection for the code current then, not as it was
    /// posted.
    pub current: bool,
}
wire_struct!(Refcode {
    code,
    words,
    text,
    time_ms,
    current
});

/// The system's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// What it is doing.
    pub state: SysState,
    /// The cumulative fix level, empty until known.
    pub fix_level: String,
    /// The IP addresses, empty until known.
    pub addresses: Vec<String>,
}
wire_struct!(State {
    state,
    fix_level,
    addresses
});

/// A request accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ack {
    /// The request's `id`.
    pub id: u32,
}
wire_struct!(Ack { id });

/// A request refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The request's `id`; 0 when it could not be read.
    pub id: u32,
    /// The message identifier ([`msgid`]).
    pub msgid: String,
    /// The message's text.
    pub text: String,
}
wire_struct!(Refused { id, msgid, text });

/// A ping's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pong {
    /// The ping's `id`.
    pub id: u32,
    /// The system's clock, milliseconds since 1970-01-01 UTC.
    pub time_ms: u64,
}
wire_struct!(Pong { id, time_ms });

/// The hypervisor's hello.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerHello {
    /// Its protocol version.
    pub version: u16,
    /// Its capabilities.
    pub caps: u64,
    /// Its name and release, for the system's log.
    pub name: String,
}
wire_struct!(PeerHello {
    version,
    caps,
    name
});

/// Power down: `PWRDWNSYS` with these options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerDown {
    /// The request's id.
    pub id: u32,
    /// `OPTION(*IMMED)`; else `*CNTRLD`.
    pub immediate: bool,
    /// `DELAY`, seconds, for a controlled one.
    pub delay: u32,
    /// `RESTART(*YES)`.
    pub restart: bool,
}
wire_struct!(PowerDown {
    id,
    immediate,
    delay,
    restart
});

/// The longest `DELAY` (`PWRDWNSYS` takes up to 99999 seconds).
pub const MAX_DELAY: u32 = 99_999;

/// A control panel function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    /// The request's id.
    pub id: u32,
    /// The function's number ([`panel`]).
    pub function: u8,
}
wire_struct!(Panel { id, function });

/// A ping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ping {
    /// The request's id.
    pub id: u32,
}
wire_struct!(Ping { id });

/// The hypervisor's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    /// Nanoseconds since 1970-01-01 UTC.
    pub unix_ns: i64,
}
wire_struct!(Time { unix_ns });

/// The longest text a field of the peer's carries.
const MAX_PEER_TEXT: usize = 256;

/// What the system sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromSystem {
    /// [`Hello`].
    Hello(Hello),
    /// [`Refcode`].
    Refcode(Refcode),
    /// [`State`].
    State(State),
    /// [`Ack`].
    Ack(Ack),
    /// [`Refused`].
    Refused(Refused),
    /// [`Pong`].
    Pong(Pong),
    /// Ask for the hypervisor's clock.
    TimeRequest,
}

impl FromSystem {
    /// Its kind.
    #[must_use]
    pub fn kind(&self) -> u16 {
        match self {
            Self::Hello(_) => kind::HELLO,
            Self::Refcode(_) => kind::REFCODE,
            Self::State(_) => kind::STATE,
            Self::Ack(_) => kind::ACK,
            Self::Refused(_) => kind::REFUSED,
            Self::Pong(_) => kind::PONG,
            Self::TimeRequest => kind::TIME_REQUEST,
        }
    }

    /// Its frame's bytes.
    ///
    /// # Errors
    /// It does not fit a frame.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let k = self.kind();
        match self {
            Self::Hello(m) => frame_of(k, m),
            Self::Refcode(m) => frame_of(k, m),
            Self::State(m) => frame_of(k, m),
            Self::Ack(m) => frame_of(k, m),
            Self::Refused(m) => frame_of(k, m),
            Self::Pong(m) => frame_of(k, m),
            Self::TimeRequest => frame_of(k, &()),
        }
    }

    /// A frame from the system; `None` for a kind this version does not
    /// know, which the receiver passes over.
    ///
    /// # Errors
    /// The body does not read as its kind's message.
    pub fn decode(f: &Frame) -> Result<Option<Self>, Error> {
        Ok(Some(match f.kind {
            kind::HELLO => Self::Hello(body(f)?),
            kind::REFCODE => Self::Refcode(body(f)?),
            kind::STATE => Self::State(body(f)?),
            kind::ACK => Self::Ack(body(f)?),
            kind::REFUSED => Self::Refused(body(f)?),
            kind::PONG => Self::Pong(body(f)?),
            kind::TIME_REQUEST => Self::TimeRequest,
            _ => return Ok(None),
        }))
    }
}

/// What the hypervisor sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToSystem {
    /// [`PeerHello`].
    Hello(PeerHello),
    /// [`PowerDown`].
    PowerDown(PowerDown),
    /// [`Panel`].
    Panel(Panel),
    /// [`Ping`].
    Ping(Ping),
    /// [`Time`].
    Time(Time),
}

impl ToSystem {
    /// Its kind.
    #[must_use]
    pub fn kind(&self) -> u16 {
        match self {
            Self::Hello(_) => kind::PEER_HELLO,
            Self::PowerDown(_) => kind::POWER_DOWN,
            Self::Panel(_) => kind::PANEL,
            Self::Ping(_) => kind::PING,
            Self::Time(_) => kind::TIME,
        }
    }

    /// Its frame's bytes.
    ///
    /// # Errors
    /// It does not fit a frame.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let k = self.kind();
        match self {
            Self::Hello(m) => frame_of(k, m),
            Self::PowerDown(m) => frame_of(k, m),
            Self::Panel(m) => frame_of(k, m),
            Self::Ping(m) => frame_of(k, m),
            Self::Time(m) => frame_of(k, m),
        }
    }

    /// A frame from the hypervisor, checked; `None` for a kind this
    /// version does not know (a request of it is refused with
    /// [`msgid::UNKNOWN`]).
    ///
    /// # Errors
    /// The body does not read as its kind's message, or says something
    /// out of range.
    pub fn decode(f: &Frame) -> Result<Option<Self>, Error> {
        let invalid = |why| Error::Invalid { kind: f.kind, why };
        Ok(Some(match f.kind {
            kind::PEER_HELLO => {
                let h: PeerHello = body(f)?;
                if h.name.len() > MAX_PEER_TEXT {
                    return Err(invalid("the peer's name is too long"));
                }
                Self::Hello(h)
            }
            kind::POWER_DOWN => {
                let p: PowerDown = body(f)?;
                if p.delay > MAX_DELAY {
                    return Err(invalid("the delay is longer than 99999 seconds"));
                }
                Self::PowerDown(p)
            }
            kind::PANEL => Self::Panel(body(f)?),
            kind::PING => Self::Ping(body(f)?),
            kind::TIME => Self::Time(body(f)?),
            _ => return Ok(None),
        }))
    }

    /// The `id` of a request, read from what can be read of its body:
    /// for a refusal of one that does not decode or is of a kind this
    /// version does not know. Every kind of the peer's (`0x01nn`) but
    /// `peer_hello` and `time`, a later version's too, starts with it.
    #[must_use]
    pub fn request_id(f: &Frame) -> u32 {
        match f.kind {
            kind::PEER_HELLO | kind::TIME => 0,
            k if k & 0xFF00 == 0x0100 => u32::get(&mut ipc::Dec::new(&f.body)).unwrap_or(0),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Reader;

    fn round<T>(bytes: &[u8], dec: fn(&Frame) -> Result<Option<T>, Error>) -> T {
        let mut r = Reader::new();
        r.push(bytes);
        let f = r.next_frame().expect("a frame");
        dec(&f).unwrap().expect("a known kind")
    }

    #[test]
    fn every_message_round_trips() {
        let up = [
            FromSystem::Hello(Hello {
                version: 1,
                caps: crate::caps::SERVICE_V1,
                system: "QSHQ400".into(),
                release: "V1R2M0".into(),
                serial: "10ABCDE".into(),
                slot: "A".into(),
                ipl_type: "B".into(),
                keylock: Keylock::Manual,
            }),
            FromSystem::Refcode(Refcode {
                code: "C1001000".into(),
                words: vec!["00000001".into()],
                text: "IPL started".into(),
                time_ms: 1_793_000_000_123,
                current: true,
            }),
            FromSystem::State(State {
                state: SysState::Complete,
                fix_level: "QS26318".into(),
                addresses: vec!["10.0.2.15/24".into()],
            }),
            FromSystem::Ack(Ack { id: 7 }),
            FromSystem::Refused(Refused {
                id: 8,
                msgid: msgid::NOT_MANUAL.into(),
                text: "no".into(),
            }),
            FromSystem::Pong(Pong { id: 9, time_ms: 5 }),
            FromSystem::TimeRequest,
        ];
        for m in up {
            assert_eq!(round(&m.encode().unwrap(), FromSystem::decode), m);
        }
        let down = [
            ToSystem::Hello(PeerHello {
                version: 1,
                caps: 0,
                name: "qsvmm V1".into(),
            }),
            ToSystem::PowerDown(PowerDown {
                id: 1,
                immediate: false,
                delay: 30,
                restart: true,
            }),
            ToSystem::Panel(Panel {
                id: 2,
                function: panel::DST,
            }),
            ToSystem::Ping(Ping { id: 3 }),
            ToSystem::Time(Time { unix_ns: -1 }),
        ];
        for m in down {
            assert_eq!(round(&m.encode().unwrap(), ToSystem::decode), m);
        }
    }

    #[test]
    fn older_and_newer_peers() {
        // A later version's field after the known ones is passed over.
        let mut b = ipc::encode(&Ping { id: 4 });
        b.extend_from_slice(b"later");
        let f = Frame {
            kind: kind::PING,
            body: b,
        };
        assert_eq!(
            ToSystem::decode(&f).unwrap(),
            Some(ToSystem::Ping(Ping { id: 4 }))
        );
        assert_eq!(ToSystem::request_id(&f), 4);
        // An unknown kind is not an error.
        let f = Frame {
            kind: 0x01FF,
            body: vec![1, 2],
        };
        assert_eq!(ToSystem::decode(&f).unwrap(), None);
        // A value a later version adds to an enum is kept.
        let f = Frame {
            kind: kind::STATE,
            body: vec![9, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        let Some(FromSystem::State(s)) = FromSystem::decode(&f).unwrap() else {
            panic!("a state");
        };
        assert_eq!(s.state, SysState::Other(9));
    }

    #[test]
    fn hostile_bodies() {
        let short = Frame {
            kind: kind::POWER_DOWN,
            body: vec![1, 0],
        };
        assert!(ToSystem::decode(&short).is_err());
        let long = ToSystem::PowerDown(PowerDown {
            id: 1,
            immediate: false,
            delay: MAX_DELAY + 1,
            restart: false,
        });
        let mut r = Reader::new();
        r.push(&long.encode().unwrap());
        let f = r.next_frame().unwrap();
        assert!(matches!(ToSystem::decode(&f), Err(Error::Invalid { .. })));
        // A length past the body.
        let lying = Frame {
            kind: kind::PEER_HELLO,
            body: vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0x7F],
        };
        assert!(ToSystem::decode(&lying).is_err());
    }
}
