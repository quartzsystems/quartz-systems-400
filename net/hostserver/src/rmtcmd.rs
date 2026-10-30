// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The remote command and distributed program call server (8475):
//! commands through the job's command analyzer, program calls with
//! their parameters marshalled into the job's storage, and the job's
//! messages returned.

use hostsvr::frame::{Frame, Param};
use hostsvr::message::HostMessage;
use hostsvr::rmtcmd::{ProgramParameter, id, message_option, rc, usage};
use hostsvr::cp::rmtcmd as cp;
use regfac::ExitRequest;
use regfac::formats::host::{Remote, czrc0100};

use crate::exits::Exits;
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, SERVER_LEVEL, SERVER_VRM, reply, text};

/// The exit point.
pub const EXIT_POINT: &str = "QIBM_QZRC_RMT";

/// A message the job sent while running a request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobMessage {
    /// Message ID (`CPF9801`).
    pub id: String,
    /// Type: 1 completion, 2 diagnostic, 4 informational, 5 inquiry,
    /// 15 escape.
    pub kind: u8,
    /// Severity.
    pub severity: u16,
    /// Message file and library.
    pub file: (String, String),
    /// First level text.
    pub text: String,
    /// Substitution data.
    pub data: Vec<u8>,
    /// Second level text.
    pub help: String,
}

impl JobMessage {
    /// The extended layout (`0x1106`), text in CCSID 37.
    pub fn to_host(&self) -> HostMessage {
        // Clients split every field: none is sent empty.
        let or = |s: &str, d: &str| if s.is_empty() { d.to_string() } else { s.to_string() };
        let file = (or(&self.file.0, "QCPFMSG"), or(&self.file.1, "QSYS"));
        let text = or(&self.text, &self.id);
        let help = or(&self.help, &text);
        HostMessage {
            text_ccsid: 0xFFFF,
            substitution_ccsid: 0xFFFF,
            severity: self.severity,
            kind: text::ebcdic(&format!("{:02}", self.kind)),
            id: text::ebcdic(&self.id),
            file: text::ebcdic_fixed(&file.0, 10),
            library: text::ebcdic_fixed(&file.1, 10),
            text: text::ebcdic(&text),
            substitution: self.data.clone(),
            help: text::ebcdic(&help),
            rest: Vec::new(),
        }
    }
}

/// How a program call went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallResult {
    /// It ran; the buffers hold the outputs.
    Ran(Vec<JobMessage>),
    /// It could not be called or ended in an exception.
    Failed(Vec<JobMessage>),
}

/// The job behind the server.
pub trait Commands {
    /// The job's CCSID.
    fn job_ccsid(&mut self) -> u32 {
        37
    }
    /// Run a CL command; whether it succeeded and the messages it left.
    fn run(&mut self, command: &str) -> (bool, Vec<JobMessage>);
    /// Call `library/program` with the parameters by reference.
    fn call(&mut self, program: &str, library: &str, parms: &mut [Vec<u8>]) -> CallResult;
}

/// The server.
pub struct RmtCmd<A, C, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// The job.
    pub job: C,
    /// Exit programs.
    pub exits: X,
    /// The datastream level agreed on.
    pub level: u16,
}

impl<A: Auth, C: Commands, X: Exits> RmtCmd<A, C, X> {
    /// A server for one connection.
    pub fn new(auth: A, job: C, exits: X) -> Self {
        Self {
            session: Session::default(),
            auth,
            job,
            exits,
            level: SERVER_LEVEL,
        }
    }

    fn user(&self) -> String {
        self.session.user.clone().unwrap_or_default()
    }

    fn exchange(&mut self, f: &Frame) -> Outcome {
        let nlv = reply::slice(&f.template, 4, 4).to_vec();
        self.level = reply::be16(&f.template, 12).min(SERVER_LEVEL);
        let mut t = rc::OK.to_be_bytes().to_vec();
        t.extend(self.job.job_ccsid().to_be_bytes());
        t.extend(if nlv.len() == 4 { nlv } else { text::ebcdic("2924") });
        t.extend(SERVER_VRM.to_be_bytes());
        t.extend(SERVER_LEVEL.to_be_bytes());
        Outcome::one(reply::frame(
            &f.header,
            id::EXCHANGE_ATTRIBUTES_REPLY,
            t,
            Vec::new(),
        ))
    }

    fn messages(option: u8, msgs: &[JobMessage]) -> Vec<Param> {
        let max = match option {
            message_option::NONE => 0,
            message_option::ALL | message_option::ALL_EXTENDED | message_option::ALL_LEVEL_11 => {
                usize::MAX
            }
            _ => 10,
        };
        msgs.iter()
            .take(max)
            .map(|m| Param::new(cp::MESSAGE_EXTENDED, m.to_host().encode()))
            .collect()
    }

    fn run_command(&mut self, f: &Frame) -> Outcome {
        let option = f.template.first().copied().unwrap_or(0);
        let (cmd_text, ccsid) = match hostsvr::rmtcmd::command(&f.params) {
            Some((Some(c), b)) => (text::decode(c, &b), c),
            Some((None, b)) => (text::decode(self.job.job_ccsid(), &b), 37),
            None => (String::new(), 37),
        };
        let req = ExitRequest::host(
            "CZRC0100",
            czrc0100(
                &self.user(),
                &Remote::Command {
                    ccsid: i32::try_from(ccsid).unwrap_or(37),
                    text: text::ebcdic(&cmd_text),
                },
            ),
        );
        let (ok, msgs) = if self.exits.allow(EXIT_POINT, &req) {
            self.job.run(cmd_text.trim())
        } else {
            (false, vec![refused()])
        };
        let params = Self::messages(option, &msgs);
        let mut t = if ok { rc::OK } else { rc::COMMAND_FAILED }
            .to_be_bytes()
            .to_vec();
        t.extend(reply::len16(params.len()).to_be_bytes());
        Outcome::one(reply::frame(&f.header, id::RUN_COMMAND_REPLY, t, params))
    }

    fn call_program(&mut self, f: &Frame) -> Outcome {
        let program = text::from_ebcdic(reply::slice(&f.template, 0, 10));
        let library = text::from_ebcdic(reply::slice(&f.template, 10, 10));
        let option = f.template.get(20).copied().unwrap_or(0);
        let parms: Vec<ProgramParameter> = hostsvr::rmtcmd::parameters(&f.params).unwrap_or_default();
        let mut bufs: Vec<Vec<u8>> = parms
            .iter()
            .map(|p| {
                if p.usage == usage::NULL {
                    Vec::new()
                } else if p.usage % 10 == 2 {
                    vec![0; p.length as usize]
                } else {
                    p.value().unwrap_or_else(|_| vec![0; p.length as usize])
                }
            })
            .collect();
        let info: Vec<u8> = parms
            .iter()
            .flat_map(|p| {
                let mut v = p.length.to_be_bytes().to_vec();
                v.extend(p.usage.to_be_bytes());
                v
            })
            .collect();
        let req = ExitRequest::host(
            "CZRC0100",
            czrc0100(
                &self.user(),
                &Remote::Program {
                    program: (program.clone(), library.clone()),
                    parms: info,
                },
            ),
        );
        let result = if self.exits.allow(EXIT_POINT, &req) {
            self.job.call(&program, &library, &mut bufs)
        } else {
            CallResult::Failed(vec![refused()])
        };
        match result {
            CallResult::Ran(_) => {
                let mut out = Vec::new();
                for (p, b) in parms.iter().zip(&bufs) {
                    let base = p.usage % 10;
                    if p.usage == usage::NULL || !(base == 2 || base == 3) {
                        continue;
                    }
                    let mut data = b.clone();
                    data.truncate(p.length as usize);
                    while data.last() == Some(&0) {
                        data.pop();
                    }
                    out.push(
                        ProgramParameter {
                            length: p.length,
                            usage: 10 + base,
                            data,
                        }
                        .to_param(),
                    );
                }
                let mut t = rc::OK.to_be_bytes().to_vec();
                t.extend(reply::len16(out.len()).to_be_bytes());
                Outcome::one(reply::frame(&f.header, id::CALL_PROGRAM_REPLY, t, out))
            }
            CallResult::Failed(msgs) => {
                let out = Self::messages(option, &msgs);
                let mut t = rc::PROGRAM_FAILED.to_be_bytes().to_vec();
                t.extend(reply::len16(out.len()).to_be_bytes());
                Outcome::one(reply::frame(&f.header, id::CALL_PROGRAM_FAILED, t, out))
            }
        }
    }
}

/// The message an exit program's refusal leaves.
pub fn refused() -> JobMessage {
    JobMessage {
        id: "CPF2234".into(),
        kind: 15,
        severity: 40,
        file: ("QCPFMSG".into(), "QSYS".into()),
        text: "The exit program for this server did not allow the request.".into(),
        ..JobMessage::default()
    }
}

impl<A: Auth, C: Commands, X: Exits> Handler for RmtCmd<A, C, X> {
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
        match f.header.req_rep_id {
            id::EXCHANGE_ATTRIBUTES => self.exchange(&f),
            id::RUN_COMMAND => self.run_command(&f),
            id::CALL_PROGRAM => self.call_program(&f),
            // End (and anything unknown) closes.
            _ => Outcome::close(),
        }
    }
}
