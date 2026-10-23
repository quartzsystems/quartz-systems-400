// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server side of the negotiation, in the order an IBM i opens
//! with: `DO NEW-ENVIRON`, `DO TERMINAL-TYPE`, the environment `SEND`
//! with the seed, the terminal type `SEND`, then `DO EOR`, `WILL EOR`,
//! `DO BINARY`, `WILL BINARY`.

use super::env::{self, Var};
use super::{
    DO, DONT, Event, OPT_BINARY, OPT_EOR, OPT_NEW_ENVIRON, OPT_TERMINAL_TYPE, Parser, TT_IS,
    TT_SEND, WILL, WONT, command, option_name, sub,
};

/// What the client told the server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Negotiated {
    /// The terminal type, upper case.
    pub terminal_type: String,
    /// The environment variables of the `IS`.
    pub vars: Vec<Var>,
}

impl Negotiated {
    /// A variable's text value.
    pub fn var(&self, name: &str) -> Option<String> {
        self.vars
            .iter()
            .find(|v| v.name == name.as_bytes())
            .map(Var::value_str)
    }

    /// A variable's bytes.
    pub fn var_bytes(&self, name: &str) -> Option<&[u8]> {
        self.vars
            .iter()
            .find(|v| v.name == name.as_bytes())
            .and_then(|v| v.value.as_deref())
    }
}

/// The server negotiator and record splitter.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // one flag per option and direction
pub struct Server {
    parser: Parser,
    record: Vec<u8>,
    seed: [u8; 8],
    /// What the client said so far.
    pub negotiated: Negotiated,
    got_tt: bool,
    got_env: bool,
    will_env: bool,
    will_tt: bool,
    sent_tail: bool,
    will_eor: bool,
    do_eor: bool,
    will_binary: bool,
    do_binary: bool,
    /// The negotiation is complete.
    pub done: bool,
    /// One line per command, for traces.
    pub trace: Vec<String>,
}

impl Server {
    /// A server for one connection with its random seed.
    pub fn new(seed: [u8; 8]) -> Self {
        Self {
            parser: Parser::new(),
            record: Vec::new(),
            seed,
            negotiated: Negotiated::default(),
            got_tt: false,
            got_env: false,
            will_env: false,
            will_tt: false,
            sent_tail: false,
            will_eor: false,
            do_eor: false,
            will_binary: false,
            do_binary: false,
            done: false,
            trace: Vec::new(),
        }
    }

    /// The opening bytes to send on accept.
    pub fn opening(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(command(DO, OPT_NEW_ENVIRON));
        out.extend(command(DO, OPT_TERMINAL_TYPE));
        out
    }

    /// Feed what the socket read; what to send back, and the records
    /// completed.
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut out = Vec::new();
        let mut records = Vec::new();
        for ev in self.parser.feed(bytes) {
            match ev {
                Event::Data(d) => self.record.extend(d),
                Event::Eor => records.push(std::mem::take(&mut self.record)),
                Event::Will(opt) => {
                    self.trace.push(format!("< WILL {}", option_name(opt)));
                    match opt {
                        OPT_NEW_ENVIRON if !self.will_env => {
                            self.will_env = true;
                            let mut seeded = b"IBMRSEED".to_vec();
                            seeded.extend(self.seed);
                            let vars = vec![
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
                            ];
                            let mut b = vec![OPT_NEW_ENVIRON];
                            b.extend(env::encode(env::SEND, &vars));
                            self.trace
                                .push("> SB NEW-ENVIRON SEND IBMRSEED VAR USERVAR".into());
                            out.extend(sub(&b));
                        }
                        OPT_TERMINAL_TYPE if !self.will_tt => {
                            self.will_tt = true;
                            self.trace.push("> SB TERMINAL-TYPE SEND".into());
                            out.extend(sub(&[OPT_TERMINAL_TYPE, TT_SEND]));
                        }
                        OPT_EOR => self.will_eor = true,
                        OPT_BINARY => self.will_binary = true,
                        OPT_NEW_ENVIRON | OPT_TERMINAL_TYPE => {}
                        other => out.extend(command(DONT, other)),
                    }
                }
                Event::Wont(opt) => {
                    self.trace.push(format!("< WONT {}", option_name(opt)));
                    // Without the environment or a terminal type the
                    // session still proceeds, as a plain 5250.
                    if opt == OPT_NEW_ENVIRON {
                        self.got_env = true;
                    }
                    if opt == OPT_TERMINAL_TYPE {
                        self.got_tt = true;
                        self.negotiated.terminal_type = "IBM-3179-2".into();
                    }
                }
                Event::Do(opt) => {
                    self.trace.push(format!("< DO {}", option_name(opt)));
                    match opt {
                        OPT_EOR => self.do_eor = true,
                        OPT_BINARY => self.do_binary = true,
                        other => out.extend(command(WONT, other)),
                    }
                }
                Event::Dont(opt) => self.trace.push(format!("< DONT {}", option_name(opt))),
                Event::Sub(body) => self.subnegotiation(&body),
                Event::Command(c) => self.trace.push(format!("< IAC {c}")),
            }
            if self.got_env && self.got_tt && !self.sent_tail {
                self.sent_tail = true;
                self.trace
                    .push("> DO EOR, WILL EOR, DO BINARY, WILL BINARY".into());
                out.extend(command(DO, OPT_EOR));
                out.extend(command(WILL, OPT_EOR));
                out.extend(command(DO, OPT_BINARY));
                out.extend(command(WILL, OPT_BINARY));
            }
            if self.sent_tail && self.will_eor && self.do_eor && self.will_binary && self.do_binary
            {
                self.done = true;
            }
        }
        (out, records)
    }

    fn subnegotiation(&mut self, body: &[u8]) {
        let Some((&opt, rest)) = body.split_first() else {
            return;
        };
        match opt {
            OPT_TERMINAL_TYPE if rest.first() == Some(&TT_IS) => {
                let t = String::from_utf8_lossy(&rest[1..])
                    .trim()
                    .to_ascii_uppercase();
                self.trace.push(format!("< SB TERMINAL-TYPE IS {t}"));
                self.negotiated.terminal_type = t;
                self.got_tt = true;
            }
            OPT_NEW_ENVIRON => {
                if let Some((cmd, vars)) = env::decode(rest)
                    && cmd == env::IS
                {
                    self.trace.push(format!(
                        "< SB NEW-ENVIRON IS {}",
                        super::client::describe(&vars)
                    ));
                    self.negotiated.vars = vars;
                    self.got_env = true;
                }
            }
            _ => self.trace.push(format!("< SB {} ...", option_name(opt))),
        }
    }

    /// The seed this server sent.
    pub fn seed(&self) -> [u8; 8] {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::super::client::{Client, Config};
    use super::*;

    #[test]
    fn client_and_server_agree() {
        let mut s = Server::new([7; 8]);
        let mut c = Client::new(Config {
            user: Some("USR".into()),
            password: Some("pw".into()),
            device: "DSP01".into(),
            ..Config::default()
        });
        let mut to_client = s.opening();
        let mut rounds = 0;
        while !(s.done && c.negotiated) && rounds < 10 {
            let (to_server, _) = c.feed(&to_client);
            let (back, _) = s.feed(&to_server);
            to_client = back;
            rounds += 1;
        }
        assert!(s.done, "{:?}", s.trace);
        assert!(c.negotiated, "{:?}", c.trace);
        assert_eq!(s.negotiated.terminal_type, "IBM-3179-2");
        assert_eq!(s.negotiated.var("DEVNAME").as_deref(), Some("DSP01"));
        assert_eq!(s.negotiated.var("USER").as_deref(), Some("USR"));
        assert_eq!(s.negotiated.var_bytes("IBMSUBSPW").unwrap().len(), 20);
    }
}
