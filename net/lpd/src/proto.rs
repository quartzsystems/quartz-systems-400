// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RFC 1179's lines. A daemon command is one octet, the queue name and
//! operands separated by blanks, and LF (section 5); inside "receive a
//! printer job" each subcommand is one octet, a byte count, a blank, a
//! file name and LF (section 6).

use std::io::{self, Read};

use thiserror::Error;

/// The longest command line taken.
pub const MAX_LINE: usize = 1024;

/// A malformed line.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProtoError {
    /// An unknown command or subcommand octet.
    #[error("unknown LPD code {0:#04x}")]
    Code(u8),
    /// An operand missing or not valid.
    #[error("malformed LPD line: {0}")]
    Malformed(String),
}

/// A daemon command (section 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// 01: print any waiting jobs.
    PrintWaiting {
        /// The queue.
        queue: String,
    },
    /// 02: receive a printer job.
    Receive {
        /// The queue.
        queue: String,
    },
    /// 03: send the queue state, short.
    ShortState {
        /// The queue.
        queue: String,
        /// User names or job numbers to report on; empty for all.
        list: Vec<String>,
    },
    /// 04: send the queue state, long.
    LongState {
        /// The queue.
        queue: String,
        /// As for 03.
        list: Vec<String>,
    },
    /// 05: remove jobs.
    Remove {
        /// The queue.
        queue: String,
        /// Who asks.
        agent: String,
        /// User names or job numbers.
        list: Vec<String>,
    },
}

/// A receive-job subcommand (section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sub {
    /// 01: abort the job.
    Abort,
    /// 02: a control file of `count` bytes follows.
    Control {
        /// Its length.
        count: u64,
        /// Its name (`cfA123host`).
        name: String,
    },
    /// 03: a data file of `count` bytes follows; 0 means up to the end
    /// of the connection (an `LPRng` extension some clients use).
    Data {
        /// Its length.
        count: u64,
        /// Its name (`dfA123host`).
        name: String,
    },
}

/// Bytes as text, the way RFC 1179 carries names (ASCII; anything else
/// as Latin-1).
fn text(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

fn words(rest: &[u8]) -> Vec<String> {
    rest.split(|c| *c == b' ' || *c == b'\t')
        .filter(|w| !w.is_empty())
        .map(text)
        .collect()
}

impl Command {
    /// From a line without its LF.
    pub fn parse(line: &[u8]) -> Result<Self, ProtoError> {
        let (&code, rest) = line.split_first().ok_or_else(|| ProtoError::Malformed("empty".into()))?;
        let mut w = words(rest);
        if w.is_empty() {
            return Err(ProtoError::Malformed("no queue name".into()));
        }
        let queue = w.remove(0);
        Ok(match code {
            1 => Self::PrintWaiting { queue },
            2 => Self::Receive { queue },
            3 => Self::ShortState { queue, list: w },
            4 => Self::LongState { queue, list: w },
            5 => {
                if w.is_empty() {
                    return Err(ProtoError::Malformed("no agent".into()));
                }
                let agent = w.remove(0);
                Self::Remove { queue, agent, list: w }
            }
            c => return Err(ProtoError::Code(c)),
        })
    }

    /// The line with its LF.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let (code, mut parts) = match self {
            Self::PrintWaiting { queue } => (1, vec![queue.clone()]),
            Self::Receive { queue } => (2, vec![queue.clone()]),
            Self::ShortState { queue, list } => (3, [vec![queue.clone()], list.clone()].concat()),
            Self::LongState { queue, list } => (4, [vec![queue.clone()], list.clone()].concat()),
            Self::Remove { queue, agent, list } => (5, [vec![queue.clone(), agent.clone()], list.clone()].concat()),
        };
        let mut out = vec![code];
        out.extend_from_slice(parts.remove(0).as_bytes());
        for p in parts {
            out.push(b' ');
            out.extend_from_slice(p.as_bytes());
        }
        out.push(b'\n');
        out
    }
}

impl Sub {
    /// From a line without its LF.
    pub fn parse(line: &[u8]) -> Result<Self, ProtoError> {
        let (&code, rest) = line.split_first().ok_or_else(|| ProtoError::Malformed("empty".into()))?;
        if code == 1 {
            return Ok(Self::Abort);
        }
        let w = words(rest);
        let (count, name) = match w.as_slice() {
            [c, n, ..] => (
                c.parse::<u64>()
                    .map_err(|_| ProtoError::Malformed(format!("count {c}")))?,
                n.clone(),
            ),
            _ => return Err(ProtoError::Malformed(text(line))),
        };
        match code {
            2 => Ok(Self::Control { count, name }),
            3 => Ok(Self::Data { count, name }),
            c => Err(ProtoError::Code(c)),
        }
    }

    /// The line with its LF.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Abort => vec![1, b'\n'],
            Self::Control { count, name } => format!("\x02{count} {name}\n").into_bytes(),
            Self::Data { count, name } => format!("\x03{count} {name}\n").into_bytes(),
        }
    }
}

/// One line up to LF (the LF dropped, a CR before it too); `None` at
/// the end of the stream before any byte.
pub fn read_line(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match r.read(&mut b) {
            Ok(0) if out.is_empty() => return Ok(None),
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "line cut short")),
            Ok(_) if b[0] == b'\n' => {
                if out.last() == Some(&b'\r') {
                    out.pop();
                }
                return Ok(Some(out));
            }
            Ok(_) => {
                if out.len() >= MAX_LINE {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "LPD line too long"));
                }
                out.push(b[0]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        let all = [
            Command::PrintWaiting { queue: "QPRINT".into() },
            Command::Receive { queue: "QGPL/PRT01".into() },
            Command::ShortState {
                queue: "lp".into(),
                list: vec!["anne".into(), "12".into()],
            },
            Command::LongState { queue: "lp".into(), list: vec![] },
            Command::Remove {
                queue: "lp".into(),
                agent: "root".into(),
                list: vec!["7".into()],
            },
        ];
        for c in all {
            let line = c.encode();
            assert_eq!(line.last(), Some(&b'\n'));
            assert_eq!(Command::parse(&line[..line.len() - 1]).unwrap(), c);
        }
        assert_eq!(Command::Receive { queue: "Q".into() }.encode(), b"\x02Q\n");
    }

    #[test]
    fn subcommands() {
        assert_eq!(
            Sub::parse(b"\x02123 cfA001host").unwrap(),
            Sub::Control {
                count: 123,
                name: "cfA001host".into()
            }
        );
        assert_eq!(Sub::parse(b"\x030 dfA001host").unwrap(), Sub::Data { count: 0, name: "dfA001host".into() });
        assert_eq!(Sub::parse(b"\x01").unwrap(), Sub::Abort);
        let d = Sub::Data { count: 9, name: "dfA9h".into() };
        assert_eq!(Sub::parse(&d.encode()[..d.encode().len() - 1]).unwrap(), d);
        assert!(matches!(Sub::parse(b"\x02x cf"), Err(ProtoError::Malformed(_))));
        assert_eq!(Sub::parse(b"\x0912 n"), Err(ProtoError::Code(9)));
        assert_eq!(Command::parse(b"\x09q"), Err(ProtoError::Code(9)));
        assert!(Command::parse(b"\x02").is_err());
    }

    #[test]
    fn lines() {
        let mut r: &[u8] = b"\x02lp\r\nrest";
        assert_eq!(read_line(&mut r).unwrap().unwrap(), b"\x02lp");
        assert!(read_line(&mut r).is_err(), "cut short");
        let mut empty: &[u8] = b"";
        assert_eq!(read_line(&mut empty).unwrap(), None);
    }
}
