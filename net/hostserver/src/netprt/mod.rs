// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network print server (8474): the server's own attributes,
//! spooled file lists, attributes, data, hold, release, delete and
//! move, output queue and writer lists. The request ID is the object
//! type; every template is action, flags, return code and error offset;
//! a list answers one reply per object, flagged `0x80000000` while
//! more follow.

pub mod attrs;

use hostsvr::frame::{Frame, Param};
use hostsvr::netprt::{action, id};
use regfac::ExitRequest;
use regfac::formats::host::{SplfId, entr0100, func, splf0100};

use attrs::{Value, ids};

use crate::exits::{Exits, who};
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, reply};

/// The exit points.
pub const EXIT_ENTRY: &str = "QIBM_QNPS_ENTRY";
/// Spooled files.
pub const EXIT_SPLF: &str = "QIBM_QNPS_SPLF";

/// Code points.
pub mod cp {
    /// The object's ID (an attribute list).
    pub const ID: u16 = 0x0001;
    /// Spooled file data.
    pub const DATA: u16 = 0x0002;
    /// A list's selection.
    pub const SELECTION: u16 = 0x0006;
    /// Attributes to return.
    pub const ATTR_IDS: u16 = 0x0007;
    /// Attribute values.
    pub const ATTRS: u16 = 0x0008;
    /// The target output queue of a move.
    pub const TARGET_OUTQ: u16 = 0x000C;
}

/// Return codes.
pub mod rc {
    /// Success.
    pub const OK: u16 = 0;
    /// Not authorized.
    pub const NOT_AUTHORIZED: u16 = 8;
    /// A CPF message.
    pub const CPF_MESSAGE: u16 = 9;
    /// End of the data.
    pub const READ_EOF: u16 = 19;
    /// Nothing to list.
    pub const EMPTY_LIST: u16 = 20;
    /// Not supported.
    pub const NOT_SUPPORTED: u16 = 21;
    /// No such spooled file.
    pub const SPLF_NOT_FOUND: u16 = 30;
    /// An exit program refused.
    pub const EXIT_DENIED: u16 = 27;
}

/// A spooled file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Splf {
    /// Job name, user and number.
    pub job: (String, String, String),
    /// File name.
    pub file: String,
    /// File number.
    pub number: i32,
    /// The system.
    pub system: String,
    /// Created: `CYYMMDD`.
    pub date: String,
    /// Created: `HHMMSS`.
    pub time: String,
    /// `*READY`, `*HELD`, ...
    pub status: String,
    /// Pages.
    pub pages: i32,
    /// Copies.
    pub copies: i32,
    /// Output queue library and name.
    pub outq: (String, String),
    /// Printer file library and name.
    pub prtf: (String, String),
    /// User data.
    pub user_data: String,
    /// Form type.
    pub form_type: String,
    /// Device type (`*SCS`).
    pub device_type: String,
    /// Output priority.
    pub priority: String,
    /// Page length and width, lines per inch.
    pub page: (i64, i64, i64),
}

/// The job's spooling.
pub trait Spool {
    /// The system name.
    fn system(&mut self) -> String;
    /// Spooled files of `user` (`*CURRENT`, `*ALL` or a name), on `outq`
    /// when given.
    fn list(&mut self, user: &str, outq: Option<(String, String)>) -> Vec<Splf>;
    /// Hold, release or delete a spooled file; `Err` is a message.
    fn act(&mut self, s: &Splf, what: SplfAction) -> Result<(), String>;
    /// A spooled file's data.
    fn data(&mut self, s: &Splf) -> Result<Vec<u8>, String>;
    /// Output queues: library and name.
    fn outqs(&mut self) -> Vec<(String, String)>;
    /// Writers: name, status and output queue.
    fn writers(&mut self) -> Vec<(String, String, String)>;
}

/// What to do to a spooled file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SplfAction {
    /// Hold it.
    Hold,
    /// Release it.
    Release,
    /// Delete it.
    Delete,
    /// Move it to an output queue (library, name).
    Move(String, String),
}

/// The server.
pub struct NetPrint<A, S, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// Spooling.
    pub spool: S,
    /// Exit programs.
    pub exits: X,
    /// The server job: name, user, number.
    pub job: (String, String, String),
    open: Option<(Vec<u8>, usize)>,
}

fn template(act: u16, flags: u32, code: u16) -> Vec<u8> {
    let mut t = act.to_be_bytes().to_vec();
    t.extend(flags.to_be_bytes());
    t.extend(code.to_be_bytes());
    t.extend(0u32.to_be_bytes());
    t
}

fn text(s: &str, w: usize) -> Value {
    Value::Text(s.to_string(), w)
}

/// The ID attributes of a spooled file.
pub fn splf_id(s: &Splf) -> Vec<(u16, Value)> {
    vec![
        (ids::JOBNAME, text(&s.job.0, 10)),
        (ids::JOBNUMBER, text(&s.job.2, 6)),
        (ids::JOBUSER, text(&s.job.1, 10)),
        (ids::SPOOLFILE, text(&s.file, 10)),
        (ids::SPLFNUM, Value::Bin4(s.number)),
        (ids::JOBSYSTEM, text(&s.system, 8)),
        (ids::DATE, text(&s.date, 7)),
        (ids::TIME, text(&s.time, 6)),
    ]
}

/// The value of one attribute of a spooled file, when the server has it.
pub fn splf_attr(s: &Splf, a: u16) -> Option<Value> {
    Some(match a {
        ids::JOBNAME => text(&s.job.0, 10),
        ids::JOBNUMBER => text(&s.job.2, 6),
        ids::JOBUSER => text(&s.job.1, 10),
        ids::SPOOLFILE => text(&s.file, 10),
        ids::SPLFNUM => Value::Bin4(s.number),
        ids::JOBSYSTEM => text(&s.system, 8),
        ids::DATE => text(&s.date, 7),
        ids::TIME => text(&s.time, 6),
        ids::SPLFSTATUS => text(&s.status, 10),
        ids::PAGES => Value::Bin4(s.pages),
        ids::COPIES => Value::Bin4(s.copies),
        ids::OUTQUELIB => text(&s.outq.0, 10),
        ids::OUTQUE => text(&s.outq.1, 10),
        ids::PRTFLIB => text(&s.prtf.0, 10),
        ids::PRTFILE => text(&s.prtf.1, 10),
        ids::USERDATA => text(&s.user_data, 10),
        ids::FORMTYPE => text(&s.form_type, 10),
        ids::PRTDEVTYPE => text(&s.device_type, 10),
        ids::OUTPTY => text(&s.priority, 2),
        ids::PAGELEN => Value::Packed(s.page.0),
        ids::PAGEWIDTH => Value::Packed(s.page.1),
        ids::LPI => Value::Packed(s.page.2),
        _ => return None,
    })
}

/// The spooled file an ID code point names.
fn find(list: &[Splf], idd: &[u8]) -> Option<Splf> {
    let raws = attrs::decode(idd);
    let get = |i: u16| raws.iter().find(|r| r.id == i);
    let name = get(ids::SPOOLFILE)?.text();
    let number = get(ids::SPLFNUM).map_or(-1, attrs::Raw::bin4);
    let job = get(ids::JOBNAME).map(attrs::Raw::text).unwrap_or_default();
    let jnum = get(ids::JOBNUMBER).map(attrs::Raw::text).unwrap_or_default();
    list.iter()
        .find(|s| {
            s.file == name
                && (number <= 0 || s.number == number)
                && (job.is_empty() || s.job.0 == job)
                && (jnum.is_empty() || s.job.2 == jnum)
        })
        .cloned()
}

impl<A: Auth, S: Spool, X: Exits> NetPrint<A, S, X> {
    /// A server for one connection; `job` names the server job.
    pub fn new(auth: A, spool: S, exits: X, job: (String, String, String)) -> Self {
        Self {
            session: Session::default(),
            auth,
            spool,
            exits,
            job,
            open: None,
        }
    }

    fn user(&self) -> String {
        self.session.user.clone().unwrap_or_default()
    }

    fn rid(f: &Frame) -> u16 {
        0x8000 | f.header.req_rep_id
    }

    fn act(f: &Frame) -> u16 {
        reply::be16(&f.template, 0)
    }

    fn answer(f: &Frame, code: u16, params: Vec<Param>) -> Vec<u8> {
        reply::frame(&f.header, Self::rid(f), template(Self::act(f), 0, code), params)
    }

    fn server(&mut self, f: &Frame) -> Outcome {
        match Self::act(f) {
            action::RETRIEVE_ATTRIBUTES => {
                let want = f.param(cp::ATTR_IDS).map(|p| attrs::ids_of(&p.data)).unwrap_or_default();
                let mut out = Vec::new();
                for a in want {
                    let v = match a {
                        ids::NPSCCSID => Value::Bin4(37),
                        ids::JOBNAME => text(&self.job.0, 10),
                        ids::JOBUSER => text(&self.job.1, 10),
                        ids::JOBNUMBER => text(&self.job.2, 6),
                        ids::NPSLEVEL => text("V7R1M0", 0),
                        _ => continue,
                    };
                    out.push((a, v));
                }
                Outcome::one(Self::answer(f, rc::OK, vec![Param::new(cp::ATTRS, attrs::encode(&out))]))
            }
            action::CHANGE_ATTRIBUTES => {
                let p = f.param(cp::ATTRS).cloned().into_iter().collect();
                Outcome::one(Self::answer(f, rc::OK, p))
            }
            _ => Outcome::one(Self::answer(f, rc::NOT_SUPPORTED, Vec::new())),
        }
    }

    fn selection(f: &Frame) -> (String, Option<(String, String)>) {
        let raws = f.param(cp::SELECTION).map(|p| attrs::decode(&p.data)).unwrap_or_default();
        let get = |i: u16| raws.iter().find(|r| r.id == i).map(attrs::Raw::text);
        let user = get(ids::JOBUSER).unwrap_or_else(|| "*CURRENT".into());
        let outq = get(ids::OUTQUE).filter(|q| !q.is_empty() && q != "*ALL").map(|q| {
            (get(ids::OUTQUELIB).unwrap_or_else(|| "*LIBL".into()), q)
        });
        (user, outq)
    }

    fn user_of(&self, user: &str) -> String {
        if user == "*CURRENT" { self.user() } else { user.to_string() }
    }

    fn spooled(&mut self, f: &Frame) -> Outcome {
        let who_ = who(&self.user(), func::NPS_SPLF);
        match Self::act(f) {
            action::LIST => {
                let (user, outq) = Self::selection(f);
                let user = self.user_of(&user);
                let want = f.param(cp::ATTR_IDS).map(|p| attrs::ids_of(&p.data)).unwrap_or_default();
                let list = self.spool.list(&user, outq);
                if list.is_empty() {
                    return Outcome::one(Self::answer(f, rc::EMPTY_LIST, Vec::new()));
                }
                let n = list.len();
                let replies = list
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let values: Vec<(u16, Value)> = want.iter().filter_map(|a| splf_attr(s, *a).map(|v| (*a, v))).collect();
                        let mut params = vec![Param::new(cp::ID, attrs::encode(&splf_id(s)))];
                        if !want.is_empty() {
                            params.push(Param::new(cp::ATTRS, attrs::encode(&values)));
                        }
                        let flags = if i + 1 < n { 0x8000_0000 } else { 0 };
                        reply::frame(&f.header, Self::rid(f), template(action::LIST, flags, rc::OK), params)
                    })
                    .collect();
                Outcome {
                    replies,
                    close: false,
                }
            }
            act => {
                let Some(idp) = f.param(cp::ID) else {
                    return Outcome::one(Self::answer(f, rc::SPLF_NOT_FOUND, Vec::new()));
                };
                let user = self.user();
                let all = self.spool.list("*ALL", None);
                let Some(s) = find(&all, &idp.data).or_else(|| find(&self.spool.list(&user, None), &idp.data)) else {
                    return Outcome::one(Self::answer(f, rc::SPLF_NOT_FOUND, Vec::new()));
                };
                let sid = SplfId {
                    job: s.job.clone(),
                    name: s.file.clone(),
                    number: s.number,
                    system: s.system.clone(),
                    created: (s.date.clone(), s.time.clone()),
                };
                let req = ExitRequest::host("SPLF0100", splf0100(&who_, &sid, &[]));
                if act != action::RETRIEVE_ATTRIBUTES && !self.exits.allow(EXIT_SPLF, &req) {
                    return Outcome::one(Self::answer(f, rc::EXIT_DENIED, Vec::new()));
                }
                let done = |r: Result<(), String>| match r {
                    Ok(()) => rc::OK,
                    Err(_) => rc::CPF_MESSAGE,
                };
                match act {
                    action::RETRIEVE_ATTRIBUTES => {
                        let want = f.param(cp::ATTR_IDS).map(|p| attrs::ids_of(&p.data)).unwrap_or_default();
                        let values: Vec<(u16, Value)> = want.iter().filter_map(|a| splf_attr(&s, *a).map(|v| (*a, v))).collect();
                        Outcome::one(Self::answer(f, rc::OK, vec![Param::new(cp::ATTRS, attrs::encode(&values))]))
                    }
                    action::HOLD => Outcome::one(Self::answer(f, done(self.spool.act(&s, SplfAction::Hold)), Vec::new())),
                    action::RELEASE => Outcome::one(Self::answer(f, done(self.spool.act(&s, SplfAction::Release)), Vec::new())),
                    action::DELETE => Outcome::one(Self::answer(f, done(self.spool.act(&s, SplfAction::Delete)), Vec::new())),
                    action::MOVE => {
                        let raws = f.param(cp::TARGET_OUTQ).map(|p| attrs::decode(&p.data)).unwrap_or_default();
                        let get = |i: u16| raws.iter().find(|r| r.id == i).map(attrs::Raw::text).unwrap_or_default();
                        let to = SplfAction::Move(get(ids::OUTQUELIB), get(ids::OUTQUE));
                        Outcome::one(Self::answer(f, done(self.spool.act(&s, to)), Vec::new()))
                    }
                    action::OPEN => match self.spool.data(&s) {
                        Ok(d) => {
                            self.open = Some((d, 0));
                            Outcome::one(Self::answer(f, rc::OK, Vec::new()))
                        }
                        Err(_) => Outcome::one(Self::answer(f, rc::CPF_MESSAGE, Vec::new())),
                    },
                    _ => Outcome::one(Self::answer(f, rc::NOT_SUPPORTED, Vec::new())),
                }
            }
        }
    }

    /// Reading an opened spooled file: `READ` (byte count in the
    /// attributes) and `CLOSE` arrive without an ID.
    fn read_open(&mut self, f: &Frame) -> Option<Outcome> {
        let act = Self::act(f);
        if f.header.req_rep_id != id::SPOOLED_FILE || f.param(cp::ID).is_some() {
            return None;
        }
        match act {
            action::READ => {
                let (d, at) = self.open.as_mut()?;
                let n = 4096.min(d.len() - *at);
                let chunk = d[*at..*at + n].to_vec();
                *at += n;
                let code = if n == 0 { rc::READ_EOF } else { rc::OK };
                Some(Outcome::one(Self::answer(f, code, vec![Param::new(cp::DATA, chunk)])))
            }
            action::CLOSE => {
                self.open = None;
                Some(Outcome::one(Self::answer(f, rc::OK, Vec::new())))
            }
            _ => None,
        }
    }

    fn list_names(&mut self, f: &Frame) -> Outcome {
        if Self::act(f) != action::LIST {
            return Outcome::one(Self::answer(f, rc::NOT_SUPPORTED, Vec::new()));
        }
        let ids_: Vec<Vec<(u16, Value)>> = if f.header.req_rep_id == id::OUTPUT_QUEUE {
            self.spool
                .outqs()
                .into_iter()
                .map(|(l, n)| vec![(ids::OUTQUELIB, text(&l, 10)), (ids::OUTQUE, text(&n, 10))])
                .collect()
        } else {
            self.spool
                .writers()
                .into_iter()
                .map(|(w, _, _)| vec![(ids::WRITER, text(&w, 10))])
                .collect()
        };
        if ids_.is_empty() {
            return Outcome::one(Self::answer(f, rc::EMPTY_LIST, Vec::new()));
        }
        let n = ids_.len();
        let replies = ids_
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let flags = if i + 1 < n { 0x8000_0000 } else { 0 };
                reply::frame(&f.header, Self::rid(f), template(action::LIST, flags, rc::OK), vec![Param::new(cp::ID, attrs::encode(v))])
            })
            .collect();
        Outcome {
            replies,
            close: false,
        }
    }
}

impl<A: Auth, S: Spool, X: Exits> Handler for NetPrint<A, S, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        if let Some(o) = self.session.common(&f, &mut self.auth) {
            if o.replies.len() == 1 && f.header.req_rep_id == hostsvr::common::id::START_SERVER && self.session.started() {
                let req = ExitRequest::host("ENTR0100", entr0100(&who(&self.user(), func::NPS_ENTRY)));
                if !self.exits.allow(EXIT_ENTRY, &req) {
                    return Outcome::close();
                }
            }
            return o;
        }
        if !self.session.started() {
            return Outcome::close();
        }
        if let Some(o) = self.read_open(&f) {
            return o;
        }
        match f.header.req_rep_id {
            id::NP_SERVER => self.server(&f),
            id::SPOOLED_FILE => self.spooled(&f),
            id::OUTPUT_QUEUE | id::WRITER_JOB => self.list_names(&f),
            _ => Outcome::one(Self::answer(&f, rc::NOT_SUPPORTED, Vec::new())),
        }
    }
}
