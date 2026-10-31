// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a job's commands reach besides the job itself: the storage
//! service, the control channel to the parent (output, the display),
//! the inbox, the jobs it started, its command definitions and `*LDA`.
//! Every part is shared, so a command run from inside a program (the
//! `QCMDEXC` hook) has the same reach as one typed at the console.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use ipc::chan::Chan;
use mi::Job;
use stg::proto::ExecReply;
use stg::remote::{Lda, RemoteCmds};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Panel, Start, ToJob};
use wm::job::JobAttrs;
use wm::obj::msgq::QMsg;

use super::Inbox;
use crate::work::spawn::Spawned;

/// How often a waiting job looks for break messages.
pub const BREAK_POLL: Duration = Duration::from_secs(1);

/// Raise the runtime's attention flag every `BREAK_POLL`, so a running
/// program looks for break messages at its next statement. One timer
/// serves every runtime job of the process.
pub fn attention_timer() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(BREAK_POLL);
                mi::rtlib::ATTN.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
    });
}

/// The job's surroundings.
#[derive(Clone)]
pub struct Host {
    /// The storage service.
    pub link: Shared,
    /// The control channel to the parent.
    pub ctl: Rc<RefCell<Chan>>,
    /// What the parent sends, read on a thread.
    pub inbox: Inbox,
    /// Jobs this one started (subsystem monitors), until they end.
    pub children: Rc<RefCell<Vec<Spawned>>>,
    /// Command definitions.
    pub cmds: Rc<RefCell<RemoteCmds>>,
    /// The job's `*LDA`.
    pub lda: Lda,
    /// Who the job is.
    pub start: Rc<Start>,
    /// The job's attributes as last fetched; `None` until needed.
    pub attrs: Rc<RefCell<Option<JobAttrs>>>,
    /// The job's own display device, when it has one (every
    /// interactive job; not `QSVCJOB` nor a batch job).
    pub display: Rc<RefCell<Option<crate::display::Display>>>,
    /// What the inbox delivered while a program ran, for the next wait.
    pub pending: Rc<RefCell<VecDeque<ToJob>>>,
}

impl Host {
    /// Text for the console now.
    pub fn output(&self, text: &str) {
        let _ = self.ctl.borrow_mut().send(&FromJob::Output {
            text: text.to_string(),
        });
    }

    /// Show a panel on the job's display.
    pub fn show(&self, panel: Panel) {
        if let Some(d) = self.display.borrow_mut().as_mut() {
            d.show(&panel);
            return;
        }
        let _ = self.ctl.borrow_mut().send(&FromJob::Show { panel });
    }

    /// Whether the job has a display to show panels on.
    #[must_use]
    pub fn has_display(&self) -> bool {
        self.start.role == "INTER"
    }

    /// Whether the job is interactive (its commands are `*INTERACT`).
    #[must_use]
    pub fn interactive(&self) -> bool {
        matches!(self.start.role.as_str(), "INTER" | "QSVCJOB")
    }

    /// `LMTCPB(*YES)`: only commands that allow limited users.
    #[must_use]
    pub fn limited(&self) -> bool {
        self.start
            .signon
            .as_ref()
            .is_some_and(|s| s.lmtcpb == "*YES")
    }

    /// The next message for the job: one put aside first, else the
    /// inbox's within `d`.
    pub fn next(&self, d: Duration) -> Result<ToJob, RecvTimeoutError> {
        if let Some(m) = self.pending.borrow_mut().pop_front() {
            return Ok(m);
        }
        self.inbox.recv_timeout(d)
    }

    /// Wait for the display to answer: the key (0 for Enter) and the
    /// fields. Breaks are taken meanwhile, and System Request,
    /// Attention and Print are the system's (`crate::sysdsp::keys`);
    /// `None` when the job is to end or the request was ended.
    pub fn wait_input(&self, job: &mut Job) -> Option<(u8, Vec<String>)> {
        use crate::sysdsp::keys;
        loop {
            if keys::unwinding() {
                return None;
            }
            match self.next(BREAK_POLL) {
                Ok(ToJob::Input { key, fields }) => return Some((key, fields)),
                Ok(ToJob::Record { flags, data, .. }) => {
                    let got = self
                        .display
                        .borrow_mut()
                        .as_mut()
                        .and_then(|d| d.accept(flags, &data));
                    match got {
                        Some((k, _)) if keys::is_system(k) => {
                            if keys::take(self, k) == keys::After::Unwind {
                                return None;
                            }
                        }
                        Some(r) => return Some(r),
                        None => {}
                    }
                }
                Ok(ToJob::DeviceReset { device }) => {
                    if let Some(d) = self.display.borrow_mut().as_mut() {
                        d.reset(device);
                    }
                }
                Ok(ToJob::End { .. }) | Err(RecvTimeoutError::Disconnected) => return None,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => self.breaks(job),
            }
        }
    }

    /// Wait up to `d`, taking breaks; `false` when the job is to end.
    pub fn idle(&self, job: &mut Job, d: Duration) -> bool {
        match self.next(d) {
            Ok(ToJob::End { .. }) | Err(RecvTimeoutError::Disconnected) => false,
            _ => {
                self.breaks(job);
                true
            }
        }
    }

    /// The attention hook of a running program: the keys the display
    /// sent meanwhile (System Request, Attention) are taken now, the
    /// rest put aside for the next wait; then the breaks. A request
    /// ended from System Request cancels the program to its control
    /// boundary.
    pub fn attention(&self, job: &mut Job) {
        use crate::sysdsp::keys;
        while let Ok(m) = self.inbox.try_recv() {
            let system = match &m {
                ToJob::Record { flags, .. } if self.display.borrow().is_some() => {
                    crate::display::flag_key(*flags).filter(|k| keys::is_system(*k))
                }
                _ => None,
            };
            match system {
                Some(k) => {
                    if keys::take(self, k) == keys::After::Unwind && keys::cancelling() {
                        keys::cancel_running(job);
                    }
                }
                None => self.pending.borrow_mut().push_back(m),
            }
        }
        self.breaks(job);
    }

    /// Run a line in the storage service's console table.
    pub fn exec(&self, text: &str) -> ExecReply {
        self.link.borrow_mut().with(|l| l.exec(text.to_string()))
    }

    /// The job's attributes, fetched once and after `CHGJOB`.
    pub fn attrs(&self) -> JobAttrs {
        if let Some(a) = self.attrs.borrow().as_ref() {
            return a.clone();
        }
        let a = self.link.borrow_mut().with(Client::rtvjoba);
        *self.attrs.borrow_mut() = Some(a.clone());
        a
    }

    /// Forget the attributes: they changed.
    pub fn attrs_changed(&self) {
        *self.attrs.borrow_mut() = None;
    }

    /// Deliver the breaks waiting for the job: a job at a display shows
    /// them on Display Program Messages, any other writes them out; a
    /// queue with a break-handling program calls it.
    pub fn breaks(&self, job: &mut Job) {
        let waiting = self.link.borrow_mut().with(Client::breaks);
        self.deliver(job, waiting);
    }

    /// Deliver breaks taken from the service.
    pub fn deliver(&self, job: &mut Job, waiting: Vec<(String, (String, String), QMsg)>) {
        let mut shown = Vec::new();
        for (queue, (pgm, lib), m) in waiting {
            if pgm.is_empty() || pgm.starts_with('*') {
                if self.display.borrow().is_some() {
                    shown.push((queue, m));
                    continue;
                }
                let (date, time) = wm::joblog::stamp_of(m.time);
                self.output(&format!(
                    "\nBreak message on {queue} from {} ({date} {}):\n  {}\n",
                    m.job.1,
                    &time[..8],
                    m.text
                ));
                continue;
            }
            // The program gets the queue name, its library and the key,
            // as IBM's break-handling programs do.
            let (qlib, qname) = queue.split_once('/').unwrap_or(("*LIBL", queue.as_str()));
            let mut name = wm::rec::text_bytes(qname, 10);
            let mut library = wm::rec::text_bytes(qlib, 10);
            let mut key = m.key.to_be_bytes().to_vec();
            let args = [
                name.as_mut_ptr() as u64,
                library.as_mut_ptr() as u64,
                key.as_mut_ptr() as u64,
            ];
            let lib = if lib.is_empty() {
                "*LIBL".to_string()
            } else {
                lib
            };
            if let Err(e) = job.call_program(&pgm, &lib, &args) {
                self.output(&format!(
                    "{}: break-handling program {lib}/{pgm} could not be called.\n",
                    e.msgid()
                ));
            }
            let _ = job.clear();
        }
        if !shown.is_empty() {
            crate::sysdsp::msgs::breaks(self, shown);
        }
    }
}
