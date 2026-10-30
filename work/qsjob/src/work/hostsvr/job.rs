// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server job's work for the host servers: commands through the
//! command analyzer as the job, program calls in the job's runtime,
//! exit programs, data queues and spooled files over the link.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use hostserver::dtaq::{DataQueues, Entry, Fail, QueueAttrs};
use hostserver::exits::Exits;
use hostserver::netprt::{Splf, SplfAction, Spool};
use hostserver::rmtcmd::{CallResult, Commands, JobMessage};
use regfac::ExitRequest;
use stg::{Client, Shared};

use crate::console::Console;

/// The job's runtime, shared by a server's backends.
pub type Job = Rc<RefCell<Console>>;

fn kind_of(k: &str) -> u8 {
    match k {
        "Completion" => 1,
        "Diagnostic" => 2,
        "Information" => 4,
        "Inquiry" => 5,
        "Notify" => 14,
        _ => 15,
    }
}

/// Run a command as the job: whether it ended without an escape, and
/// the messages it left in the job log.
fn run(job: &Job, command: &str) -> (bool, Vec<JobMessage>) {
    let mut c = job.borrow_mut();
    let host = c.host.clone();
    let before = c.job().log_outbox.len();
    let o = crate::command::run(c.job(), &host, command);
    let entries: Vec<wm::joblog::Entry> = c.job().log_outbox.get(before..).map(<[_]>::to_vec).unwrap_or_default();
    c.sync_log();
    let mut msgs: Vec<JobMessage> = entries
        .iter()
        .filter(|e| !e.msgid.trim().is_empty())
        .map(|e| JobMessage {
            id: e.msgid.clone(),
            kind: kind_of(&e.kind),
            severity: e.severity,
            file: ("QCPFMSG".into(), "QSYS".into()),
            text: e.text.clone(),
            data: Vec::new(),
            help: e.help.clone(),
        })
        .collect();
    if let Some(f) = &o.escape
        && !msgs.iter().any(|m| m.id == f.msgid)
    {
        msgs.push(JobMessage {
            id: f.msgid.clone(),
            kind: 15,
            severity: 40,
            file: ("QCPFMSG".into(), "QSYS".into()),
            text: f.text.clone(),
            data: f.data.clone(),
            help: String::new(),
        });
    }
    (o.escape.is_none(), msgs)
}

/// Commands and program calls.
pub struct JobCommands(pub Job);

impl Commands for JobCommands {
    fn run(&mut self, command: &str) -> (bool, Vec<JobMessage>) {
        run(&self.0, command)
    }

    fn call(&mut self, program: &str, library: &str, parms: &mut [Vec<u8>]) -> CallResult {
        let mut c = self.0.borrow_mut();
        let lib = if library.is_empty() { "*LIBL" } else { library };
        let r = regfac::ProgramCaller::call(c.job(), program, lib, parms);
        c.sync_log();
        match r {
            Ok(()) => CallResult::Ran(Vec::new()),
            // A program that cannot be resolved is MCH3401 with the
            // object's type and name, which clients read.
            Err(why) if why.contains("could not be called") => {
                let mut data = vec![0x02, 0x01];
                data.extend(hostserver::text::ebcdic_fixed(program, 30));
                data.extend([0; 16]);
                CallResult::Failed(vec![JobMessage {
                    id: "MCH3401".into(),
                    kind: 15,
                    severity: 40,
                    file: ("QCPFMSG".into(), "QSYS".into()),
                    text: format!("Program {lib}/{program} cannot be found."),
                    data,
                    help: String::new(),
                }])
            }
            Err(why) => {
                let id = why.split(':').next().unwrap_or("CPF9811").trim().to_string();
                CallResult::Failed(vec![JobMessage {
                    id,
                    kind: 15,
                    severity: 40,
                    file: ("QCPFMSG".into(), "QSYS".into()),
                    text: why,
                    ..JobMessage::default()
                }])
            }
        }
    }
}

/// Exit programs, called in the job.
pub struct JobExits(pub Job);

impl Exits for JobExits {
    fn allow(&mut self, point: &str, req: &ExitRequest) -> bool {
        let mut c = self.0.borrow_mut();
        let link = c.host.link.clone();
        let mut reg = stg::remote::RemoteRegistry::new(link);
        regfac::call_exit(&mut reg, c.job(), point, req).allow
    }
}

/// Data queues: the queue APIs' requests, `CRTDTAQ` and `DLTDTAQ`.
pub struct JobQueues {
    /// The job.
    pub job: Job,
    /// Its link.
    pub link: Shared,
}

fn fail(r: &(bool, Vec<JobMessage>)) -> Result<(), Fail> {
    if r.0 {
        return Ok(());
    }
    let m = r.1.iter().rev().find(|m| m.kind == 15).or(r.1.last());
    Err(m.map_or_else(|| ("CPF9898".into(), String::new()), |m| (m.id.clone(), bare(&m.id, &m.text))))
}

/// A message's text without the identifier some texts start with.
fn bare(id: &str, text: &str) -> String {
    text.strip_prefix(id).map_or(text, str::trim_start).to_string()
}

fn api(e: &sysapi::ApiFail) -> Fail {
    (e.msgid.clone(), format!("{} ended the request.", e.msgid))
}

impl DataQueues for JobQueues {
    fn create(&mut self, name: &str, lib: &str, a: &QueueAttrs) -> Result<(), Fail> {
        let seq = match a.sequence {
            'L' => "*LIFO",
            'K' => "*KEYED",
            _ => "*FIFO",
        };
        let mut cmd = format!("CRTDTAQ DTAQ({lib}/{name}) MAXLEN({}) SEQ({seq})", a.max_len.max(1));
        if a.sequence == 'K' {
            let _ = write!(cmd, " KEYLEN({})", a.key_len.max(1));
        }
        cmd.push_str(if a.sender { " SENDERID(*YES)" } else { " SENDERID(*NO)" });
        let _ = write!(cmd, " AUT({})", a.authority);
        if !a.text.is_empty() {
            let _ = write!(cmd, " TEXT('{}')", a.text.replace('\'', "''"));
        }
        fail(&run(&self.job, &cmd))
    }

    fn delete(&mut self, name: &str, lib: &str) -> Result<(), Fail> {
        fail(&run(&self.job, &format!("DLTDTAQ DTAQ({lib}/{name})")))
    }

    fn attributes(&mut self, name: &str, lib: &str) -> Result<QueueAttrs, Fail> {
        let d = self
            .link
            .borrow_mut()
            .with(|l| l.api_dtaq_describe(lib.into(), name.into()))
            .map_err(|e| api(&e))?;
        Ok(QueueAttrs {
            max_len: d.max_len,
            key_len: u16::try_from(d.key_len).unwrap_or(0),
            sequence: d.sequence,
            sender: d.sender_id,
            force: false,
            text: d.text,
            authority: "*LIBCRTAUT".into(),
        })
    }

    fn write(&mut self, name: &str, lib: &str, key: &[u8], data: &[u8]) -> Result<(), Fail> {
        let user = self.link.borrow_mut().with(Client::whoami).user;
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaq_send(lib.into(), name.into(), key.to_vec(), data.to_vec(), user))
            .map_err(|e| api(&e))
    }

    fn read(&mut self, name: &str, lib: &str, order: &str, key: &[u8], wait: i32, peek: bool) -> Result<Option<Entry>, Fail> {
        let until = (wait >= 0).then(|| Instant::now() + Duration::from_secs(u64::try_from(wait).unwrap_or(0)));
        loop {
            let got = self
                .link
                .borrow_mut()
                .with(|l| l.api_dtaq_receive(lib.into(), name.into(), order.into(), key.to_vec(), !peek))
                .map_err(|e| api(&e))?;
            if let Some(e) = got {
                return Ok(Some(Entry {
                    data: e.data,
                    key: e.key,
                    sender: e.sender,
                }));
            }
            if until.is_some_and(|t| Instant::now() >= t) {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn clear(&mut self, name: &str, lib: &str, order: &str, key: &[u8]) -> Result<(), Fail> {
        self.link
            .borrow_mut()
            .with(|l| l.api_dtaq_clear(lib.into(), name.into(), order.into(), key.to_vec()))
            .map_err(|e| api(&e))
    }
}

/// Spooled files over the link; changes as the spool commands.
pub struct JobSpool {
    /// The job.
    pub job: Job,
    /// Its link.
    pub link: Shared,
    /// The system name.
    pub system: String,
}

/// `CYYMMDD` and `HHMMSS` of a Unix time.
#[allow(clippy::many_single_char_names)]
fn created(secs: u64) -> (String, String) {
    let (y, m, d, h, mi, s) = hostserver::signon::stamp_of(secs);
    (
        format!("{}{:02}{:02}{:02}", u16::from(y >= 2000), y % 100, m, d),
        format!("{h:02}{mi:02}{s:02}"),
    )
}

impl Spool for JobSpool {
    fn system(&mut self) -> String {
        self.system.clone()
    }

    fn list(&mut self, user: &str, outq: Option<(String, String)>) -> Vec<Splf> {
        let who = if user == "*ALL" { String::new() } else { user.to_string() };
        let all = self.link.borrow_mut().with(|l| l.splf_list(who, String::new()));
        all.into_iter()
            .filter(|a| outq.as_ref().is_none_or(|(_, q)| a.outq.rsplit('/').next() == Some(q.as_str())))
            .map(|a| {
                let (date, time) = created(a.created);
                let (ql, qn) = a.outq.split_once('/').map_or(("QGPL".to_string(), a.outq.clone()), |(l, n)| (l.into(), n.into()));
                Splf {
                    job: (a.job.0.clone(), a.job.1.clone(), format!("{:06}", a.job.2)),
                    file: a.name.clone(),
                    number: i32::try_from(a.number).unwrap_or(0),
                    system: self.system.clone(),
                    date,
                    time,
                    status: format!("*{}", match a.status.code() {
                        "RDY" => "READY",
                        "HLD" => "HELD",
                        "SAV" => "SAVED",
                        "WTR" => "WRITING",
                        "OPN" => "OPEN",
                        "CLO" => "CLOSED",
                        "PND" => "PENDING",
                        "DFR" => "DEFERRED",
                        _ => "MESSAGE",
                    }),
                    pages: i32::try_from(a.pages).unwrap_or(0),
                    copies: i32::from(a.copies),
                    outq: (ql, qn),
                    prtf: ("QSYS".into(), a.name.clone()),
                    user_data: a.usrdta.clone(),
                    form_type: a.formtype.clone(),
                    device_type: if a.devtype.is_empty() { "*SCS".into() } else { a.devtype.clone() },
                    priority: a.outpty.to_string(),
                    page: (i64::from(a.page_len), i64::from(a.page_width), i64::from(a.lpi)),
                }
            })
            .collect()
    }

    fn act(&mut self, s: &Splf, what: SplfAction) -> Result<(), String> {
        let id = format!("FILE({}) JOB({}/{}/{}) SPLNBR({})", s.file, s.job.2, s.job.1, s.job.0, s.number);
        let cmd = match what {
            SplfAction::Hold => format!("HLDSPLF {id}"),
            SplfAction::Release => format!("RLSSPLF {id}"),
            SplfAction::Delete => format!("DLTSPLF {id}"),
            SplfAction::Move(l, q) => format!("CHGSPLFA {id} OUTQ({}/{q})", if l.is_empty() { "*LIBL" } else { &l }),
        };
        fail(&run(&self.job, &cmd)).map_err(|(id, t)| format!("{id} {t}"))
    }

    fn data(&mut self, _s: &Splf) -> Result<Vec<u8>, String> {
        // BLOCKED(phase10): no request reads a spooled file's bytes for
        // a job that is not its writer.
        Err("CPF3492 spooled file data is not served here.".into())
    }

    fn outqs(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn writers(&mut self) -> Vec<(String, String, String)> {
        self.link
            .borrow_mut()
            .with(Client::wtr_list)
            .into_iter()
            .map(|w| (w.0, w.2, w.4))
            .collect()
    }
}
