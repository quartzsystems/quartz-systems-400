// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client side of the negotiation: answers the server's `DO`s,
//! sends the environment (device, keyboard, code page and, when a
//! password is given, the RFC 4777 substitute) and the terminal type,
//! then switches to binary records.

use super::env::{self, Var};
use super::{
    DO, DONT, Event, OPT_BINARY, OPT_EOR, OPT_NEW_ENVIRON, OPT_SGA, OPT_TERMINAL_TYPE, Parser,
    TT_IS, TT_SEND, WILL, WONT, command, option_name, signon, sub,
};

/// Which password substitute to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding {
    /// DES, password levels 0 and 1.
    Des,
    /// SHA-1, password levels 2 and 3.
    #[default]
    Sha,
}

/// What the client offers.
#[derive(Clone, Debug)]
pub struct Config {
    /// `IBM-3179-2`, `IBM-3477-FC`, ...
    pub terminal_type: String,
    /// `DEVNAME`; empty for the server's choice.
    pub device: String,
    /// `USER` for auto-signon.
    pub user: Option<String>,
    /// The password for auto-signon.
    pub password: Option<String>,
    /// The substitute's encoding.
    pub encoding: Encoding,
    /// `KBDTYPE`.
    pub kbdtype: String,
    /// `CODEPAGE`.
    pub codepage: String,
    /// `CHARSET`.
    pub charset: String,
    /// The client's seed.
    pub client_seed: [u8; 8],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal_type: "IBM-3179-2".into(),
            device: String::new(),
            user: None,
            password: None,
            encoding: Encoding::Sha,
            kbdtype: "USB".into(),
            codepage: "37".into(),
            charset: "697".into(),
            client_seed: [0x51, 0x53, 0x34, 0x30, 0x30, 0x01, 0x02, 0x03],
        }
    }
}

/// The client negotiator and record splitter.
#[derive(Debug)]
pub struct Client {
    cfg: Config,
    parser: Parser,
    record: Vec<u8>,
    /// The server's seed, once it sent one.
    pub server_seed: Option<[u8; 8]>,
    sent_will_binary: bool,
    got_will_binary: bool,
    /// Both sides agreed on binary and end-of-record.
    pub negotiated: bool,
    /// The negotiation, one line per command, for the trace.
    pub trace: Vec<String>,
}

impl Client {
    /// A client about to connect.
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            parser: Parser::new(),
            record: Vec::new(),
            server_seed: None,
            sent_will_binary: false,
            got_will_binary: false,
            negotiated: false,
            trace: Vec::new(),
        }
    }

    /// Feed what the socket read; what to send back, and the records
    /// completed (header and data, `IAC EOR` removed).
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut out = Vec::new();
        let mut records = Vec::new();
        for ev in self.parser.feed(bytes) {
            match ev {
                Event::Data(d) => self.record.extend(d),
                Event::Eor => records.push(std::mem::take(&mut self.record)),
                Event::Do(opt) => {
                    self.trace.push(format!("< DO {}", option_name(opt)));
                    let verb = match opt {
                        OPT_NEW_ENVIRON | OPT_TERMINAL_TYPE | OPT_EOR | OPT_SGA => WILL,
                        OPT_BINARY => {
                            self.sent_will_binary = true;
                            WILL
                        }
                        _ => WONT,
                    };
                    self.trace.push(format!(
                        "> {} {}",
                        if verb == WILL { "WILL" } else { "WONT" },
                        option_name(opt)
                    ));
                    out.extend(command(verb, opt));
                }
                Event::Will(opt) => {
                    self.trace.push(format!("< WILL {}", option_name(opt)));
                    let verb = match opt {
                        OPT_EOR | OPT_SGA => DO,
                        OPT_BINARY => {
                            self.got_will_binary = true;
                            DO
                        }
                        _ => DONT,
                    };
                    self.trace.push(format!(
                        "> {} {}",
                        if verb == DO { "DO" } else { "DONT" },
                        option_name(opt)
                    ));
                    out.extend(command(verb, opt));
                }
                Event::Wont(opt) => self.trace.push(format!("< WONT {}", option_name(opt))),
                Event::Dont(opt) => self.trace.push(format!("< DONT {}", option_name(opt))),
                Event::Sub(body) => out.extend(self.subnegotiation(&body)),
                Event::Command(c) => self.trace.push(format!("< IAC {c}")),
            }
            if self.sent_will_binary && self.got_will_binary {
                self.negotiated = true;
            }
        }
        (out, records)
    }

    fn subnegotiation(&mut self, body: &[u8]) -> Vec<u8> {
        let Some((&opt, rest)) = body.split_first() else {
            return Vec::new();
        };
        match opt {
            OPT_TERMINAL_TYPE if rest.first() == Some(&TT_SEND) => {
                self.trace.push("< SB TERMINAL-TYPE SEND".into());
                self.trace
                    .push(format!("> SB TERMINAL-TYPE IS {}", self.cfg.terminal_type));
                let mut b = vec![OPT_TERMINAL_TYPE, TT_IS];
                b.extend(self.cfg.terminal_type.as_bytes());
                sub(&b)
            }
            OPT_NEW_ENVIRON => {
                let Some((cmd, vars)) = env::decode(rest) else {
                    return Vec::new();
                };
                self.trace.push(format!(
                    "< SB NEW-ENVIRON {} {}",
                    if cmd == env::SEND { "SEND" } else { "IS" },
                    describe(&vars)
                ));
                if cmd != env::SEND {
                    return Vec::new();
                }
                for v in &vars {
                    // The seed follows the name directly (RFC 4777), or
                    // sits in a value.
                    let seed = if v.name.len() == 16 && v.name.starts_with(b"IBMRSEED") {
                        Some(&v.name[8..])
                    } else if v.name == b"IBMRSEED" {
                        v.value.as_deref().filter(|val| val.len() == 8)
                    } else {
                        None
                    };
                    if let Some(val) = seed {
                        let mut s = [0u8; 8];
                        s.copy_from_slice(val);
                        self.server_seed = Some(s);
                    }
                }
                let reply = self.environment();
                self.trace
                    .push(format!("> SB NEW-ENVIRON IS {}", describe(&reply)));
                let mut b = vec![OPT_NEW_ENVIRON];
                b.extend(env::encode(env::IS, &reply));
                sub(&b)
            }
            _ => {
                self.trace.push(format!("< SB {} ...", option_name(opt)));
                Vec::new()
            }
        }
    }

    /// The variables of the `IS`.
    fn environment(&self) -> Vec<Var> {
        let mut vars = Vec::new();
        if let (Some(user), Some(seed)) = (&self.cfg.user, self.server_seed) {
            vars.push(Var::known("USER", &user.to_ascii_uppercase()));
            vars.push(Var::user_bytes("IBMRSEED", &self.cfg.client_seed));
            if let Some(pw) = &self.cfg.password {
                let sub: Vec<u8> = match self.cfg.encoding {
                    Encoding::Des => {
                        signon::des_password(user, pw, &seed, &self.cfg.client_seed).to_vec()
                    }
                    Encoding::Sha => {
                        signon::sha_password(user, pw, &seed, &self.cfg.client_seed).to_vec()
                    }
                };
                vars.push(Var::user_bytes("IBMSUBSPW", &sub));
            }
        } else if let Some(user) = &self.cfg.user {
            vars.push(Var::known("USER", &user.to_ascii_uppercase()));
        }
        if !self.cfg.device.is_empty() {
            vars.push(Var::user("DEVNAME", &self.cfg.device));
        }
        vars.push(Var::user("KBDTYPE", &self.cfg.kbdtype));
        vars.push(Var::user("CODEPAGE", &self.cfg.codepage));
        vars.push(Var::user("CHARSET", &self.cfg.charset));
        vars
    }
}

/// Variables for the trace, secrets as lengths.
pub fn describe(vars: &[Var]) -> String {
    vars.iter()
        .map(|v| {
            let name = if v.name.len() == 16 && v.name.starts_with(b"IBMRSEED") {
                "IBMRSEED<8 bytes>".to_string()
            } else {
                v.name_str()
            };
            match &v.value {
                None => name,
                Some(val) if name == "IBMRSEED" || name == "IBMSUBSPW" => {
                    format!("{name}=<{} bytes>", val.len())
                }
                Some(val) => format!("{name}={}", String::from_utf8_lossy(val)),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telnet::{IAC, SB, SE};

    #[test]
    fn answers_the_opening() {
        let mut c = Client::new(Config {
            user: Some("qsecofr".into()),
            password: Some("pw".into()),
            device: "QPADEV0001".into(),
            ..Config::default()
        });
        let (out, _) = c.feed(&[IAC, DO, OPT_NEW_ENVIRON, IAC, DO, OPT_TERMINAL_TYPE]);
        assert_eq!(
            out,
            vec![IAC, WILL, OPT_NEW_ENVIRON, IAC, WILL, OPT_TERMINAL_TYPE]
        );
        let mut send = vec![IAC, SB, OPT_NEW_ENVIRON];
        let mut seeded = b"IBMRSEED".to_vec();
        seeded.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        send.extend(env::encode(
            env::SEND,
            &[
                Var {
                    kind: env::USERVAR,
                    name: seeded,
                    value: None,
                },
                Var {
                    kind: env::VAR,
                    name: Vec::new(),
                    value: None,
                },
                Var {
                    kind: env::USERVAR,
                    name: Vec::new(),
                    value: None,
                },
            ],
        ));
        send.extend([IAC, SE]);
        let (out, _) = c.feed(&send);
        assert_eq!(c.server_seed, Some([1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(&out[..3], &[IAC, SB, OPT_NEW_ENVIRON]);
        let (_, vars) = env::decode(&out[3..out.len() - 2]).unwrap();
        assert_eq!(vars[0].name, b"USER");
        assert_eq!(vars[0].value.as_deref(), Some(&b"QSECOFR"[..]));
        assert_eq!(vars[2].name, b"IBMSUBSPW");
        assert_eq!(vars[2].value.as_ref().unwrap().len(), 20);
        let (out, _) = c.feed(&[IAC, SB, OPT_TERMINAL_TYPE, TT_SEND, IAC, SE]);
        assert!(out.ends_with(b"IBM-3179-2\xff\xf0"));
        let (out, _) = c.feed(&[
            IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY,
        ]);
        assert_eq!(out.len(), 12);
        assert!(c.negotiated);
        let (_, recs) = c.feed(&[
            0,
            12,
            0x12,
            0xA0,
            0,
            0,
            4,
            0,
            0,
            0,
            0xFF,
            0xFF,
            IAC,
            super::super::EOR,
        ]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].len(), 11);
    }
}
