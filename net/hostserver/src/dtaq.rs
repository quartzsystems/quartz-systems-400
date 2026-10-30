// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data queue server (8472): create, delete, attributes, write,
//! read and peek (keyed or not), clear. Every template starts with the
//! queue and library (EBCDIC, ten each); replies start with a u16
//! return code, `0xF000` for success.

use hostsvr::cp::dtaq as cp;
use hostsvr::dtaq::id;
use hostsvr::frame::{Frame, Param};
use regfac::ExitRequest;
use regfac::formats::host::{DataQueue, zhq00100};

use crate::exits::{Exits, who};
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, reply, text};

/// The exit point.
pub const EXIT_POINT: &str = "QIBM_QZHQ_DATA_QUEUE";

/// Return codes.
pub mod rc {
    /// Success.
    pub const OK: u16 = 0xF000;
    /// The request failed; a message follows.
    pub const ERROR: u16 = 0xF001;
    /// Protocol error.
    pub const PROTOCOL: u16 = 0xF002;
    /// No entry to read.
    pub const NO_ENTRY: u16 = 0xF006;
    /// An exit program refused the request.
    pub const EXIT_REJECTED: u16 = 0xF009;
}

/// A queue's attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueAttrs {
    /// Maximum entry length.
    pub max_len: u32,
    /// Key length (keyed queues).
    pub key_len: u16,
    /// `F` FIFO, `L` LIFO, `K` keyed.
    pub sequence: char,
    /// Sender information kept with each entry.
    pub sender: bool,
    /// Forced to auxiliary storage.
    pub force: bool,
    /// Description.
    pub text: String,
    /// Public authority (`*LIBCRTAUT`, `*ALL`, `*CHANGE`, `*USE`, `*EXCLUDE`).
    pub authority: String,
}

/// An entry read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The data.
    pub data: Vec<u8>,
    /// Its key.
    pub key: Vec<u8>,
    /// Sender information (36 bytes when kept).
    pub sender: Vec<u8>,
}

/// A failure: a message ID and its text.
pub type Fail = (String, String);

/// The job's data queues.
pub trait DataQueues {
    /// Create a queue.
    fn create(&mut self, name: &str, lib: &str, a: &QueueAttrs) -> Result<(), Fail>;
    /// Delete a queue.
    fn delete(&mut self, name: &str, lib: &str) -> Result<(), Fail>;
    /// A queue's attributes.
    fn attributes(&mut self, name: &str, lib: &str) -> Result<QueueAttrs, Fail>;
    /// Write an entry.
    fn write(&mut self, name: &str, lib: &str, key: &[u8], data: &[u8]) -> Result<(), Fail>;
    /// Read (or with `peek` look at) an entry, waiting up to `wait`
    /// seconds (-1 forever); `order` is `EQ`, `GE`, ... for keyed reads.
    fn read(
        &mut self,
        name: &str,
        lib: &str,
        order: &str,
        key: &[u8],
        wait: i32,
        peek: bool,
    ) -> Result<Option<Entry>, Fail>;
    /// Clear a queue, or its entries matching a key.
    fn clear(&mut self, name: &str, lib: &str, order: &str, key: &[u8]) -> Result<(), Fail>;
}

/// The server.
pub struct DtaqServer<A, Q, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// The queues.
    pub queues: Q,
    /// Exit programs.
    pub exits: X,
}

fn common(f: &Frame, code: u16, msg: Option<&Fail>) -> Outcome {
    let params = msg
        .map(|(id, t)| {
            let mut d = text::ebcdic(id);
            d.extend(text::ebcdic(&format!(" {t}")));
            vec![Param::new(0x5003, d)]
        })
        .unwrap_or_default();
    Outcome::one(reply::frame(
        &f.header,
        id::COMMON_REPLY,
        code.to_be_bytes().to_vec(),
        params,
    ))
}

fn done(f: &Frame, r: Result<(), Fail>) -> Outcome {
    match r {
        Ok(()) => common(f, rc::OK, None),
        Err(e) => common(f, rc::ERROR, Some(&e)),
    }
}

fn authority(b: u8) -> &'static str {
    match b {
        0xF0 => "*ALL",
        0xF1 => "*CHANGE",
        0xF2 => "*EXCLUDE",
        0xF3 => "*USE",
        _ => "*LIBCRTAUT",
    }
}

fn order_text(b: &[u8]) -> String {
    text::from_ebcdic(b).trim().to_string()
}

impl<A: Auth, Q: DataQueues, X: Exits> DtaqServer<A, Q, X> {
    /// A server for one connection.
    pub fn new(auth: A, queues: Q, exits: X) -> Self {
        Self {
            session: Session::default(),
            auth,
            queues,
            exits,
        }
    }

    fn allowed(&mut self, function: i32, name: &str, lib: &str, op: &str, key: &[u8]) -> bool {
        let user = self.session.user.clone().unwrap_or_default();
        let req = ExitRequest::host(
            "ZHQ00100",
            zhq00100(
                &who(&user, function),
                &DataQueue {
                    queue: (name.into(), lib.into()),
                    key_op: op.into(),
                    key: key.to_vec(),
                },
            ),
        );
        self.exits.allow(EXIT_POINT, &req)
    }

    fn request(&mut self, f: &Frame) -> Outcome {
        let t = &f.template;
        let name = text::from_ebcdic(reply::slice(t, 0, 10));
        let lib = text::from_ebcdic(reply::slice(t, 10, 10));
        let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
        let key = f.param(cp::KEY).map(|p| p.data.clone()).unwrap_or_default();
        let data = f.param(cp::ENTRY).map(|p| p.data.clone()).unwrap_or_default();
        let function = i32::from(f.header.req_rep_id);
        let op = if f.header.req_rep_id == id::READ {
            order_text(reply::slice(t, 21, 2))
        } else {
            String::new()
        };
        if !self.allowed(function, &name, &lib, &op, &key) {
            return common(f, rc::EXIT_REJECTED, None);
        }
        match f.header.req_rep_id {
            id::CREATE => {
                let a = QueueAttrs {
                    max_len: reply::be32(t, 20),
                    authority: authority(t.get(24).copied().unwrap_or(0xF4)).into(),
                    sender: t.get(25) == Some(&0xF1),
                    sequence: match t.get(26) {
                        Some(0xF1) => 'L',
                        Some(0xF2) => 'K',
                        _ => 'F',
                    },
                    key_len: reply::be16(t, 27),
                    force: t.get(29) == Some(&0xF1),
                    text: text::from_ebcdic(reply::slice(t, 30, 50)),
                };
                let r = self.queues.create(&name, &lib, &a);
                done(f, r)
            }
            id::DELETE => {
                let r = self.queues.delete(&name, &lib);
                done(f, r)
            }
            id::WRITE => {
                let r = self.queues.write(&name, &lib, &key, &data);
                // Byte 21 asks for a reply.
                if t.get(21) == Some(&0xF0) && r.is_ok() {
                    return Outcome::none();
                }
                done(f, r)
            }
            id::CLEAR => {
                let order = if key.is_empty() { "" } else { "EQ" };
                let r = self.queues.clear(&name, &lib, order, &key);
                done(f, r)
            }
            id::READ => {
                let wait = i32::from_be_bytes(reply::slice(t, 23, 4).try_into().unwrap_or([0; 4]));
                let peek = t.get(27) == Some(&0xF1);
                match self.queues.read(&name, &lib, &op, &key, wait, peek) {
                    Ok(Some(e)) => {
                        let mut tp = rc::OK.to_be_bytes().to_vec();
                        let mut s = e.sender.clone();
                        s.resize(36, 0x40);
                        tp.extend(s);
                        let mut params = vec![Param::new(cp::ENTRY, e.data)];
                        if !op.is_empty() || !e.key.is_empty() {
                            params.push(Param::new(cp::KEY, e.key));
                        }
                        Outcome::one(reply::frame(&f.header, id::READ_REPLY, tp, params))
                    }
                    Ok(None) => common(f, rc::NO_ENTRY, None),
                    Err(e) => common(f, rc::ERROR, Some(&e)),
                }
            }
            id::ATTRIBUTES => match self.queues.attributes(&name, &lib) {
                Ok(a) => {
                    let mut tp = rc::OK.to_be_bytes().to_vec();
                    tp.extend(a.max_len.to_be_bytes());
                    tp.push(if a.sender { 0xF1 } else { 0xF0 });
                    tp.push(match a.sequence {
                        'L' => 0xF1,
                        'K' => 0xF2,
                        _ => 0xF0,
                    });
                    tp.extend(a.key_len.to_be_bytes());
                    tp.push(if a.force { 0xF1 } else { 0xF0 });
                    tp.extend(text::ebcdic_fixed(&a.text, 50));
                    Outcome::one(reply::frame(&f.header, id::ATTRIBUTES_REPLY, tp, Vec::new()))
                }
                Err(e) => common(f, rc::ERROR, Some(&e)),
            },
            _ => common(f, rc::PROTOCOL, None),
        }
    }
}

impl<A: Auth, Q: DataQueues, X: Exits> Handler for DtaqServer<A, Q, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        if let Some(o) = self.session.common(&f, &mut self.auth) {
            return o;
        }
        if !self.session.started() {
            return Outcome::close();
        }
        if f.header.req_rep_id == id::EXCHANGE_ATTRIBUTES {
            let mut t = rc::OK.to_be_bytes().to_vec();
            t.extend(text::ebcdic("V7R1M0"));
            t.extend([0, 0]);
            return Outcome::one(reply::frame(
                &f.header,
                id::EXCHANGE_ATTRIBUTES_REPLY,
                t,
                Vec::new(),
            ));
        }
        self.request(&f)
    }
}
