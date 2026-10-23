// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Telnet (RFC 854): the command bytes, the options TN5250E needs, a
//! parser that splits a byte stream into data and commands, and the
//! escaping of data in binary mode.

pub mod client;
pub mod env;
pub mod server;
pub mod signon;

/// Interpret As Command.
pub const IAC: u8 = 255;
/// `DONT`.
pub const DONT: u8 = 254;
/// `DO`.
pub const DO: u8 = 253;
/// `WONT`.
pub const WONT: u8 = 252;
/// `WILL`.
pub const WILL: u8 = 251;
/// Subnegotiation begin.
pub const SB: u8 = 250;
/// Subnegotiation end.
pub const SE: u8 = 240;
/// No operation.
pub const NOP: u8 = 241;
/// End of record (RFC 885), sent as `IAC EOR` after each 5250 record.
pub const EOR: u8 = 239;

/// Option: binary transmission (RFC 856).
pub const OPT_BINARY: u8 = 0;
/// Option: echo.
pub const OPT_ECHO: u8 = 1;
/// Option: suppress go ahead (RFC 858).
pub const OPT_SGA: u8 = 3;
/// Option: terminal type (RFC 1091).
pub const OPT_TERMINAL_TYPE: u8 = 24;
/// Option: end of record (RFC 885).
pub const OPT_EOR: u8 = 25;
/// Option: new environment (RFC 1572), carrying the TN5250E variables.
pub const OPT_NEW_ENVIRON: u8 = 39;

/// Terminal-type subnegotiation: `IS`.
pub const TT_IS: u8 = 0;
/// Terminal-type subnegotiation: `SEND`.
pub const TT_SEND: u8 = 1;

/// What the parser found in the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Data bytes, IAC escapes removed.
    Data(Vec<u8>),
    /// `IAC EOR`: the record so far is complete.
    Eor,
    /// `IAC WILL option`.
    Will(u8),
    /// `IAC WONT option`.
    Wont(u8),
    /// `IAC DO option`.
    Do(u8),
    /// `IAC DONT option`.
    Dont(u8),
    /// `IAC SB ... IAC SE`, the bytes between.
    Sub(Vec<u8>),
    /// Another two-byte command (`NOP`, `BREAK`, ...).
    Command(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Data,
    Iac,
    Opt(u8),
    Sub,
    SubIac,
}

/// Splits a telnet byte stream into events. Feed it what the socket
/// reads; it keeps partial commands between calls.
#[derive(Debug)]
pub struct Parser {
    state: State,
    data: Vec<u8>,
    sub: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// A parser at the start of a stream.
    pub fn new() -> Self {
        Self {
            state: State::Data,
            data: Vec::new(),
            sub: Vec::new(),
        }
    }

    /// Feed bytes; the events they complete, in order. Data runs are
    /// flushed before any command and at the end of the input.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        for &b in bytes {
            match self.state {
                State::Data => {
                    if b == IAC {
                        self.state = State::Iac;
                    } else {
                        self.data.push(b);
                    }
                }
                State::Iac => match b {
                    IAC => {
                        self.data.push(IAC);
                        self.state = State::Data;
                    }
                    WILL | WONT | DO | DONT => {
                        self.flush(&mut out);
                        self.state = State::Opt(b);
                    }
                    SB => {
                        self.flush(&mut out);
                        self.sub.clear();
                        self.state = State::Sub;
                    }
                    EOR => {
                        self.flush(&mut out);
                        out.push(Event::Eor);
                        self.state = State::Data;
                    }
                    other => {
                        self.flush(&mut out);
                        out.push(Event::Command(other));
                        self.state = State::Data;
                    }
                },
                State::Opt(verb) => {
                    out.push(match verb {
                        WILL => Event::Will(b),
                        WONT => Event::Wont(b),
                        DO => Event::Do(b),
                        _ => Event::Dont(b),
                    });
                    self.state = State::Data;
                }
                State::Sub => {
                    if b == IAC {
                        self.state = State::SubIac;
                    } else {
                        self.sub.push(b);
                    }
                }
                State::SubIac => match b {
                    IAC => {
                        self.sub.push(IAC);
                        self.state = State::Sub;
                    }
                    SE => {
                        out.push(Event::Sub(std::mem::take(&mut self.sub)));
                        self.state = State::Data;
                    }
                    _ => {
                        // A stray command inside a subnegotiation: keep going.
                        self.state = State::Sub;
                    }
                },
            }
        }
        self.flush(&mut out);
        out
    }

    fn flush(&mut self, out: &mut Vec<Event>) {
        if !self.data.is_empty() {
            out.push(Event::Data(std::mem::take(&mut self.data)));
        }
    }
}

/// Data with every `IAC` doubled.
pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    for &b in data {
        out.push(b);
        if b == IAC {
            out.push(IAC);
        }
    }
    out
}

/// `IAC verb option`.
pub fn command(verb: u8, option: u8) -> [u8; 3] {
    [IAC, verb, option]
}

/// `IAC SB bytes IAC SE`, the bytes escaped.
pub fn sub(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![IAC, SB];
    out.extend(escape(bytes));
    out.push(IAC);
    out.push(SE);
    out
}

/// A record and its `IAC EOR`, escaped for binary mode.
pub fn record(bytes: &[u8]) -> Vec<u8> {
    let mut out = escape(bytes);
    out.push(IAC);
    out.push(EOR);
    out
}

/// The option's name for traces.
pub fn option_name(option: u8) -> &'static str {
    match option {
        OPT_BINARY => "BINARY",
        OPT_ECHO => "ECHO",
        OPT_SGA => "SGA",
        OPT_TERMINAL_TYPE => "TERMINAL-TYPE",
        OPT_EOR => "EOR",
        OPT_NEW_ENVIRON => "NEW-ENVIRON",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commands_and_data() {
        let mut p = Parser::new();
        let ev = p.feed(&[IAC, DO, OPT_NEW_ENVIRON, 0x41, IAC, IAC, 0x42, IAC, EOR]);
        assert_eq!(
            ev,
            vec![
                Event::Do(OPT_NEW_ENVIRON),
                Event::Data(vec![0x41, IAC, 0x42]),
                Event::Eor
            ]
        );
    }

    #[test]
    fn subnegotiation_spans_feeds() {
        let mut p = Parser::new();
        assert!(p.feed(&[IAC, SB, OPT_TERMINAL_TYPE, TT_SEND]).is_empty());
        assert_eq!(
            p.feed(&[IAC, SE]),
            vec![Event::Sub(vec![OPT_TERMINAL_TYPE, TT_SEND])]
        );
    }

    #[test]
    fn escapes_iac() {
        assert_eq!(record(&[1, IAC, 2]), vec![1, IAC, IAC, 2, IAC, EOR]);
    }
}
